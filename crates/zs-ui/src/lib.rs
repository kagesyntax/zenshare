//! The interface.
//!
//! Every control here is painted rather than instantiated. `ui.interact` over a
//! computed rect gives a `Response`; the visual comes from `widgets`. That is
//! the only way to hit exact metrics in egui, and it is what makes the snapshot
//! harness meaningful — the output is our drawing, not a theme's.
//!
//! Layout is a single `Layout` struct computed once per frame, so panels cannot
//! disagree about where they start. The previous version handed two panels the
//! same rect and let them overlap.
//!
//! Design decisions and their reasoning live in `docs/UI-SYSTEM.md`.

// `p`, `m`, `t`, `i` are the conventional bindings in a paint routine; long
// names here would be noise, not clarity.
#![allow(clippy::many_single_char_names)]

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use egui::{Color32, Id, Rect, Sense, Stroke, pos2, vec2};
use zs_core::{DeviceId, DeviceKind};

pub mod theme;
pub mod widgets;

use widgets::{ButtonKind, States};

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Unified titlebar: the only horizontal band at the top.
const TITLE_H: f32 = 52.0;
/// Sidebar width at the default window size.
const SIDEBAR_W: f32 = 280.0;
/// Device row height.
const ROW_H: f32 = 52.0;
/// Transfer row height.
const XFER_H: f32 = 64.0;
/// Collapsed bottom panel.
const BOTTOM_COLLAPSED: f32 = 36.0;
/// Expanded bottom panel.
const BOTTOM_EXPANDED: f32 = 220.0;
/// Below this width the sidebar becomes a horizontal strip.
const NARROW: f32 = 720.0;
/// Height of that strip.
const STRIP_H: f32 = 88.0;

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// A device we can send to.
#[derive(Clone, Debug)]
pub struct Peer {
    /// Stable identity.
    pub id: DeviceId,
    /// Display name.
    pub name: String,
    /// Icon hint.
    pub kind: DeviceKind,
    /// Where to reach it, for display.
    pub addr: String,
    /// Whether we have seen an announcement recently.
    pub online: bool,
}

/// What a transfer is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferState {
    /// Waiting for the peer to accept.
    Pending,
    /// Moving bytes.
    Active,
    /// Finished and verified.
    Done,
    /// Aborted.
    Failed,
}

/// One row in the transfer list.
#[derive(Clone, Debug)]
pub struct TransferRow {
    /// Stable key for animation state.
    pub id: String,
    /// Peer display name.
    pub peer: String,
    /// File name.
    pub file: String,
    /// Bytes confirmed.
    pub bytes_done: u64,
    /// Total bytes.
    pub total: u64,
    /// Current throughput.
    pub speed_bps: f64,
    /// Current state.
    pub state: TransferState,
}

impl TransferRow {
    /// 0.0 to 1.0.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.bytes_done as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }

    /// Seconds remaining at the current rate, if we are moving.
    #[must_use]
    pub fn eta_secs(&self) -> Option<f64> {
        if self.state != TransferState::Active || self.speed_bps <= 1.0 {
            return None;
        }
        let left = self.total.saturating_sub(self.bytes_done) as f64;
        Some(left / self.speed_bps)
    }
}

/// A file waiting to be sent.
#[derive(Clone, Debug)]
pub struct QueuedFile {
    /// Absolute path.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
}

impl QueuedFile {
    /// Read metadata and build a queue entry.
    #[must_use]
    pub fn new(path: PathBuf) -> Option<Self> {
        let size = std::fs::metadata(&path).ok()?.len();
        Some(Self { path, size })
    }

    /// Name shown in the list.
    #[must_use]
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }
}

/// Everything the UI renders from. Replaced wholesale by the engine.
#[derive(Clone, Debug, Default)]
pub struct UiModel {
    /// This device's display name.
    pub my_name: String,
    /// This device's short id.
    pub my_id: String,
    /// Port we listen on.
    pub port: u16,
    /// Peers currently visible.
    pub peers: Vec<Peer>,
    /// Transfers, newest first.
    pub transfers: Vec<TransferRow>,
    /// One-line status.
    pub status: String,
    /// What discovery is doing. Empty when normal; otherwise a short reason,
    /// because "0 devices" with no explanation is unactionable.
    pub discovery: String,
}

