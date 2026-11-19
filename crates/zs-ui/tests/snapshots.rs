//! Golden-image tests for the interface.
//!
//! The point of this file is to make the UI refactor measurable. Every later
//! phase — tokens, primitives, shell, motion — changes pixels, and without a
//! checked-in baseline there is no way to tell a deliberate change from a
//! regression except by looking at it and hoping.
//!
//! Run with `UPDATE_SNAPSHOTS=1 cargo test -p zs-ui` to accept a change.
//! Goldens live in `tests/snapshots/`.

use std::sync::{Arc, RwLock};

use egui_kittest::Harness;
use zs_core::DeviceId;
use zs_ui::{App, AppOptions, Peer, TransferRow, TransferState, UiModel};

const DESKTOP: [f32; 2] = [1080.0, 700.0];
const LARGE: [f32; 2] = [1440.0, 900.0];
const NARROW: [f32; 2] = [380.0, 720.0];

fn peer(seed: u8, name: &str, kind: zs_core::DeviceKind, addr: &str) -> Peer {
    Peer {
        id: DeviceId([seed; 32]),
        name: name.to_string(),
        kind,
        addr: addr.to_string(),
        online: true,
    }
}

fn base() -> UiModel {
    UiModel {
        my_name: "Kage's MacBook".to_string(),
        my_id: "4f2a-91c3".to_string(),
        port: 47820,
        peers: Vec::new(),
        transfers: Vec::new(),
        status: "10.1.0.24:47820  ·  encrypted  ·  no account".to_string(),
        discovery: String::new(),
    }
}

/// Nothing found yet — the first thing a new user sees.
fn fixture_empty() -> UiModel {
    base()
}

/// The state the app is in most of the time: two devices on the network.
fn fixture_peers() -> UiModel {
    let mut m = base();
    m.peers = vec![
        peer(1, "Pixel 9", zs_core::DeviceKind::Phone, "192.168.1.24"),
        peer(2, "Studio", zs_core::DeviceKind::Desktop, "192.168.1.31"),
    ];
    m.status = "2 devices nearby  ·  encrypted  ·  no account".to_string();
    m
}

/// Mid-transfer, with one finished and one failed, because the three states
/// have to read differently at a glance.
fn fixture_transfers() -> UiModel {
    let mut m = fixture_peers();
    m.transfers = vec![
        TransferRow {
            id: "t1".to_string(),
            peer: "Pixel 9".to_string(),
            file: "wedding-raw.zip".to_string(),
            bytes_done: 1_610_612_736,
            total: 2_684_354_560,
            speed_bps: 96_468_992.0,
            state: TransferState::Active,
        },
        TransferRow {
            id: "t2".to_string(),
            peer: "Studio".to_string(),
            file: "session-14.mov".to_string(),
            bytes_done: 1_073_741_824,
            total: 1_073_741_824,
            speed_bps: 0.0,
            state: TransferState::Done,
        },
        TransferRow {
            id: "t3".to_string(),
            peer: "Pixel 9".to_string(),
            file: "assets.tar".to_string(),
            bytes_done: 0,
            total: 512_000_000,
            speed_bps: 0.0,
            state: TransferState::Failed,
        },
    ];
    m
}

/// Discovery failed. This is the state that used to say nothing at all.
fn fixture_discovery_failed() -> UiModel {
    let mut m = base();
    m.discovery = "Discovery failed to start: multicast unavailable".to_string();
    m
}

fn harness(model: UiModel, size: [f32; 2]) -> Harness<'static, App> {
    let model = Arc::new(RwLock::new(model));
    let (tx, rx) = async_channel::unbounded();
    // The UI holds the sender; if the receiver is dropped every send fails and
    // the app would take a different code path than it does in production.
    std::mem::forget(rx);

    Harness::builder()
        .with_size(size)
        .wgpu()
        // Fonts and style must be installed before the harness lays out its
        // first pass, or that pass measures with the wrong typeface.
        .with_context_setup(|ctx| {
            zs_ui::theme::install_fonts(ctx);
            zs_ui::theme::install_style(ctx);
        })
        .build_eframe(move |cc| zs_ui::build_app(cc, model, tx, AppOptions::default()))
}

#[test]
fn empty_state() {
    let mut h = harness(fixture_empty(), DESKTOP);
    h.run();
    h.snapshot("empty_1080x700");
}

#[test]
fn two_peers() {
    let mut h = harness(fixture_peers(), DESKTOP);
    h.run();
    h.snapshot("peers_1080x700");
}

#[test]
fn two_peers_large() {
    let mut h = harness(fixture_peers(), LARGE);
    h.run();
    h.snapshot("peers_1440x900");
}

#[test]
fn transfers() {
    let mut h = harness(fixture_transfers(), DESKTOP);
    h.run();
    h.snapshot("transfers_1080x700");
}

#[test]
fn discovery_failed() {
    let mut h = harness(fixture_discovery_failed(), DESKTOP);
    h.run();
    h.snapshot("discovery_failed_1080x700");
}

/// The layout the phone build has to survive. It is currently wrong in a way
/// the desktop never shows, and this fixture is what will prove it is fixed.
#[test]
fn narrow() {
    let mut h = harness(fixture_peers(), NARROW);
    h.run();
    h.snapshot("peers_380x720");
}
