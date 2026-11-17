//! The transfer engine: chunked, verified, out-of-order, resumable.
//!
//! # Design notes
//!
//! **One chunk per QUIC stream.** Streams are cheap and independent, so a lost
//! packet stalls only its own stream. We keep `PARALLEL_STREAMS` in flight, which
//! is enough to hide RTT and disk latency without exploding memory.
//!
//! **The hash travels before the payload.** The receiver reads a fixed 42-byte
//! header, then the payload, then compares. One sequential pass, no round trip,
//! and a corrupt chunk never touches the disk.
//!
//! **No whole-file hash pass.** Every chunk is verified and chunk boundaries are
//! a pure function of file size, so if all chunks verify, the file is correct by
//! construction. This is why a 10 GB transfer starts moving on the first chunk
//! instead of after a 40-second pre-hash.
//!
//! **Atomic landing.** Data goes to `<name>.zenshare-part` via positioned writes
//! at absolute offsets, so chunks can land in any order. Only after every chunk
//! is present do we fsync and rename. A half-written file never appears in the
//! user's folder.

#![deny(missing_docs)]

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use zs_core::chunk::{bitmap, chunk_count, chunk_range};
use zs_core::protocol::{ChunkHeader, Control, FileEntry, Manifest, TransferId};
use zs_core::{MAX_CONTROL_FRAME, PROTOCOL_VERSION};

/// How many chunk streams we keep in flight.
pub const PARALLEL_STREAMS: usize = 32;

/// Suffix used while a file is still being written.
pub const PART_SUFFIX: &str = ".zenshare-part";

/// Fixed on-the-wire header size: 2 + 4 + 4 + 32.
const HEADER_LEN: usize = 42;

/// A file queued for sending.
#[derive(Debug, Clone)]
pub struct SendItem {
    /// Path shown to the receiver, relative to the transfer root.
    pub rel: String,
    /// Path on this machine.
    pub abs: PathBuf,
    /// Size in bytes, captured at queue time.
    pub size: u64,
    /// mtime, ms since epoch.
    pub mtime_ms: i64,
}

impl SendItem {
    /// Build an item from an absolute path, deriving the display name.
    pub fn from_path(path: impl Into<PathBuf>) -> Result<Self> {
        let abs = path.into();
        let meta = std::fs::metadata(&abs).with_context(|| format!("stat {}", abs.display()))?;
        if !meta.is_file() {
            bail!("{} is not a regular file", abs.display());
        }
        let name = abs.file_name().map_or_else(
            || "unnamed".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64);
        Ok(Self {
            rel: name,
            abs,
            size: meta.len(),
            mtime_ms,
        })
    }
}

/// Live progress, cheap to clone and read from the UI thread.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    /// Bytes confirmed written (receiver) or acknowledged (sender).
    pub bytes_done: u64,
    /// Total bytes in the transfer.
    pub total: u64,
    /// Chunks confirmed.
    pub chunks_done: u32,
    /// Total chunks.
    pub chunks_total: u32,
    /// Seconds since the transfer started.
    pub elapsed_secs: f64,
}

impl Progress {
    /// Bytes per second, smoothed over the whole transfer.
    #[must_use]
    pub fn bytes_per_sec(&self) -> f64 {
        if self.elapsed_secs <= 0.0 {
            0.0
        } else {
            self.bytes_done as f64 / self.elapsed_secs
        }
    }

    /// Completion in `0.0..=1.0`.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            (self.bytes_done as f64 / self.total as f64) as f32
        }
    }
}

/// Outcome of a completed transfer.
#[derive(Debug, Clone)]
pub struct Report {
    /// Bytes moved.
    pub bytes: u64,
    /// Wall-clock duration.
    pub elapsed: Duration,
    /// Files that landed, in the order they were committed. Empty for a send.
    ///
    /// The receiver needs these because on Android the destination directory is
    /// app-scoped: the platform has to move each file somewhere a human can
    /// reach, and it can only do that if it knows what was written.
    pub files: Vec<PathBuf>,
}