impl UiModel {
    /// Sample data for design work and for the snapshot fixtures.
    #[must_use]
    pub fn demo() -> Self {
        Self {
            my_name: "Kage's MacBook".to_string(),
            my_id: "a3f1-9c02".to_string(),
            port: 47820,
            peers: vec![
                Peer {
                    id: DeviceId([0x11; 32]),
                    name: "Pixel 9".to_string(),
                    kind: DeviceKind::Phone,
                    addr: "192.168.1.24".to_string(),
                    online: true,
                },
                Peer {
                    id: DeviceId([0x22; 32]),
                    name: "Studio".to_string(),
                    kind: DeviceKind::Desktop,
                    addr: "192.168.1.31".to_string(),
                    online: true,
                },
            ],
            transfers: vec![
                TransferRow {
                    id: "demo-1".to_string(),
                    peer: "Pixel 9".to_string(),
                    file: "wedding-raw.zip".to_string(),
                    bytes_done: 2_684_354_560,
                    total: 4_294_967_296,
                    speed_bps: 96_468_992.0,
                    state: TransferState::Active,
                },
                TransferRow {
                    id: "demo-2".to_string(),
                    peer: "Studio".to_string(),
                    file: "session-14.mov".to_string(),
                    bytes_done: 1_073_741_824,
                    total: 1_073_741_824,
                    speed_bps: 0.0,
                    state: TransferState::Done,
                },
            ],
            status: "2 devices nearby  ·  encrypted  ·  no account".to_string(),
            discovery: String::new(),
        }
    }
}

/// Commands the UI sends to the engine.
#[derive(Clone, Debug)]
pub enum UiAction {
    /// Send `files` to `peer`.
    Send {
        /// Destination.
        peer: DeviceId,
        /// Files to send.
        files: Vec<PathBuf>,
    },
    /// Abort a transfer.
    Cancel(String),
    /// Reveal the download folder.
    RevealDownloads,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

/// How the window should behave, separate from what it displays.
#[derive(Debug, Clone, Copy)]
pub struct AppOptions {
    /// Keep the window above everything else.
    pub on_top: bool,
    /// Fill the model with sample data instead of talking to an engine.
    pub demo: bool,
    /// Leading inset for the macOS traffic lights when the window paints its own
    /// titlebar. Zero everywhere else, including in tests.
    pub titlebar_inset: f32,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            on_top: false,
            demo: false,
            titlebar_inset: 0.0,
        }
    }
}

/// Build the app directly, with a concrete type.
///
/// [`make_app`] erases this to `Box<dyn eframe::App>`, which is what eframe
/// wants but what the snapshot harness cannot hold — it needs a sized `App`
/// so it can drive frames itself.
#[must_use]
pub fn build_app(
    cc: &eframe::CreationContext<'_>,
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Sender<UiAction>,
    options: AppOptions,
) -> App {
    theme::install_fonts(&cc.egui_ctx);
    theme::install_style(&cc.egui_ctx);
    App::new(model, actions, options)
}

/// Build the app for a given egui context, erased for eframe.
#[must_use]
pub fn make_app(
    cc: &eframe::CreationContext<'_>,
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Sender<UiAction>,
    options: AppOptions,
) -> Box<dyn eframe::App> {
    Box::new(build_app(cc, model, actions, options))
}

