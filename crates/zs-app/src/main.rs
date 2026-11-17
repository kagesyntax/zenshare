//! ZenShare desktop entry point.
//!
//! All the interesting parts live in the library so the Android build can reuse
//! them; this file is argument parsing and process setup.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use zs_app::engine;
use zs_app::paths::AppPaths;
use zs_transfer::{Progress, SendItem};
use zs_ui::UiModel;

#[derive(Parser, Debug)]
#[command(
    name = "zenshare",
    version,
    about = "Fast, private, cross-platform file transfer"
)]
struct Cli {
    /// Use a separate identity and settings profile. Handy for testing two
    /// devices on one machine.
    #[arg(long, global = true, default_value = "")]
    profile: String,

    /// Address to listen on. Use 127.0.0.1 to stay off the network entirely.
    #[arg(long, global = true, default_value = "0.0.0.0")]
    bind: String,

    /// Keep the window above all others.
    #[arg(long, global = true)]
    on_top: bool,

    /// Render a populated window with sample data, for design work.
    #[arg(long, global = true)]
    demo: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Open the window (default).
    Gui,
    /// Check the environment and report what is available.
    Doctor,
    /// Listen for nearby devices and print what is heard.
    ///
    /// Exists so discovery can be tested between two real processes instead of
    /// being assumed to work. Run two of these and they should find each other.
    Peers {
        /// How long to listen, in seconds.
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        /// Name to announce as. Defaults to the hostname.
        #[arg(long)]
        name: Option<String>,
    },
    /// Send files to a running `ZenShare` without opening a window.
    Send {
        /// Files to send.
        paths: Vec<PathBuf>,
        /// Destination, as `ip:port`.
        #[arg(long)]
        to: String,
    },
    /// Move a file through a real QUIC connection and verify it.
    Selftest {
        /// Size of the generated payload, in MiB.
        #[arg(long, default_value_t = 256)]
        size_mb: u64,
        /// Keep the received copy instead of deleting it.
        #[arg(long)]
        keep: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Gui) {
        Command::Gui => gui(&cli.profile, &cli.bind, cli.on_top, cli.demo),
        Command::Doctor => doctor(&cli.profile),
        Command::Peers { seconds, name } => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(peers(seconds, name)),
        Command::Send { paths, to } => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(send_files(paths, &to)),
        Command::Selftest { size_mb, keep } => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?
            .block_on(selftest(size_mb, keep)),
    }
}

fn gui(profile: &str, bind: &str, on_top: bool, demo: bool) -> Result<()> {
    let initial = if demo {
        UiModel::demo()
    } else {
        UiModel::default()
    };
    let model = Arc::new(RwLock::new(initial));
    let (tx, rx) = async_channel::unbounded();

    if !demo {
        let bind_ip: std::net::Ipv4Addr = bind.parse().context("--bind must be an IPv4 address")?;
        let paths = Arc::new(AppPaths::desktop(profile)?);
        engine::spawn(Arc::clone(&model), rx, bind_ip, paths)?;
    }

    zs_ui::run(model, tx, on_top, demo).map_err(|e| anyhow::anyhow!("window: {e}"))?;
    Ok(())
}

async fn peers(seconds: u64, name: Option<String>) -> Result<()> {
    // Without a subscriber, `tracing::warn!` from the beacon goes nowhere and a
    // failed multicast join looks identical to a working one.
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .try_init();

    let name = name.unwrap_or_else(|| {
        std::env::var("USER").map_or_else(|_| "device".to_string(), |u| format!("{u}-device"))
    });

    // Deterministic per name, so two processes with different --name values are
    // different devices and do not filter each other out as "self".
    let id = zs_core::DeviceId(*blake3::hash(name.as_bytes()).as_bytes());
    let device = zs_core::DeviceInfo::local(id, name.clone());

    let mut rx = zs_net::beacon::spawn(&device, 47820)?;
    println!("announcing as {name}; listening for {seconds}s");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds);
    loop {
        tokio::select! {
            Some(heard) = rx.recv() => {
                println!("HEARD {} at {}", heard.announce.device.name, heard.quic_addr);
            }
            () = tokio::time::sleep_until(deadline) => break,
        }
    }

    println!("done");
    Ok(())
}

fn doctor(profile: &str) -> Result<()> {
    println!("zenshare {}", env!("CARGO_PKG_VERSION"));
    println!("  protocol      v{}", zs_transfer::version());
    println!(
        "  chunk size    {}",
        bytesize::ByteSize(u64::from(zs_core::DEFAULT_CHUNK_SIZE))
    );
    println!("  streams       {}", zs_transfer::PARALLEL_STREAMS);

    let paths = AppPaths::desktop(profile)?;
    paths.ensure()?;
    let identity = zs_crypto::load_or_create(&paths.identity)?;
    println!(
        "  identity      {}  ({})",
        identity.device_id(),
        paths.identity.display()
    );
    println!("  downloads     {}", paths.downloads.display());

    let ips = zs_net::local_ips();
    if ips.is_empty() {
        println!("  interfaces    none — you are offline");
    } else {
        for ip in ips {
            println!("  interface     {ip}");
        }
    }

    // Binding a QUIC endpoint needs a Tokio reactor even though doctor never
    // awaits anything, so give it one rather than pretending it is optional.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let addr: std::net::SocketAddr = "0.0.0.0:0".parse()?;

    match runtime.block_on(async { zs_net::listen(addr) }) {
        Ok(l) => println!("  quic          ok, bound {}", l.local_addr),
        Err(e) => {
            // The context line alone ("bind QUIC listener") tells the user
            // nothing actionable, so print the whole chain.
            println!("  quic          FAILED");
            for cause in e.chain() {
                println!("                  {cause}");
            }
            // Exit non-zero. A diagnostic command that reports a broken
            // environment and then exits 0 is worse than no command at all:
            // CI reads the exit code, not the prose.
            anyhow::bail!("QUIC listener could not bind");
        }
    }
    Ok(())
}