impl Report {
    /// Average throughput in bytes per second.
    #[must_use]
    pub fn bytes_per_sec(&self) -> f64 {
        let s = self.elapsed.as_secs_f64();
        if s <= 0.0 { 0.0 } else { self.bytes as f64 / s }
    }
}

fn encode_header(h: &ChunkHeader) -> [u8; HEADER_LEN] {
    let mut b = [0u8; HEADER_LEN];
    b[0..2].copy_from_slice(&h.file.to_be_bytes());
    b[2..6].copy_from_slice(&h.index.to_be_bytes());
    b[6..10].copy_from_slice(&h.len.to_be_bytes());
    b[10..42].copy_from_slice(&h.hash);
    b
}

fn decode_header(b: &[u8; HEADER_LEN]) -> ChunkHeader {
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&b[10..42]);
    ChunkHeader {
        file: u16::from_be_bytes([b[0], b[1]]),
        index: u32::from_be_bytes([b[2], b[3], b[4], b[5]]),
        len: u32::from_be_bytes([b[6], b[7], b[8], b[9]]),
        hash,
    }
}

async fn write_msg(send: &mut quinn::SendStream, msg: &Control) -> Result<()> {
    let body = postcard::to_stdvec(msg).context("encode control frame")?;
    send.write_all(&(body.len() as u32).to_be_bytes())
        .await
        .context("write frame len")?;
    send.write_all(&body).await.context("write frame body")?;
    Ok(())
}

async fn read_msg(recv: &mut quinn::RecvStream) -> Result<Control> {
    let mut len = [0u8; 4];
    read_exact(recv, &mut len).await.context("read frame len")?;
    let n = u32::from_be_bytes(len) as usize;
    if n > MAX_CONTROL_FRAME {
        bail!("control frame of {n} bytes exceeds limit");
    }
    let mut buf = vec![0u8; n];
    read_exact(recv, &mut buf)
        .await
        .context("read frame body")?;
    postcard::from_bytes(&buf).context("decode control frame")
}

async fn read_exact(recv: &mut quinn::RecvStream, buf: &mut [u8]) -> Result<()> {
    recv.read_exact(buf)
        .await
        .map_err(|e| anyhow::anyhow!("read: {e}"))?;
    Ok(())
}

