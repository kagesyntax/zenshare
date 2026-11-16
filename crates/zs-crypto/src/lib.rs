//! Device identity and the short authentication string.
//!
//! # Threat model
//!
//! mDNS is unauthenticated: anyone on the network can advertise a device that
//! claims to be your laptop. TLS alone does not fix this, because there is no CA
//! and the attacker can present their own self-signed certificate.
//!
//! The fix is a **short authentication string (SAS)**: both sides derive six
//! digits from their TLS certificate fingerprints plus both nonces, and the user
//! confirms the digits match. A man in the middle necessarily holds two
//! different certificates — one facing each side — so the two sides compute
//! different digits. Six digits gives a 1-in-a-million chance of a blind
//! collision, and the attacker gets exactly one attempt per session.
//!
//! This is the same idea as Signal's safety numbers, reduced to something a
//! human will actually read out loud.

#![deny(missing_docs)]

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use std::path::Path;
use zeroize::Zeroize;
use zs_core::DeviceId;

/// Errors from the identity layer.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// The OS random number generator failed.
    #[error("entropy source failed")]
    Entropy,
    /// A key file was the wrong length or corrupt.
    #[error("malformed key material")]
    MalformedKey,
    /// Filesystem failure while loading or saving an identity.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, CryptoError>;

/// A device's long-term Ed25519 identity.
///
/// The secret is zeroized on drop. This is the one piece of state that, if
/// stolen, lets someone impersonate you to peers that already trust you.
pub struct Identity {
    signing: SigningKey,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("id", &self.device_id())
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// Generate a fresh identity from OS entropy.
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|_| CryptoError::Entropy)?;
        let id = Self::from_seed(&seed);
        seed.zeroize();
        Ok(id)
    }

    /// Rebuild from a 32-byte seed.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(seed),
        }
    }

    /// Export the seed. Caller is responsible for wiping the result.
    #[must_use]
    pub fn seed(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// This device's stable identifier.
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        DeviceId(self.signing.verifying_key().to_bytes())
    }

    /// Raw public key bytes.
    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// Sign a message.
    #[must_use]
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.signing.sign(msg).to_bytes()
    }

    /// Verify a signature against a public key.
    #[must_use]
    pub fn verify(public: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
        let Ok(vk) = VerifyingKey::from_bytes(public) else {
            return false;
        };
        let sig = ed25519_dalek::Signature::from_bytes(sig);
        vk.verify_strict(msg, &sig).is_ok()
    }
}

/// Load an identity from `path`, creating one on first run.
///
/// The file is created with mode 0600 on Unix. On Windows we rely on the ACL of
/// the per-user app-data directory, which is already user-scoped.
pub fn load_or_create(path: &Path) -> Result<Identity> {
    if let Ok(bytes) = std::fs::read(path) {
        if bytes.len() == 32 {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            let id = Identity::from_seed(&seed);
            seed.zeroize();
            return Ok(id);
        }
        return Err(CryptoError::MalformedKey);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let identity = Identity::generate()?;
    let mut seed = identity.seed();
    std::fs::write(path, seed)?;
    seed.zeroize();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }

    Ok(identity)
}

/// Short hex fingerprint of arbitrary bytes, for display.
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> String {
    let h = blake3::hash(bytes);
    hex::encode(&h.as_bytes()[..8])
}

/// BLAKE3 of a certificate's DER encoding — the value we pin.
#[must_use]
pub fn cert_fingerprint(der: &[u8]) -> [u8; 32] {
    *blake3::hash(der).as_bytes()
}

/// Derive the six-digit short authentication string.
///
/// Both peers must feed the same two certificate fingerprints in the same
/// order. We sort them so neither side has to agree on who goes first, and mix
/// in both nonces so a recorded session cannot be replayed at a user.
#[must_use]
pub fn sas_code(
    cert_a: &[u8; 32],
    cert_b: &[u8; 32],
    nonce_a: &[u8; 32],
    nonce_b: &[u8; 32],
) -> u32 {
    let (lo, hi) = if cert_a <= cert_b {
        (cert_a, cert_b)
    } else {
        (cert_b, cert_a)
    };
    let (nlo, nhi) = if nonce_a <= nonce_b {
        (nonce_a, nonce_b)
    } else {
        (nonce_b, nonce_a)
    };

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"zenshare-sas-v1");
    hasher.update(lo);
    hasher.update(hi);
    hasher.update(nlo);
    hasher.update(nhi);
    let digest = hasher.finalize();

    let mut eight = [0u8; 8];
    eight.copy_from_slice(&digest.as_bytes()[..8]);
    (u64::from_be_bytes(eight) % 1_000_000) as u32
}

/// Format a SAS code as `123 456`.
#[must_use]
pub fn format_sas(code: u32) -> String {
    format!("{:03} {:03}", code / 1000, code % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_roundtrip() {
        let a = Identity::generate().unwrap();
        let seed = a.seed();
        let b = Identity::from_seed(&seed);
        assert_eq!(a.device_id(), b.device_id());

        let msg = b"hello";
        let sig = a.sign(msg);
        assert!(Identity::verify(&a.public_key(), msg, &sig));
        assert!(!Identity::verify(&a.public_key(), b"tampered", &sig));
    }

    #[test]
    fn sas_is_order_independent() {
        let ca = [1u8; 32];
        let cb = [2u8; 32];
        let na = [3u8; 32];
        let nb = [4u8; 32];
        assert_eq!(sas_code(&ca, &cb, &na, &nb), sas_code(&cb, &ca, &nb, &na));
    }

    #[test]
    fn sas_detects_a_different_certificate() {
        // A MITM presents different certs to each side, so each side hashes a
        // different pair and the digits diverge. That is the whole security
        // property, so it gets a test.
        let real = [9u8; 32];
        let fake_a = [7u8; 32];
        let fake_b = [8u8; 32];
        let na = [1u8; 32];
        let nb = [2u8; 32];

        let side_a = sas_code(&fake_a, &real, &na, &nb);
        let side_b = sas_code(&real, &fake_b, &na, &nb);
        assert_ne!(side_a, side_b);
    }

    #[test]
    fn sas_is_six_digits() {
        for i in 0..1000u32 {
            let mut a = [0u8; 32];
            a[..4].copy_from_slice(&i.to_be_bytes());
            assert!(sas_code(&a, &[0u8; 32], &[0u8; 32], &[0u8; 32]) < 1_000_000);
        }
    }
}
