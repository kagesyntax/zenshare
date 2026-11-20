//! The engine: discovery, listener, and transfer orchestration.
//!
//! Runs on its own thread with its own Tokio runtime. The UI never touches this
//! code directly — it reads an immutable snapshot and pushes commands down a
//! channel. That separation is why the window stays at 60 fps while eight QUIC
//! streams are saturating the link.

use crate::paths::AppPaths;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zs_core::{DeviceId, DeviceInfo};
use zs_transfer::{Progress, SendItem};
use zs_ui::{Peer, TransferRow, TransferState, UiAction, UiModel};

/// How long a peer stays visible after its last announcement.
const PEER_TTL: Duration = Duration::from_secs(8);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> String {
    format!("t{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

fn hostname() -> String {
    if let Ok(h) = std::env::var("HOSTNAME")
        && !h.trim().is_empty()
    {
        return h.trim_end_matches(".local").to_string();
    }
    if let Ok(h) = std::env::var("COMPUTERNAME")
        && !h.trim().is_empty()
    {
        return h;
    }
    match std::env::var("USER") {
        Ok(u) if !u.is_empty() => format!("{u}'s device"),
        _ => "This device".to_string(),
    }
}

/// Start the engine on a background thread.
pub fn spawn(
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Receiver<UiAction>,
    bind: Ipv4Addr,
    paths: Arc<AppPaths>,
) -> Result<()> {
    std::thread::Builder::new()
        .name("zs-engine".to_string())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("runtime: {e}");
                    return;
                }
            };
            if let Err(e) = rt.block_on(run(model, actions, bind, paths)) {
                tracing::error!("engine stopped: {e:#}");
            }
        })
        .context("spawn engine thread")?;
    Ok(())
}

async fn run(
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Receiver<UiAction>,
    bind_ip: Ipv4Addr,
    paths: Arc<AppPaths>,
) -> Result<()> {
    paths.ensure()?;
    let identity = zs_crypto::load_or_create(&paths.identity)?;
    let device = DeviceInfo::local(identity.device_id(), hostname());

    let listener = zs_net::listen(SocketAddr::from((bind_ip, 0)))?;
    let port = listener.local_addr.port();
    let lan = if bind_ip.is_unspecified() {
        zs_net::primary_lan_ip().map_or_else(|| "offline".to_string(), |ip| ip.to_string())
    } else {
        bind_ip.to_string()
    };

    {
        let mut m = model
            .write()
            .map_err(|_| anyhow::anyhow!("model poisoned"))?;
        m.my_name.clone_from(&device.name);
        m.my_id = device.id.short();
        m.port = port;
        m.status = format!("{lan}:{port}  ·  encrypted  ·  no account");
    }

    // Discovery is best-effort. A machine with multicast disabled still runs;
    // it just cannot see peers until they are addressed directly.
    let mut announces = match zs_net::beacon::spawn(&device, port) {
        Ok(rx) => Some(rx),
        Err(e) => {
            tracing::warn!("discovery unavailable: {e}");
            if let Ok(mut m) = model.write() {
                m.discovery = format!("Discovery failed to start: {e}");
            }
            None
        }
    };
    // Printed so a headless sender can be pointed at this window.
    eprintln!("listening on {bind_ip}:{port}");

    let mut peers: HashMap<DeviceId, (SocketAddr, Instant)> = HashMap::new();

    loop {
        tokio::select! {
            action = actions.recv() => {
                match action {
                    Ok(action) => handle_action(&model, &peers, action, &paths),
                    Err(_) => break,
                }
            }
            heard = async {
                match announces.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match heard {
                    Some(heard) => on_heard(&model, &mut peers, &heard),
                    None => break,
                }
            }
            incoming = listener.endpoint.accept() => {
                if let Some(incoming) = incoming {
                    spawn_receive(&model, incoming, Arc::clone(&paths));
                }
            }
            () = tokio::time::sleep(Duration::from_secs(1)) => {
                prune(&model, &mut peers);
            }
        }
    }

    Ok(())
}

