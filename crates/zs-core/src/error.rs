//! Errors.

/// Result alias used across the workspace.
pub type Result<T> = std::result::Result<T, CoreError>;

/// Everything that can go wrong in the protocol layer.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A control frame exceeded [`crate::MAX_CONTROL_FRAME`].
    #[error("control frame too large: {0} bytes")]
    FrameTooLarge(usize),

    /// Peer announced a protocol version we do not speak.
    #[error("protocol mismatch: peer speaks v{peer}, we speak v{ours}")]
    VersionMismatch {
        /// Peer's version.
        peer: u16,
        /// Ours.
        ours: u16,
    },

    /// Chunk index outside the file.
    #[error("chunk {index} out of range for file of {size} bytes")]
    ChunkOutOfRange {
        /// Requested index.
        index: u32,
        /// File size.
        size: u64,
    },

    /// A chunk arrived with the wrong hash.
    #[error("chunk {file}:{index} failed verification")]
    ChunkCorrupt {
        /// File index.
        file: u16,
        /// Chunk index.
        index: u32,
    },

    /// Encoding or decoding failed.
    #[error("codec: {0}")]
    Codec(String),
}
