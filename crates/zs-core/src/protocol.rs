//! Wire protocol.
//!
//! Control messages travel on one bidirectional stream, length-prefixed with a
//! `u32` and encoded with postcard. File payloads travel on many unidirectional
//! streams, each carrying exactly one chunk.
//!
//! Why one chunk per stream: QUIC streams are cheap and independent, so a lost
//! packet only stalls the stream it belongs to. That is the whole reason we are
//! not on TCP.

use crate::device::DeviceInfo;
use serde::{Deserialize, Serialize};

/// Identifies a transfer within a session. Random, 128-bit.
pub type TransferId = [u8; 16];

/// One file in an offer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path relative to the transfer root. Already sanitised by the sender;
    /// the receiver sanitises again, because trust is not transitive.
    pub path: String,
    /// Exact size in bytes.
    pub size: u64,
    /// Modification time, ms since epoch. Preserved on receive.
    pub mtime_ms: i64,
}

/// A complete offer: what the sender wants to put on the receiver's disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Session-scoped identifier.
    pub transfer_id: TransferId,
    /// Files, in display order.
    pub files: Vec<FileEntry>,
    /// Chunk size used for every file in this transfer.
    pub chunk_size: u32,
    /// Sum of all file sizes, so the receiver can check free space up front.
    pub total_bytes: u64,
}

impl Manifest {
    /// Total number of chunks across every file.
    #[must_use]
    pub fn total_chunks(&self) -> u32 {
        self.files
            .iter()
            .map(|f| crate::chunk::chunk_count(f.size, self.chunk_size))
            .sum()
    }
}

/// Header written at the start of every chunk stream.
///
/// The hash comes *before* the payload so the receiver can verify with a single
/// sequential read and no round trip.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChunkHeader {
    /// Index into `Manifest::files`.
    pub file: u16,
    /// Chunk index within that file.
    pub index: u32,
    /// Payload length in bytes.
    pub len: u32,
    /// BLAKE3 of the payload.
    pub hash: [u8; 32],
}

/// Control-plane messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Control {
    /// First message from the initiator.
    Hello {
        /// Protocol version.
        version: u16,
        /// Sender's identity.
        device: DeviceInfo,
        /// Random nonce, mixed into the short authentication string.
        nonce: [u8; 32],
    },
    /// What is being sent.
    Offer {
        /// The manifest.
        manifest: Manifest,
    },
    /// Receiver agrees. Carries per-file resume bitmaps (bit set = already have).
    Accept {
        /// One bitmap per file, in manifest order.
        have: Vec<Vec<u8>>,
    },
    /// Receiver declines.
    Reject {
        /// Human-readable reason.
        reason: String,
    },
    /// All chunks have been written; receiver should fsync and rename.
    Complete {
        /// Bytes actually transferred.
        bytes: u64,
    },
    /// Receiver confirms the files are in place.
    Done,
    /// Terminal error.
    Error {
        /// Human-readable reason.
        message: String,
    },
}