async fn send_files(paths: Vec<PathBuf>, to: &str) -> Result<()> {
    let addr: std::net::SocketAddr = to.parse().context("--to must be ip:port")?;
    let items: Vec<SendItem> = paths
        .iter()
        .filter_map(|p| SendItem::from_path(p).ok())
        .collect();
    if items.is_empty() {
        anyhow::bail!("no readable files given");
    }
    let total: u64 = items.iter().map(|i| i.size).sum();
    println!("sending {} to {addr}", bytesize::ByteSize(total));

    let conn = zs_net::connect("0.0.0.0:0".parse()?, addr, None).await?;
    let report = zs_transfer::send(&conn, items, Arc::new(|p: Progress| draw_progress(&p))).await?;
    println!();
    println!("sent           {}", bytesize::ByteSize(report.bytes));
    println!(
        "throughput     {:.1} MiB/s",
        report.bytes_per_sec() / (1024.0 * 1024.0)
    );
    conn.close(0u32.into(), b"done");
    Ok(())
}

async fn selftest(size_mb: u64, keep: bool) -> Result<()> {
    let size = size_mb * 1024 * 1024;
    let root = std::env::temp_dir().join(format!("zenshare-selftest-{}", std::process::id()));
    let src_dir = root.join("src");
    let dst_dir = root.join("dst");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::create_dir_all(&dst_dir)?;

    let src = src_dir.join("payload.bin");
    println!("generating {} payload…", bytesize::ByteSize(size));
    write_payload(&src, size)?;

    let expected = hash_file(&src)?;
    println!("source blake3  {expected}");

    let listener = zs_net::listen("127.0.0.1:0".parse()?)?;
    let addr = listener.local_addr;
    println!("listening on   {addr}");

    let item = SendItem::from_path(&src)?;
    let total = item.size;

    let dst_for_task = dst_dir.clone();
    let server = tokio::spawn(async move {
        let incoming = listener
            .endpoint
            .accept()
            .await
            .context("no incoming connection")?;
        let conn = incoming.await.context("accept handshake")?;
        zs_transfer::recv(&conn, &dst_for_task, Arc::new(|_p: Progress| {})).await
    });

    let conn = zs_net::connect("0.0.0.0:0".parse()?, addr, None).await?;

    let started = Instant::now();
    let sent =
        zs_transfer::send(&conn, vec![item], Arc::new(|p: Progress| draw_progress(&p))).await?;
    let received = server.await.context("receiver task panicked")??;
    let elapsed = started.elapsed();

    println!();

    let actual = hash_file(&dst_dir.join("payload.bin"))?;
    println!("dest   blake3  {actual}");

    if actual != expected {
        anyhow::bail!("INTEGRITY FAILURE — hashes differ");
    }
    if received.bytes != total || sent.bytes != total {
        anyhow::bail!(
            "byte count mismatch: sent {} recv {} expected {total}",
            sent.bytes,
            received.bytes
        );
    }

    let mbps = total as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    println!("integrity      ok");
    println!("moved          {}", bytesize::ByteSize(total));
    println!("elapsed        {}", humantime::format_duration(elapsed));
    println!("throughput     {mbps:.1} MiB/s");

    if keep {
        println!("kept           {}", root.display());
    } else {
        let _ = std::fs::remove_dir_all(&root);
    }
    Ok(())
}

fn draw_progress(p: &Progress) {
    let pct = p.fraction() * 100.0;
    let mib = p.bytes_per_sec() / (1024.0 * 1024.0);
    eprint!(
        "\r  {pct:5.1}%  {mib:>7.1} MiB/s  {}/{} chunks   ",
        p.chunks_done, p.chunks_total
    );
}

fn write_payload(path: &Path, size: u64) -> Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::with_capacity(4 * 1024 * 1024, std::fs::File::create(path)?);
    let mut buf = vec![0u8; 1024 * 1024];
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut written = 0u64;
    while written < size {
        for chunk in buf.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let bytes = state.to_le_bytes();
            let n = chunk.len().min(8);
            chunk[..n].copy_from_slice(&bytes[..n]);
        }
        let take = (size - written).min(buf.len() as u64) as usize;
        f.write_all(&buf[..take])?;
        written += take as u64;
    }
    f.flush()?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let f = std::fs::File::open(path)?;
    // SAFETY: read-only mapping of a file we just wrote and nobody else touches.
    #[allow(unsafe_code)]
    let mmap = unsafe { memmap2::Mmap::map(&f)? };
    Ok(blake3::hash(&mmap).to_hex().to_string())
}
