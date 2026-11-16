//! Device identity types.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A device's long-term public key, used as its stable identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceId(pub [u8; 32]);

impl DeviceId {
    /// Short human-readable form, e.g. `a3f1-9c02`. Stable and unique enough to
    /// show in the UI; not a security boundary.
    #[must_use]
    pub fn short(&self) -> String {
        format!(
            "{:02x}{:02x}-{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self.short())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

/// Coarse device class, used only to pick an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceKind {
    /// Desktop machine.
    Desktop,
    /// Laptop.
    Laptop,
    /// Phone.
    Phone,
    /// Tablet.
    Tablet,
    /// Headless server or NAS.
    Server,
    /// Anything else.
    Unknown,
}

impl DeviceKind {
    /// Guess a kind from the OS string. Wrong guesses only cost an icon.
    #[must_use]
    pub fn guess(os: &str) -> Self {
        let os = os.to_ascii_lowercase();
        if os.contains("android") || os.contains("ios") {
            Self::Phone
        } else if os.contains("mac") || os.contains("darwin") {
            Self::Laptop
        } else if os.contains("windows") || os.contains("linux") {
            Self::Desktop
        } else {
            Self::Unknown
        }
    }
}

/// What a peer tells us about itself during the handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Stable identity.
    pub id: DeviceId,
    /// Display name, user-editable.
    pub name: String,
    /// Icon hint.
    pub kind: DeviceKind,
    /// Free-form platform string, e.g. `macos 15.1`.
    pub platform: String,
}

impl DeviceInfo {
    /// This machine's info.
    #[must_use]
    pub fn local(id: DeviceId, name: impl Into<String>) -> Self {
        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        Self {
            id,
            name: name.into(),
            kind: DeviceKind::guess(std::env::consts::OS),
            platform,
        }
    }
}
