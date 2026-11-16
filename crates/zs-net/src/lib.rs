//! QUIC transport: endpoint setup, certificate pinning, LAN address discovery.
//!
//! Every session gets a freshly minted self-signed certificate. That sounds
//! backwards, but it is the point: there is no CA to trust, so trust is
//! established per session by the SAS check (see `zs-crypto`). A fresh cert means
//! there is nothing long-lived to steal or to expire.

#![deny(missing_docs)]

pub mod beacon;

use anyhow::{Context, Result};
use quinn::VarInt;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

/// Certificate name we generate and expect. Not a real DNS name; it only has to
/// be consistent between the two sides.
pub const CERT_NAME: &str = "zenshare.local";

/// A running QUIC listener plus the fingerprint of its certificate.
pub struct Listener {
    /// The quinn endpoint.
    pub endpoint: quinn::Endpoint,
    /// BLAKE3 of our certificate DER, for the SAS computation.
    pub cert_fingerprint: [u8; 32],
    /// Actual bound address (useful when binding to port 0).
    pub local_addr: SocketAddr,
}

/// Transport tuning shared by both sides.
fn transport_config() -> Result<Arc<quinn::TransportConfig>> {
    let mut tc = quinn::TransportConfig::default();

    // Plenty of streams: one per chunk, several files in parallel.
    tc.max_concurrent_bidi_streams(VarInt::from_u32(64));
    tc.max_concurrent_uni_streams(VarInt::from_u32(512));

    // 32 MiB connection window, 8 MiB per stream. On a LAN this is what lets a
    // single stream keep a 1 GbE link busy across a 2 ms RTT.
    tc.receive_window(VarInt::from_u32(32 * 1024 * 1024));
    tc.stream_receive_window(VarInt::from_u32(8 * 1024 * 1024));

    // BBR instead of CUBIC. This is the single biggest throughput lever on
    // Wi-Fi, where loss is common and loss-based congestion control collapses.
    tc.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));

    tc.keep_alive_interval(Some(Duration::from_secs(5)));
    tc.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));

    Ok(Arc::new(tc))
}

/// Bind a QUIC listener on `addr`.
pub fn listen(addr: SocketAddr) -> Result<Listener> {
    let cert = rcgen::generate_simple_self_signed(vec![CERT_NAME.to_string()])
        .context("generate self-signed certificate")?;

    let cert_der = cert.cert.der().clone();
    let key_der = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
    let fingerprint = zs_crypto::cert_fingerprint(cert_der.as_ref());

    // ALPN has to be set on both sides; quinn's convenience constructor leaves
    // it empty, which fails the handshake with "no known protocol".
    let provider = rustls::crypto::ring::default_provider();
    let mut crypto = rustls::ServerConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der.into())
        .context("server certificate")?;
    crypto.alpn_protocols = vec![zs_core::ALPN.to_vec()];

    let quic_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)
        .context("QUIC server crypto")?;
    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_crypto));
    server_config.transport_config(transport_config()?);

    let endpoint = quinn::Endpoint::server(server_config, addr)
        .with_context(|| format!("bind QUIC listener on {addr}"))?;
    let local_addr = endpoint.local_addr()?;

    Ok(Listener { endpoint, cert_fingerprint: fingerprint, local_addr })
}

/// Connect to a peer, optionally pinning the certificate we expect.
///
/// `expected` comes from a QR code or a previous trusted session. When it is
/// `None` we still encrypt, but the peer is unverified until the user confirms
/// the SAS digits.
pub async fn connect(
    bind: SocketAddr,
    peer: SocketAddr,
    expected: Option<[u8; 32]>,
) -> Result<quinn::Connection> {
    let mut endpoint = quinn::Endpoint::client(bind).context("bind QUIC client")?;
    endpoint.set_default_client_config(client_config(expected)?);

    let conn = endpoint
        .connect(peer, CERT_NAME)
        .with_context(|| format!("dial {peer}"))?
        .await
        .context("QUIC handshake")?;

    Ok(conn)
}

fn client_config(expected: Option<[u8; 32]>) -> Result<quinn::ClientConfig> {
    let provider = rustls::crypto::ring::default_provider();
    let mut crypto = rustls::ClientConfig::builder_with_provider(Arc::new(provider.clone()))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier { expected }))
        .with_no_client_auth();
    crypto.alpn_protocols = vec![zs_core::ALPN.to_vec()];

    let quic = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?;
    let mut cfg = quinn::ClientConfig::new(Arc::new(quic));
    cfg.transport_config(transport_config()?);
    Ok(cfg)
}

/// Accepts any certificate, but optionally requires a specific fingerprint.
///
/// Accepting any certificate is not a hole here: the certificate is only a key
/// exchange vehicle. Identity is established by the SAS digits the user compares.
/// When we *do* know the fingerprint (QR code, trusted device) we enforce it,
/// which closes the window entirely.
#[derive(Debug)]
struct PinnedVerifier {
    expected: Option<[u8; 32]>,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let Some(expected) = self.expected {
            let actual = zs_crypto::cert_fingerprint(end_entity.as_ref());
            if actual != expected {
                return Err(rustls::Error::General("certificate fingerprint mismatch".into()));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Every non-loopback IPv4 address on this machine, best guess first.
#[must_use]
pub fn local_ips() -> Vec<IpAddr> {
    let mut out = Vec::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            let ip = iface.addr.ip();
            if ip.is_ipv4() {
                out.push(ip);
            }
        }
    }
    out
}

/// The address a peer on the same LAN would use to reach us.
#[must_use]
pub fn primary_lan_ip() -> Option<IpAddr> {
    local_ips().into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_binds_and_reports_fingerprint() {
        let l = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        assert_ne!(l.cert_fingerprint, [0u8; 32]);
        assert_ne!(l.local_addr.port(), 0);
    }
}