fn on_heard(
    model: &Arc<RwLock<UiModel>>,
    peers: &mut HashMap<DeviceId, (SocketAddr, Instant)>,
    heard: &zs_net::beacon::Heard,
) {
    let id = heard.announce.device.id;
    peers.insert(id, (heard.quic_addr, Instant::now()));

    let Ok(mut m) = model.write() else { return };
    let addr = heard.quic_addr.ip().to_string();
    if let Some(existing) = m.peers.iter_mut().find(|p| p.id == id) {
        existing.name.clone_from(&heard.announce.device.name);
        existing.addr = addr;
        existing.online = true;
    } else {
        m.peers.push(Peer {
            id,
            name: heard.announce.device.name.clone(),
            kind: heard.announce.device.kind,
            addr,
            online: true,
        });
    }
    m.peers.sort_by_key(|a| a.name.to_lowercase());
    let n = m.peers.len();
    m.status = format!(
        "{n} device{} nearby  ·  encrypted  ·  no account",
        if n == 1 { "" } else { "s" }
    );
}

fn prune(model: &Arc<RwLock<UiModel>>, peers: &mut HashMap<DeviceId, (SocketAddr, Instant)>) {
    let now = Instant::now();
    let stale: Vec<DeviceId> = peers
        .iter()
        .filter(|(_, (_, seen))| now.duration_since(*seen) > PEER_TTL)
        .map(|(id, _)| *id)
        .collect();
    if stale.is_empty() {
        return;
    }
    for id in &stale {
        peers.remove(id);
    }
    if let Ok(mut m) = model.write() {
        m.peers.retain(|p| !stale.contains(&p.id));
    }
}

fn handle_action(
    model: &Arc<RwLock<UiModel>>,
    peers: &HashMap<DeviceId, (SocketAddr, Instant)>,
    action: UiAction,
    paths: &Arc<AppPaths>,
) {
    match action {
        UiAction::Send { peer, files } => {
            let Some((addr, _)) = peers.get(&peer).copied() else {
                set_status(model, "that device just went away");
                return;
            };
            let peer_name = model
                .read()
                .ok()
                .and_then(|m| {
                    m.peers
                        .iter()
                        .find(|p| p.id == peer)
                        .map(|p| p.name.clone())
                })
                .unwrap_or_else(|| "device".to_string());
            let model = Arc::clone(model);
            tokio::spawn(async move {
                if let Err(e) = do_send(model, addr, peer_name, files).await {
                    tracing::warn!("send failed: {e:#}");
                }
            });
        }
        UiAction::Cancel(id) => {
            if let Ok(mut m) = model.write() {
                m.transfers.retain(|t| t.id != id);
            }
        }
        UiAction::RevealDownloads => {
            let _ = std::fs::create_dir_all(&paths.downloads);
            reveal(&paths.downloads);
        }
    }
}

async fn do_send(
    model: Arc<RwLock<UiModel>>,
    addr: SocketAddr,
    peer_name: String,
    files: Vec<PathBuf>,
) -> Result<()> {
    let id = next_id();
    let items: Vec<SendItem> = files
        .iter()
        .filter_map(|p| SendItem::from_path(p).ok())
        .collect();
    if items.is_empty() {
        set_status(&model, "nothing to send");
        return Ok(());
    }
    let total: u64 = items.iter().map(|i| i.size).sum();
    let label = if items.len() == 1 {
        items[0].rel.clone()
    } else {
        format!("{} files", items.len())
    };

    add_row(
        &model,
        TransferRow {
            id: id.clone(),
            peer: peer_name,
            file: label,
            bytes_done: 0,
            total,
            speed_bps: 0.0,
            state: TransferState::Pending,
        },
    );

    let bind = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0));
    let conn = match zs_net::connect(bind, addr, None).await {
        Ok(c) => c,
        Err(e) => {
            fail_row(&model, &id, &format!("cannot reach device: {e}"));
            return Ok(());
        }
    };

    let progress = progress_hook(Arc::clone(&model), id.clone());
    match zs_transfer::send(&conn, items, progress).await {
        Ok(report) => {
            let mib = report.bytes_per_sec() / (1024.0 * 1024.0);
            finish_row(&model, &id, TransferState::Done);
            set_status(
                &model,
                &format!(
                    "sent {} at {mib:.0} MB/s",
                    zs_ui::widgets::fmt_bytes(report.bytes)
                ),
            );
            conn.close(0u32.into(), b"done");
        }
        Err(e) => {
            fail_row(&model, &id, &format!("{e}"));
        }
    }
    Ok(())
}

