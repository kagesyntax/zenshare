//! Peer discovery over UDP multicast.
//!
//! Deliberately not mDNS. mDNS is the right *standard*, but it brings a full DNS
//! parser, a responder, and a 200 ms probe/announce dance before the first
//! packet moves. For a closed protocol between two copies of the same app, a
//! 40-byte multicast datagram every two seconds is simpler, has no spoofable
//! name resolution, and shows peers instantly.
//!
//! Discovery is never trust. Anyone can send this datagram. Identity is proven
//! later by the SAS digits (see `zs-crypto`).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use zs_core::DeviceInfo;

/// Multicast group used for announcements. Admin-scoped, so it stays on the LAN.
pub const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);

/// Fixed discovery port.
pub const PORT: u16 = 47821;

/// How often we announce ourselves.
pub const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(2);

/// What a peer broadcasts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announce {
    /// Protocol version, so we can ignore incompatible peers instead of failing.
    pub version: u16,
    /// Who is speaking.
    pub device: DeviceInfo,
    /// The QUIC port they are listening on.
    pub port: u16,
}

/// A peer we heard from, with the address it spoke from.
#[derive(Debug, Clone)]
pub struct Heard {
    /// Parsed announcement.
    pub announce: Announce,
    /// Source address, with the port replaced by the advertised QUIC port.
    pub quic_addr: SocketAddr,
}

fn shared_socket() -> Result<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    #[cfg(unix)]
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;

    // Join the group. INADDR_ANY asks the OS to pick an interface; on machines
    // with no multicast-capable uplink (or behind a restrictive firewall) that
    // fails, so fall back to the loopback interface. Discovery degrading is not
    // a reason to refuse to run: manual addressing still works.
    let joined = sock
        .join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)
        .or_else(|_| sock.join_multicast_v4(&GROUP, &Ipv4Addr::LOCALHOST));

    if let Err(e) = joined {
        tracing::warn!(error = %e, "multicast unavailable; peers must be addressed manually");
    }

    sock.set_multicast_loop_v4(true)?;
    // Stay on the local link: never routed off this network.
    sock.set_multicast_ttl_v4(1)?;

    Ok(sock.into())
}

/// Start announcing and listening. Returns a receiver of everything heard.
///
/// The returned task runs until the process exits; drop the receiver to let it
/// wind down.
pub fn spawn(device: DeviceInfo, quic_port: u16) -> Result<mpsc::Receiver<Heard>> {
    let std_sock = shared_socket()?;
    let sock = std::sync::Arc::new(UdpSocket::from_std(std_sock)?);
    let (tx, rx) = mpsc::channel(64);

    let announce = Announce {
        version: zs_core::PROTOCOL_VERSION,
        device: device.clone(),
        port: quic_port,
    };
    let payload = postcard::to_stdvec(&announce)?;

    // Sender.
    {
        let sock = std::sync::Arc::clone(&sock);
        tokio::spawn(async move {
            let group = SocketAddr::V4(SocketAddrV4::new(GROUP, PORT));
            // Loopback unicast as well as the group: some hosts (and most
            // sandboxes) silently drop multicast, and two instances on one
            // machine should still see each other.
            let local = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT));
            let mut ticker = tokio::time::interval(ANNOUNCE_INTERVAL);
            loop {
                ticker.tick().await;
                if let Err(e) = sock.send_to(&payload, group).await {
                    tracing::debug!(error = %e, "multicast announce failed");
                }
                let _ = sock.send_to(&payload, local).await;
            }
        });
    }

    // Receiver.
    {
        let sock = std::sync::Arc::clone(&sock);
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                    continue;
                };
                let Ok(announce) = postcard::from_bytes::<Announce>(&buf[..n]) else {
                    continue;
                };
                // Ignore ourselves.
                if announce.device.id == device.id {
                    continue;
                }
                if announce.version != zs_core::PROTOCOL_VERSION {
                    continue;
                }
                let quic_addr = SocketAddr::new(from.ip(), announce.port);
                if tx.send(Heard { announce, quic_addr }).await.is_err() {
                    return;
                }
            }
        });
    }

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_peers_see_each_other() {
        let a = DeviceInfo::local(zs_core::DeviceId([1u8; 32]), "alpha");
        let b = DeviceInfo::local(zs_core::DeviceId([2u8; 32]), "beta");

        let mut rx_a = spawn(a, 1000).unwrap();
        let _rx_b = spawn(b, 2000).unwrap();

        let heard = tokio::time::timeout(Duration::from_secs(6), rx_a.recv())
            .await
            .expect("timed out waiting for an announcement")
            .expect("channel closed");

        assert_eq!(heard.announce.device.name, "beta");
        assert_eq!(heard.quic_addr.port(), 2000);
    }
}