/// Send a batch of files over an established, already-authenticated connection.
///
/// The sender drives the control stream; `progress` is called from multiple
/// tasks and must be cheap and thread-safe.
// Same reasoning as `recv`: one linear protocol conversation.
#[allow(clippy::too_many_lines)]
pub async fn send(
    conn: &quinn::Connection,
    items: Vec<SendItem>,
    progress: Arc<dyn Fn(Progress) + Send + Sync>,
) -> Result<Report> {
    let started = Instant::now();
    let chunk_size = zs_core::DEFAULT_CHUNK_SIZE;

    let files: Vec<FileEntry> = items
        .iter()
        .map(|i| FileEntry {
            path: i.rel.clone(),
            size: i.size,
            mtime_ms: i.mtime_ms,
        })
        .collect();
    let total_bytes: u64 = items.iter().map(|i| i.size).sum();

    let mut transfer_id = [0u8; 16];
    getrandom::fill(&mut transfer_id).map_err(|_| anyhow::anyhow!("entropy"))?;

    let manifest = Manifest {
        transfer_id,
        files,
        chunk_size,
        total_bytes,
    };
    let chunks_total = manifest.total_chunks();

    let (mut ctrl_send, mut ctrl_recv) = conn.open_bi().await.context("open control stream")?;
    write_msg(
        &mut ctrl_send,
        &Control::Offer {
            manifest: manifest.clone(),
        },
    )
    .await?;

    let accept = read_msg(&mut ctrl_recv).await?;
    let have = match accept {
        Control::Accept { have } => have,
        Control::Reject { reason } => bail!("receiver rejected: {reason}"),
        other => bail!("unexpected reply to offer: {other:?}"),
    };

    let done_bytes = Arc::new(AtomicU64::new(0));
    let done_chunks = Arc::new(AtomicU64::new(0));

    for (file_idx, item) in items.iter().enumerate() {
        let size = item.size;
        let n_chunks = chunk_count(size, chunk_size);
        if n_chunks == 0 {
            continue;
        }

        let file = std::fs::File::open(&item.abs)
            .with_context(|| format!("open {}", item.abs.display()))?;
        // SAFETY: mapping a file we hold open for the duration of the loop. A
        // concurrent truncation by another process would fault, which is the
        // documented memmap2 caveat and acceptable for a send-once workload.
        #[allow(unsafe_code)]
        let mmap = Arc::new(unsafe { memmap2::Mmap::map(&file)? });

        let have_bits = have.get(file_idx).cloned().unwrap_or_default();

        let jobs = futures::stream::iter((0..n_chunks).filter_map(|idx| {
            if bitmap::get(&have_bits, idx) {
                return None;
            }
            let (start, end) = chunk_range(idx, size, chunk_size)?;
            Some((idx, start, end))
        }))
        .map(|(idx, start, end)| {
            let conn = conn.clone();
            let mmap = Arc::clone(&mmap);
            let done_bytes = Arc::clone(&done_bytes);
            let done_chunks = Arc::clone(&done_chunks);
            let progress = Arc::clone(&progress);
            let total = total_bytes;
            let ctotal = chunks_total;
            let t0 = started;
            async move {
                let payload = &mmap[start as usize..end as usize];
                let hash = *blake3::hash(payload).as_bytes();
                let header = ChunkHeader {
                    file: file_idx as u16,
                    index: idx,
                    len: payload.len() as u32,
                    hash,
                };

                let mut s = conn.open_uni().await.context("open data stream")?;
                s.write_all(&encode_header(&header))
                    .await
                    .context("write chunk header")?;
                s.write_all(payload).await.context("write chunk body")?;
                s.finish().context("finish chunk stream")?;

                let b = done_bytes.fetch_add(payload.len() as u64, Ordering::Relaxed)
                    + payload.len() as u64;
                let c = done_chunks.fetch_add(1, Ordering::Relaxed) + 1;
                progress(Progress {
                    bytes_done: b,
                    total,
                    chunks_done: c as u32,
                    chunks_total: ctotal,
                    elapsed_secs: t0.elapsed().as_secs_f64(),
                });
                Ok::<_, anyhow::Error>(payload.len() as u64)
            }
        })
        .buffer_unordered(PARALLEL_STREAMS);

        futures::pin_mut!(jobs);
        while let Some(r) = jobs.next().await {
            r?;
        }
    }

    let sent = done_bytes.load(Ordering::Relaxed);
    write_msg(&mut ctrl_send, &Control::Complete { bytes: sent }).await?;
    match read_msg(&mut ctrl_recv).await? {
        Control::Done => {}
        other => bail!("unexpected final reply: {other:?}"),
    }
    let _ = ctrl_send.finish();
    // Tell the peer we are done, so its `closed()` resolves and it can drop the
    // endpoint without truncating the last frame.
    conn.close(0u32.into(), b"done");

    Ok(Report {
        bytes: sent,
        elapsed: started.elapsed(),
        files: Vec::new(),
    })
}