fn spawn_receive(model: &Arc<RwLock<UiModel>>, incoming: quinn::Incoming, paths: Arc<AppPaths>) {
    let model = Arc::clone(model);
    tokio::spawn(async move {
        let conn = match incoming.await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("handshake failed: {e}");
                return;
            }
        };
        let id = next_id();
        let peer = conn.remote_address().ip().to_string();

        add_row(
            &model,
            TransferRow {
                id: id.clone(),
                peer,
                file: "incoming transfer".to_string(),
                bytes_done: 0,
                total: 0,
                speed_bps: 0.0,
                state: TransferState::Pending,
            },
        );

        let dest = paths.downloads.clone();
        let _ = std::fs::create_dir_all(&dest);
        let progress = progress_hook(Arc::clone(&model), id.clone());

        match zs_transfer::recv(&conn, &dest, progress).await {
            Ok(report) => {
                finish_row(&model, &id, TransferState::Done);
                set_status(
                    &model,
                    &format!(
                        "received {} into Downloads",
                        zs_ui::widgets::fmt_bytes(report.bytes)
                    ),
                );
            }
            Err(e) => fail_row(&model, &id, &format!("{e}")),
        }
    });
}

fn progress_hook(model: Arc<RwLock<UiModel>>, id: String) -> Arc<dyn Fn(Progress) + Send + Sync> {
    Arc::new(move |p: Progress| {
        if let Ok(mut m) = model.write()
            && let Some(row) = m.transfers.iter_mut().find(|t| t.id == id)
        {
            row.bytes_done = p.bytes_done;
            if p.total > 0 {
                row.total = p.total;
            }
            row.speed_bps = p.bytes_per_sec();
            row.state = TransferState::Active;
        }
    })
}

fn add_row(model: &Arc<RwLock<UiModel>>, row: TransferRow) {
    if let Ok(mut m) = model.write() {
        m.transfers.insert(0, row);
        m.transfers.truncate(4);
    }
}

fn finish_row(model: &Arc<RwLock<UiModel>>, id: &str, state: TransferState) {
    if let Ok(mut m) = model.write()
        && let Some(row) = m.transfers.iter_mut().find(|t| t.id == id)
    {
        row.state = state;
        row.bytes_done = row.total;
        row.speed_bps = 0.0;
    }
}

fn fail_row(model: &Arc<RwLock<UiModel>>, id: &str, reason: &str) {
    tracing::warn!("transfer failed: {reason}");
    if let Ok(mut m) = model.write() {
        if let Some(row) = m.transfers.iter_mut().find(|t| t.id == id) {
            row.state = TransferState::Failed;
        }
        m.status = format!("failed: {reason}");
    }
}

/// Ask the OS to show the downloads folder. No-op on Android, which has no
/// file manager to hand a path to.
#[cfg(not(target_os = "android"))]
fn reveal(path: &std::path::Path) {
    let _ = open::that(path);
}

#[cfg(target_os = "android")]
fn reveal(_path: &std::path::Path) {}

fn set_status(model: &Arc<RwLock<UiModel>>, text: &str) {
    if let Ok(mut m) = model.write() {
        m.status = text.to_string();
    }
}
