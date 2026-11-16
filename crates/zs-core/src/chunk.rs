//! Chunk arithmetic and resume bitmaps.
//!
//! Chunking is per-file and fixed-size, so a chunk's byte range is a pure
//! function of its index. That is what makes out-of-order transfer, resume and
//! per-chunk verification all work without any extra bookkeeping.

/// Default chunk size: 4 MiB. Large enough that syscall overhead disappears,
/// small enough that a retry costs little and a 4 MiB buffer fits comfortably
/// in L2/L3 on every target.
pub const DEFAULT_CHUNK_SIZE: u32 = 4 * 1024 * 1024;

/// Number of chunks a file of `size` bytes needs.
#[must_use]
pub const fn chunk_count(size: u64, chunk_size: u32) -> u32 {
    if size == 0 {
        return 0;
    }
    let c = chunk_size as u64;
    size.div_ceil(c) as u32
}

/// Byte range of chunk `index` within a file of `size` bytes.
///
/// Returns `None` if the index is out of range, so a hostile peer cannot make us
/// seek past the end.
#[must_use]
pub const fn chunk_range(index: u32, size: u64, chunk_size: u32) -> Option<(u64, u64)> {
    let start = index as u64 * chunk_size as u64;
    if start >= size {
        return None;
    }
    let end = if start + chunk_size as u64 > size {
        size
    } else {
        start + chunk_size as u64
    };
    Some((start, end))
}

/// Resume bitmap helpers. Bit set = chunk present.
pub mod bitmap {
    /// Bytes needed to hold `chunks` bits.
    #[must_use]
    pub const fn len(chunks: u32) -> usize {
        (chunks as usize).div_ceil(8)
    }

    /// Is chunk `i` present?
    #[must_use]
    pub fn get(bits: &[u8], i: u32) -> bool {
        let byte = (i / 8) as usize;
        bits.get(byte).is_some_and(|b| b & (1 << (i % 8)) != 0)
    }

    /// Mark chunk `i` present. Out-of-range indices are ignored rather than
    /// panicking, because the bitmap may come from an older partial transfer.
    pub fn set(bits: &mut [u8], i: u32) {
        let byte = (i / 8) as usize;
        if let Some(b) = bits.get_mut(byte) {
            *b |= 1 << (i % 8);
        }
    }

    /// Count set bits.
    #[must_use]
    pub fn count(bits: &[u8]) -> u32 {
        bits.iter().map(|b| b.count_ones()).sum()
    }

    /// An all-zero bitmap of the right size.
    #[must_use]
    pub fn empty(chunks: u32) -> Vec<u8> {
        vec![0u8; len(chunks)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_ranges() {
        assert_eq!(chunk_count(0, 4), 0);
        assert_eq!(chunk_count(1, 4), 1);
        assert_eq!(chunk_count(4, 4), 1);
        assert_eq!(chunk_count(5, 4), 2);
        assert_eq!(chunk_range(0, 9, 4), Some((0, 4)));
        assert_eq!(chunk_range(1, 9, 4), Some((4, 8)));
        assert_eq!(chunk_range(2, 9, 4), Some((8, 9)));
        assert_eq!(chunk_range(3, 9, 4), None);
    }

    #[test]
    fn ranges_tile_the_file_exactly() {
        let size = 10_000_007;
        let n = chunk_count(size, 65536);
        let mut covered = 0u64;
        for i in 0..n {
            let (a, b) = chunk_range(i, size, 65536).unwrap();
            assert_eq!(a, covered);
            covered = b;
        }
        assert_eq!(covered, size);
    }

    #[test]
    fn bitmap_roundtrip() {
        let mut b = bitmap::empty(20);
        bitmap::set(&mut b, 0);
        bitmap::set(&mut b, 13);
        bitmap::set(&mut b, 19);
        bitmap::set(&mut b, 999);
        assert!(bitmap::get(&b, 0));
        assert!(!bitmap::get(&b, 1));
        assert!(bitmap::get(&b, 13));
        assert!(bitmap::get(&b, 19));
        assert_eq!(bitmap::count(&b), 3);
    }
}