/// Launch the desktop window. Blocks until the user closes it.
///
/// # Errors
/// Returns an error if a graphics context cannot be created.
pub fn run(
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Sender<UiAction>,
    on_top: bool,
    demo: bool,
) -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1080.0, 700.0])
        .with_min_inner_size([360.0, 480.0])
        .with_title("ZenShare");

    // Let the content run underneath the titlebar so there is one horizontal
    // band instead of a native one plus ours. The traffic lights need room.
    #[cfg(target_os = "macos")]
    let inset = 72.0;
    #[cfg(not(target_os = "macos"))]
    let inset = 0.0;

    if on_top || std::env::var_os("ZENSHARE_ON_TOP").is_some() {
        viewport = viewport.with_always_on_top();
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let app_options = AppOptions {
        on_top,
        demo,
        titlebar_inset: inset,
    };

    eframe::run_native(
        "ZenShare",
        options,
        Box::new(move |cc| Ok(make_app(cc, model, actions, app_options))),
    )
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Where everything goes this frame. Computed once so panels cannot disagree.
struct Layout {
    title: Rect,
    banner: Option<Rect>,
    sidebar: Rect,
    canvas: Rect,
    bottom: Rect,
    expanded: bool,
    narrow: bool,
}

impl Layout {
    fn compute(full: Rect, model: &UiModel, bottom_open: bool) -> Self {
        let title = Rect::from_min_size(full.min, vec2(full.width(), TITLE_H));
        let mut y = title.max.y;

        let banner = if model.discovery.is_empty() {
            None
        } else {
            let r = Rect::from_min_size(pos2(full.min.x, y), vec2(full.width(), 34.0));
            y = r.max.y;
            Some(r)
        };

        let expanded = bottom_open && !model.transfers.is_empty();
        let bottom_h = if expanded {
            BOTTOM_EXPANDED
        } else {
            BOTTOM_COLLAPSED
        };
        let bottom = Rect::from_min_max(pos2(full.min.x, full.max.y - bottom_h), full.max);
        let body = Rect::from_min_max(pos2(full.min.x, y), pos2(full.max.x, bottom.min.y));
        let narrow = full.width() < NARROW;

        if narrow {
            // Phone shape: devices become a horizontal strip above the canvas
            // rather than a column that eats most of the width.
            let strip_h = STRIP_H.min(body.height() * 0.4);
            let sidebar = Rect::from_min_size(body.min, vec2(body.width(), strip_h));
            let canvas = Rect::from_min_max(pos2(body.min.x, sidebar.max.y), body.max);
            Self {
                title,
                banner,
                sidebar,
                canvas,
                bottom,
                expanded,
                narrow,
            }
        } else {
            let sidebar = Rect::from_min_size(body.min, vec2(SIDEBAR_W, body.height()));
            let canvas = Rect::from_min_max(pos2(sidebar.max.x, body.min.y), body.max);
            Self {
                title,
                banner,
                sidebar,
                canvas,
                bottom,
                expanded,
                narrow,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// The application. Owns the model handle, the outbound command channel, and
/// the transient UI state (selection, queue) that does not belong in the model.
pub struct App {
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Sender<UiAction>,
    options: AppOptions,
    queue: Vec<QueuedFile>,
    selected: Option<DeviceId>,
    bottom_open: bool,
}

impl App {
    /// Build the app directly, with a concrete type.
    #[must_use]
    pub fn new(
        model: Arc<RwLock<UiModel>>,
        actions: async_channel::Sender<UiAction>,
        options: AppOptions,
    ) -> Self {
        Self {
            model,
            actions,
            options,
            queue: Vec::new(),
            selected: None,
            bottom_open: true,
        }
    }

    /// In demo mode the active row advances, so the window visibly animates and
    /// a screenshot taken later proves the paint loop is running.
    fn tick_demo(&mut self) {
        if !self.options.demo {
            return;
        }
        let Ok(mut m) = self.model.write() else {
            return;
        };
        for t in &mut m.transfers {
            if t.state == TransferState::Active {
                t.bytes_done = (t.bytes_done + 24_000_000).min(t.total);
                if t.bytes_done >= t.total {
                    t.state = TransferState::Done;
                }
            }
        }
    }

    fn snapshot(&self) -> UiModel {
        self.model.read().map(|m| m.clone()).unwrap_or_default()
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if let Some(q) = QueuedFile::new(path) {
                self.queue.push(q);
            }
        }
    }

    fn pick_files(&mut self) {
        #[cfg(not(target_os = "android"))]
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose files to send")
            .pick_files()
        {
            self.add_files(paths);
        }

        // Android has no file dialog and no drag-and-drop. Picking files there
        // needs the Storage Access Framework; until that lands, a phone can
        // receive but not send, and the UI says so rather than offering a
        // button that silently does nothing.
    }

    fn send(&mut self) {
        let Some(peer) = self.selected else { return };
        if self.queue.is_empty() {
            return;
        }
        let files: Vec<PathBuf> = self.queue.iter().map(|q| q.path.clone()).collect();
        let _ = self.actions.try_send(UiAction::Send { peer, files });
        self.queue.clear();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, theme::BG);

        let dropped: Vec<PathBuf> = ui.ctx().input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }

        self.tick_demo();
        let m = self.snapshot();
        let layout = Layout::compute(full, &m, self.bottom_open);

        self.titlebar(ui, layout.title, &m);
        if let Some(banner) = layout.banner {
            Self::banner(ui, banner, &m.discovery);
        }

        if layout.narrow {
            self.device_strip(ui, layout.sidebar, &m);
        } else {
            self.sidebar(ui, layout.sidebar, &m);
        }
        self.canvas(ui, layout.canvas, &m, layout.narrow);
        self.bottom_panel(ui, layout.bottom, &m, layout.expanded);

        // egui only repaints on input by default. The engine mutates the model
        // from another thread and cannot poke the window, so we tick: fast while
        // something is moving, slowly when idle.
        let busy = m
            .transfers
            .iter()
            .any(|t| matches!(t.state, TransferState::Active | TransferState::Pending));
        ui.ctx().request_repaint_after(if busy {
            Duration::from_millis(80)
        } else {
            Duration::from_millis(250)
        });
    }
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

impl App {
    fn titlebar(&mut self, ui: &mut egui::Ui, rect: Rect, m: &UiModel) {
        let p = ui.painter();
        p.rect_filled(rect, 0, theme::PANEL);
        widgets::divider(p, rect.min.x, rect.max.x, rect.max.y - 0.5);

        let x = rect.min.x + self.options.titlebar_inset + theme::S5;
        let cy = rect.center().y;

        let mark = Rect::from_center_size(pos2(x + 11.0, cy), vec2(22.0, 22.0));
        p.rect_filled(mark, widgets::cr(theme::R_CONTROL), theme::accent_wash());
        widgets::icon(p, mark.center(), 13.0, widgets::Icon::Send, theme::ACCENT);
        widgets::text_left(
            p,
            pos2(mark.max.x + theme::S3, cy),
            "ZenShare",
            theme::title1(),
            theme::LABEL,
        );

        // This device. Identity, not an action, so a quiet chip rather than a
        // button.
        let name = widgets::ellipsize(&m.my_name, 26);
        let w = (name.chars().count() as f32 * 6.6).clamp(70.0, 210.0) + 34.0;
        let chip = Rect::from_min_size(pos2(rect.max.x - theme::S5 - w, cy - 12.0), vec2(w, 24.0));
        p.rect_filled(chip, widgets::cr(theme::R_PILL), theme::RAISED);
        p.circle_filled(pos2(chip.min.x + 12.0, cy), 3.0, theme::PROGRESS);
        widgets::text_left(
            p,
            pos2(chip.min.x + 22.0, cy),
            name,
            theme::caption(),
            theme::LABEL2,
        );
    }

    fn banner(ui: &mut egui::Ui, rect: Rect, message: &str) {
        let p = ui.painter();
        p.rect_filled(rect, 0, theme::WARN.gamma_multiply(0.10));
        widgets::divider(p, rect.min.x, rect.max.x, rect.max.y - 0.5);

        widgets::icon(
            p,
            pos2(rect.min.x + theme::S5, rect.center().y),
            11.0,
            widgets::Icon::Close,
            theme::WARN,
        );
        widgets::text_left(
            p,
            pos2(rect.min.x + theme::S5 + 18.0, rect.center().y),
            widgets::ellipsize(message, 110),
            theme::callout(),
            theme::WARN,
        );
    }

    fn section_header(p: &egui::Painter, x: f32, y: f32, text: &str, count: usize) {
        widgets::text_left(p, pos2(x, y), text, theme::label(), theme::LABEL3);

        if count > 0 {
            let chip = Rect::from_min_size(
                pos2(x + text.chars().count() as f32 * 7.0 + theme::S2, y - 7.0),
                vec2(18.0, 14.0),
            );
            widgets::chip(p, chip, &count.to_string(), theme::LABEL2);
        }
    }
}

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

impl App {
    fn sidebar(&mut self, ui: &mut egui::Ui, rect: Rect, m: &UiModel) {
        let p = ui.painter();
        p.rect_filled(rect, 0, theme::PANEL);
        widgets::vdivider(p, rect.max.x - 0.5, rect.min.y, rect.max.y);

        let mut y = rect.min.y + theme::S5;
        Self::section_header(p, rect.min.x + theme::S5, y, "NEARBY", m.peers.len());
        y += theme::S5;

        if m.peers.is_empty() {
            Self::empty_peers(p, rect, y);
            return;
        }

        let row_w = rect.width() - theme::S5 * 2.0;
        for (i, peer) in m.peers.iter().enumerate() {
            let row = Rect::from_min_size(pos2(rect.min.x + theme::S5, y), vec2(row_w, ROW_H));
            if row.max.y > rect.max.y {
                break;
            }
            self.device_row(ui, row, peer, i);
            y += ROW_H + theme::S2;
        }
    }

    fn device_strip(&mut self, ui: &mut egui::Ui, rect: Rect, m: &UiModel) {
        let p = ui.painter();
        p.rect_filled(rect, 0, theme::PANEL);
        widgets::divider(p, rect.min.x, rect.max.x, rect.max.y - 0.5);

        Self::section_header(
            p,
            rect.min.x + theme::S3,
            rect.min.y + theme::S3 + 5.0,
            "NEARBY",
            m.peers.len(),
        );

        if m.peers.is_empty() {
            widgets::text_left(
                p,
                pos2(rect.min.x + theme::S3, rect.min.y + 48.0),
                "Looking for devices…",
                theme::callout(),
                theme::LABEL3,
            );
            return;
        }

        // Horizontal chips: a phone has width and no height to spare.
        let mut x = rect.min.x + theme::S3;
        let y = rect.min.y + 34.0;
        let h = 44.0;
        for (i, peer) in m.peers.iter().enumerate() {
            let w = (peer.name.chars().count() as f32 * 7.0 + 52.0).clamp(110.0, 180.0);
            if x + w > rect.max.x - theme::S3 {
                break;
            }
            let r = Rect::from_min_size(pos2(x, y), vec2(w, h));
            let resp = ui.interact(r, Id::unique(("peer-strip", i)), Sense::click());
            let selected = self.selected == Some(peer.id);
            let st = States::of(&resp, selected, true);
            let pp = ui.painter();

            let fill = if st.selected {
                theme::accent_wash()
            } else if st.hovered {
                theme::RAISED
            } else {
                Color32::TRANSPARENT
            };
            if fill != Color32::TRANSPARENT {
                pp.rect_filled(r, widgets::cr(theme::R_CARD), fill);
            }

            widgets::avatar(
                pp,
                pos2(r.min.x + 20.0, r.center().y),
                13.0,
                i as u32,
                &widgets::initial(&peer.name),
            );
            widgets::text_left(
                pp,
                pos2(r.min.x + 38.0, r.center().y),
                widgets::ellipsize(&peer.name, 14),
                theme::headline(),
                theme::LABEL,
            );

            if resp.clicked() {
                self.selected = Some(peer.id);
            }
            x += w + theme::S2;
        }
    }

    fn device_row(&mut self, ui: &mut egui::Ui, rect: Rect, peer: &Peer, index: usize) {
        let resp = ui.interact(rect, Id::unique(("peer", index)), Sense::click());
        let selected = self.selected == Some(peer.id);
        let st = States::of(&resp, selected, true);
        let p = ui.painter();

        let fill = if st.selected {
            theme::accent_wash()
        } else if st.hovered {
            theme::RAISED
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            p.rect_filled(rect, widgets::cr(theme::R_CARD), fill);
        }

        widgets::avatar(
            p,
            pos2(rect.min.x + 26.0, rect.center().y),
            15.0,
            index as u32,
            &widgets::initial(&peer.name),
        );

        widgets::text_left(
            p,
            pos2(rect.min.x + 52.0, rect.center().y - 9.0),
            widgets::ellipsize(&peer.name, 22),
            theme::headline(),
            theme::LABEL,
        );
        widgets::text_left(
            p,
            pos2(rect.min.x + 52.0, rect.center().y + 8.0),
            format!("{} · {}", kind_label(peer.kind), peer.addr),
            theme::caption(),
            theme::LABEL3,
        );

        if peer.online {
            p.circle_filled(
                pos2(rect.max.x - 16.0, rect.center().y),
                3.0,
                theme::PROGRESS,
            );
        }

        if resp.clicked() {
            self.selected = Some(peer.id);
        }
        if st.hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    fn empty_peers(p: &egui::Painter, rect: Rect, y: f32) {
        let card = Rect::from_min_size(
            pos2(rect.min.x + theme::S5, y),
            vec2(rect.width() - theme::S5 * 2.0, 96.0),
        );
        p.rect_filled(card, widgets::cr(theme::R_CARD), theme::RAISED);

        widgets::text_left(
            p,
            pos2(card.min.x + theme::S4, card.min.y + 28.0),
            "No devices yet",
            theme::callout(),
            theme::LABEL2,
        );
        widgets::text_left(
            p,
            pos2(card.min.x + theme::S4, card.min.y + 50.0),
            "Same Wi-Fi, both apps open.",
            theme::caption(),
            theme::LABEL3,
        );
        widgets::text_left(
            p,
            pos2(card.min.x + theme::S4, card.min.y + 68.0),
            "Guest networks often block discovery.",
            theme::caption(),
            theme::LABEL3,
        );
    }
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

impl App {
    fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect, m: &UiModel, narrow: bool) {
        ui.painter().rect_filled(rect, 0, theme::BG);

        let pad = if narrow { theme::S3 } else { theme::S5 };
        let area = Rect::from_min_max(
            pos2(rect.min.x + pad, rect.min.y + pad),
            pos2(rect.max.x - pad, rect.max.y - pad),
        );

        let cta_h = 34.0;
        let drop = Rect::from_min_max(area.min, pos2(area.max.x, area.max.y - cta_h - theme::S4));

        if self.queue.is_empty() {
            self.drop_zone(ui, drop);
        } else {
            self.queue_list(ui, drop);
        }

        let enabled = !self.queue.is_empty() && self.selected.is_some();
        let label = if self.queue.is_empty() {
            "Choose files"
        } else {
            "Send"
        };
        let w = 132.0;
        let btn = Rect::from_min_size(pos2(area.max.x - w, area.max.y - cta_h), vec2(w, cta_h));
        let resp = ui.interact(btn, Id::unique("cta"), Sense::click());
        let st = States::of(&resp, false, enabled);
        let kind = if enabled {
            ButtonKind::Primary
        } else {
            ButtonKind::Secondary
        };
        widgets::button(ui.painter(), btn, st, label, kind);
        if resp.clicked() && enabled {
            self.send();
        }

        if !self.queue.is_empty() {
            let clear =
                Rect::from_min_size(pos2(area.min.x, area.max.y - cta_h), vec2(88.0, cta_h));
            let r2 = ui.interact(clear, Id::unique("clear"), Sense::click());
            let st2 = States::of(&r2, false, true);
            widgets::button(ui.painter(), clear, st2, "Clear", ButtonKind::Ghost);
            if r2.clicked() {
                self.queue.clear();
            }
        }

        if enabled {
            let name = m
                .peers
                .iter()
                .find(|p| Some(p.id) == self.selected)
                .map_or(String::new(), |p| p.name.clone());
            widgets::text_right(
                ui.painter(),
                pos2(area.max.x - w - theme::S3, area.max.y - cta_h / 2.0),
                format!("to {name}"),
                theme::caption(),
                theme::LABEL3,
            );
        }
    }

    fn drop_zone(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let resp = ui.interact(rect, Id::unique("drop"), Sense::click());
        let p = ui.painter();

        let hovering = resp.hovered();
        p.rect_filled(
            rect,
            widgets::cr(theme::R_PANEL),
            if hovering {
                theme::RAISED
            } else {
                theme::PANEL
            },
        );
        // No border at rest. A permanently outlined rectangle reads as a card
        // inside the canvas, which is exactly the nesting this refactor is
        // removing; the outline appears when it means "you can drop here".
        if hovering {
            p.rect_stroke(
                rect.shrink(0.5),
                widgets::cr(theme::R_PANEL),
                Stroke::new(1.0, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
        }

        let c = rect.center();
        widgets::icon(
            p,
            pos2(c.x, c.y - 30.0),
            26.0,
            widgets::Icon::Folder,
            if hovering {
                theme::ACCENT
            } else {
                theme::LABEL3
            },
        );

        #[cfg(not(target_os = "android"))]
        let (title, hint) = ("Drop files here", "or click to choose");
        #[cfg(target_os = "android")]
        let (title, hint) = (
            "Ready to receive",
            "sending from this phone is not wired up yet",
        );

        widgets::text_center(
            p,
            pos2(c.x, c.y + 8.0),
            title,
            theme::headline(),
            theme::LABEL,
        );
        widgets::text_center(
            p,
            pos2(c.x, c.y + 28.0),
            hint,
            theme::callout(),
            theme::LABEL3,
        );

        if resp.clicked() {
            self.pick_files();
        }
        if hovering {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    fn queue_list(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let p = ui.painter();
        p.rect_filled(rect, widgets::cr(theme::R_PANEL), theme::PANEL);
        p.rect_stroke(
            rect.shrink(0.5),
            widgets::cr(theme::R_PANEL),
            Stroke::new(1.0, theme::SEP),
            egui::StrokeKind::Inside,
        );

        let mut y = rect.min.y + theme::S3;
        let mut remove: Option<usize> = None;

        for (i, q) in self.queue.iter().enumerate() {
            let row = Rect::from_min_size(
                pos2(rect.min.x + theme::S3, y),
                vec2(rect.width() - theme::S3 * 2.0, 34.0),
            );
            if row.max.y > rect.max.y - theme::S3 {
                break;
            }

            widgets::text_left(
                p,
                pos2(row.min.x, row.center().y),
                widgets::ellipsize(&q.name(), 40),
                theme::body(),
                theme::LABEL,
            );
            widgets::text_right(
                p,
                pos2(row.max.x - 26.0, row.center().y),
                widgets::fmt_bytes(q.size),
                theme::mono(11.0),
                theme::LABEL3,
            );

            let x_btn =
                Rect::from_center_size(pos2(row.max.x - 10.0, row.center().y), vec2(18.0, 18.0));
            let xr = ui.interact(x_btn, Id::unique(("rm", i)), Sense::click());
            if xr.hovered() {
                ui.painter()
                    .rect_filled(x_btn, widgets::cr(theme::R_CONTROL), theme::SEP);
            }
            widgets::icon(
                ui.painter(),
                x_btn.center(),
                8.0,
                widgets::Icon::Close,
                if xr.hovered() {
                    theme::LABEL
                } else {
                    theme::LABEL3
                },
            );
            if xr.clicked() {
                remove = Some(i);
            }

            y += 34.0;
        }

        if let Some(i) = remove {
            self.queue.remove(i);
        }
    }
}

// ---------------------------------------------------------------------------
// Transfers
// ---------------------------------------------------------------------------

impl App {
    fn bottom_panel(&mut self, ui: &mut egui::Ui, rect: Rect, m: &UiModel, expanded: bool) {
        let p = ui.painter();
        p.rect_filled(rect, 0, theme::PANEL);
        widgets::divider(p, rect.min.x, rect.max.x, rect.min.y + 0.5);

        let header = Rect::from_min_size(rect.min, vec2(rect.width(), BOTTOM_COLLAPSED));
        let resp = ui.interact(header, Id::unique("bottom-toggle"), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(header, 0, theme::RAISED);
        }
        if resp.clicked() && !m.transfers.is_empty() {
            self.bottom_open = !self.bottom_open;
        }

        Self::section_header(
            ui.painter(),
            rect.min.x + theme::S5,
            header.center().y,
            "TRANSFERS",
            m.transfers.len(),
        );

        widgets::icon(
            ui.painter(),
            pos2(rect.max.x - theme::S5, header.center().y),
            10.0,
            widgets::Icon::Chevron,
            theme::LABEL3,
        );

        if !expanded {
            // Collapsed: one hairline along the top edge carrying the most
            // relevant progress, so a transfer is visible without expanding.
            if let Some(t) = m
                .transfers
                .iter()
                .find(|t| t.state == TransferState::Active)
            {
                let bar = Rect::from_min_size(
                    pos2(rect.min.x, rect.min.y + 0.5),
                    vec2(rect.width() * t.fraction(), 2.0),
                );
                ui.painter().rect_filled(bar, 0, theme::PROGRESS);
            }
            return;
        }

        let mut y = rect.min.y + BOTTOM_COLLAPSED;
        for (i, t) in m.transfers.iter().enumerate() {
            let row = Rect::from_min_size(
                pos2(rect.min.x + theme::S5, y),
                vec2(rect.width() - theme::S5 * 2.0, XFER_H),
            );
            if row.max.y > rect.max.y - theme::S2 {
                break;
            }
            self.transfer_row(ui, row, t, i);
            y += XFER_H;
        }
    }

    fn transfer_row(&mut self, ui: &mut egui::Ui, rect: Rect, t: &TransferRow, index: usize) {
        let p = ui.painter();

        widgets::text_left(
            p,
            pos2(rect.min.x, rect.min.y + 14.0),
            widgets::ellipsize(&t.file, 44),
            theme::body(),
            theme::LABEL,
        );

        let (status, color) = match t.state {
            TransferState::Active => {
                let eta = t
                    .eta_secs()
                    .map_or_else(String::new, |s| format!(" · {}", widgets::fmt_eta(s)));
                (
                    format!(
                        "{} / {}{eta}",
                        widgets::fmt_bytes(t.bytes_done),
                        widgets::fmt_bytes(t.total)
                    ),
                    theme::LABEL2,
                )
            }
            TransferState::Pending => ("waiting".to_string(), theme::LABEL3),
            TransferState::Done => (widgets::fmt_bytes(t.total), theme::PROGRESS),
            TransferState::Failed => ("failed".to_string(), theme::DANGER),
        };

        widgets::text_right(
            p,
            pos2(rect.max.x - 26.0, rect.min.y + 14.0),
            status,
            theme::mono(11.0),
            color,
        );

        let bar = Rect::from_min_size(
            pos2(rect.min.x, rect.min.y + 34.0),
            vec2(rect.width() - 26.0, 3.0),
        );
        let (fraction, fill) = match t.state {
            TransferState::Active | TransferState::Done => (t.fraction(), theme::PROGRESS),
            TransferState::Failed => (t.fraction(), theme::DANGER),
            TransferState::Pending => (0.0, theme::SEP),
        };
        widgets::progress(p, bar, fraction, fill);

        widgets::text_left(
            p,
            pos2(rect.min.x, rect.min.y + 46.0),
            format!(
                "{} · {}",
                widgets::ellipsize(&t.peer, 18),
                widgets::fmt_speed(t.speed_bps)
            ),
            theme::caption(),
            theme::LABEL3,
        );

        if matches!(t.state, TransferState::Active | TransferState::Pending) {
            let x_btn =
                Rect::from_center_size(pos2(rect.max.x - 8.0, rect.center().y), vec2(20.0, 20.0));
            let xr = ui.interact(x_btn, Id::unique(("cancel", index)), Sense::click());
            if xr.hovered() {
                ui.painter()
                    .rect_filled(x_btn, widgets::cr(theme::R_CONTROL), theme::SEP);
            }
            widgets::icon(
                ui.painter(),
                x_btn.center(),
                8.0,
                widgets::Icon::Close,
                if xr.hovered() {
                    theme::LABEL
                } else {
                    theme::LABEL3
                },
            );
            if xr.clicked() {
                let _ = self.actions.try_send(UiAction::Cancel(t.id.clone()));
            }
        }
    }
}

fn kind_label(k: DeviceKind) -> &'static str {
    match k {
        DeviceKind::Desktop => "desktop",
        DeviceKind::Laptop => "laptop",
        DeviceKind::Phone => "phone",
        DeviceKind::Tablet => "tablet",
        DeviceKind::Server => "server",
        DeviceKind::Unknown => "device",
    }
}