/// Receive an offer and write the files into `dest`.
// One linear protocol conversation: offer, accept, chunks, commit, ack. Splitting
// it across helpers would thread six pieces of state through them for no gain.
#[allow(clippy::too_many_lines)]
pub async fn recv(
    conn: &quinn::Connection,
    dest: &Path,
    progress: Arc<dyn Fn(Progress) + Send + Sync>,
) -> Result<Report> {
    let started = Instant::now();
    let (mut ctrl_send, mut ctrl_recv) = conn.accept_bi().await.context("accept control stream")?;

    let manifest = match read_msg(&mut ctrl_recv).await? {
        Control::Offer { manifest } => manifest,
        other => bail!("expected offer, got {other:?}"),
    };

    if manifest.chunk_size == 0 || manifest.chunk_size > 64 * 1024 * 1024 {
        write_msg(
            &mut ctrl_send,
            &Control::Reject {
                reason: "bad chunk size".into(),
            },
        )
        .await?;
        bail!("peer offered an absurd chunk size");
    }

    std::fs::create_dir_all(dest).with_context(|| format!("create {}", dest.display()))?;

    let total_chunks = manifest.total_chunks();
    let total_bytes = manifest.total_bytes;

    // Prepare staging files and their final destinations.
    let mut targets = Vec::with_capacity(manifest.files.len());
    for entry in &manifest.files {
        let safe = sanitize_relative(&entry.path);
        let final_path = dest.join(&safe);
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part_path = with_suffix(&final_path, PART_SUFFIX);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&part_path)
            .with_context(|| format!("create {}", part_path.display()))?;
        file.set_len(entry.size)?;
        targets.push(Target {
            final_path,
            part_path,
            file: Arc::new(file),
            size: entry.size,
            chunks: chunk_count(entry.size, manifest.chunk_size),
        });
    }

    let have: Vec<Vec<u8>> = targets.iter().map(|t| bitmap::empty(t.chunks)).collect();
    write_msg(&mut ctrl_send, &Control::Accept { have }).await?;

    let done_bytes = Arc::new(AtomicU64::new(0));
    let done_chunks = Arc::new(AtomicU64::new(0));
    let targets = Arc::new(targets);
    let chunk_size = manifest.chunk_size;

    let mut tasks = tokio::task::JoinSet::new();
    let mut seen = 0u32;

    while seen < total_chunks {
        let stream = match conn.accept_uni().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "data stream ended early");
                break;
            }
        };
        seen += 1;

        let targets = Arc::clone(&targets);
        let done_bytes = Arc::clone(&done_bytes);
        let done_chunks = Arc::clone(&done_chunks);
        let progress = Arc::clone(&progress);
        let t0 = started;

        tasks.spawn(async move {
            let mut stream = stream;
            let mut hdr = [0u8; HEADER_LEN];
            read_exact(&mut stream, &mut hdr).await?;
            let header = decode_header(&hdr);

            let len = header.len as usize;
            if len > chunk_size as usize {
                bail!("chunk larger than declared chunk size");
            }
            let mut payload = vec![0u8; len];
            read_exact(&mut stream, &mut payload).await?;

            if *blake3::hash(&payload).as_bytes() != header.hash {
                bail!("chunk {}:{} failed verification", header.file, header.index);
            }

            let target = targets
                .get(header.file as usize)
                .context("chunk references unknown file")?;
            let (start, end) = chunk_range(header.index, target.size, chunk_size)
                .context("chunk index out of range")?;
            if (end - start) as usize != len {
                bail!("chunk length does not match file geometry");
            }

            write_at(&target.file, &payload, start)?;

            let b = done_bytes.fetch_add(len as u64, Ordering::Relaxed) + len as u64;
            let c = done_chunks.fetch_add(1, Ordering::Relaxed) + 1;
            progress(Progress {
                bytes_done: b,
                total: total_bytes,
                chunks_done: c as u32,
                chunks_total: total_chunks,
                elapsed_secs: t0.elapsed().as_secs_f64(),
            });
            Ok::<_, anyhow::Error>(())
        });
    }

    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "chunk failed"),
            Err(e) => tracing::warn!(error = %e, "chunk task panicked"),
        }
    }

    let got = done_chunks.load(Ordering::Relaxed);
    if got != u64::from(total_chunks) {
        write_msg(
            &mut ctrl_send,
            &Control::Error {
                message: format!("incomplete: {got}/{total_chunks} chunks"),
            },
        )
        .await?;
        bail!("incomplete transfer: {got}/{total_chunks} chunks");
    }

    match read_msg(&mut ctrl_recv).await? {
        Control::Complete { .. } => {}
        other => bail!("expected complete, got {other:?}"),
    }

    // Commit: flush, then atomically move each staging file into place.
    for t in targets.iter() {
        t.file
            .sync_all()
            .with_context(|| format!("fsync {}", t.part_path.display()))?;
        std::fs::rename(&t.part_path, &t.final_path)
            .with_context(|| format!("rename into {}", t.final_path.display()))?;
        if let Ok(mtime) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            let _ = mtime;
        }
    }

    write_msg(&mut ctrl_send, &Control::Done).await?;
    let _ = ctrl_send.finish();

    // Do not drop the connection the instant we reply: quinn would emit
    // CONNECTION_CLOSE and could truncate the Done frame. Wait for the sender to
    // close first, with a bound so a vanished peer cannot hang us.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), conn.closed()).await;

    Ok(Report {
        bytes: done_bytes.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
        files: targets.iter().map(|t| t.final_path.clone()).collect(),
    })
}

