//! ZenShare core: shared types, wire protocol, chunk arithmetic.
//!
//! This crate has no I/O and no crypto. It is the vocabulary every other crate speaks.

#![deny(missing_docs)]

pub mod chunk;
pub mod device;
pub mod error;
pub mod protocol;

pub use chunk::{bitmap, chunk_count, chunk_range, DEFAULT_CHUNK_SIZE};
pub use device::{DeviceId, DeviceInfo, DeviceKind};
pub use error::{CoreError, Result};
pub use protocol::{Control, FileEntry, Manifest, TransferId};

/// ALPN protocol identifier. Bumping this breaks compatibility on purpose.
pub const ALPN: &[u8] = b"zenshare/1";

/// Wire protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

/// Hard ceiling on a single control frame, to stop a hostile peer from
/// making us allocate gigabytes.
pub const MAX_CONTROL_FRAME: usize = 8 * 1024 * 1024;

/// Service type advertised over mDNS.
pub const SERVICE_TYPE: &str = "_zenshare._udp.local.";