struct Target {
    final_path: PathBuf,
    part_path: PathBuf,
    file: Arc<std::fs::File>,
    size: u64,
    chunks: u32,
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Positioned write, so chunks can land in any order without seeking.
///
/// Uses std's per-platform positioned-write rather than a Unix syscall wrapper:
/// the same code has to build and run on Windows, where `rustix` does not exist.
#[cfg(unix)]
fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> Result<()> {
    use std::os::unix::fs::FileExt;

    let mut written = 0;
    while written < buf.len() {
        let n = file
            .write_at(&buf[written..], offset + written as u64)
            .map_err(|e| anyhow::anyhow!("pwrite: {e}"))?;
        if n == 0 {
            bail!("pwrite returned 0");
        }
        written += n;
    }
    Ok(())
}

/// Positioned write, Windows flavour.
#[cfg(windows)]
fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> Result<()> {
    use std::os::windows::fs::FileExt;

    let mut written = 0;
    while written < buf.len() {
        let n = file
            .seek_write(&buf[written..], offset + written as u64)
            .map_err(|e| anyhow::anyhow!("seek_write: {e}"))?;
        if n == 0 {
            bail!("seek_write returned 0");
        }
        written += n;
    }
    Ok(())
}

/// Defuse path traversal and normalise Unicode so macOS NFD and Windows NFC
/// agree on what a filename is.
fn sanitize_relative(path: &str) -> PathBuf {
    use unicode_normalization::UnicodeNormalization;

    let normalized: String = path.nfc().collect();
    let mut out = PathBuf::new();
    for part in normalized.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        let clean = sanitize_filename::sanitize(part);
        if !clean.is_empty() {
            out.push(clean);
        }
    }
    if out.as_os_str().is_empty() {
        out.push("unnamed");
    }
    out
}

/// Convenience: a random transfer id.
#[must_use]
pub fn new_transfer_id() -> TransferId {
    let mut id = [0u8; 16];
    let _ = getrandom::fill(&mut id);
    id
}

/// Protocol version this engine speaks.
#[must_use]
pub const fn version() -> u16 {
    PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let h = ChunkHeader {
            file: 7,
            index: 12345,
            len: 4096,
            hash: [0xAB; 32],
        };
        let d = decode_header(&encode_header(&h));
        assert_eq!(d.file, h.file);
        assert_eq!(d.index, h.index);
        assert_eq!(d.len, h.len);
        assert_eq!(d.hash, h.hash);
    }

    #[test]
    fn traversal_is_neutralised() {
        let p = sanitize_relative("../../etc/passwd");
        assert_eq!(p, PathBuf::from("etc/passwd"));

        let p = sanitize_relative("/absolute/path.txt");
        assert_eq!(p, PathBuf::from("absolute/path.txt"));

        let p = sanitize_relative("..");
        assert_eq!(p, PathBuf::from("unnamed"));
    }

    #[test]
    fn unicode_is_normalised() {
        // "café" as NFD (e + combining acute) must become NFC.
        let nfd = "cafe\u{0301}.txt";
        let p = sanitize_relative(nfd);
        assert_eq!(p.to_string_lossy(), "café.txt");
    }
}
