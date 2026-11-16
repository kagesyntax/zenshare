//! The `ZenShare` interface.
//!
//! Everything is painted directly onto one canvas. There is no widget theme to
//! fight and no layout engine to negotiate with, which is what keeps the app
//! from looking like every other egui project.
//!
//! The UI owns exactly two things: the outgoing file queue and the selected
//! peer. Everything else — peers, transfers, status — arrives as an immutable
//! snapshot from the engine, so the render loop can never block on I/O.

#![deny(missing_docs)]
// Short closure and loop bindings (`|m|`, `|p|`, `for i in`) are idiomatic in
// tight drawing code and add noise as long names.
#![allow(clippy::many_single_char_names)]

pub mod theme;
pub mod widgets;

use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use widgets::{Icon, ellipsize, fmt_bytes, fmt_eta, fmt_speed, icon, initial};
use zs_core::{DeviceId, DeviceKind};

/// A peer as the UI sees it.
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

/// Lifecycle of one transfer row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// Completion in `0.0..=1.0`.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            (self.bytes_done as f64 / self.total as f64) as f32
        }
    }

    /// Seconds remaining, or `None` when unknown.
    #[must_use]
    pub fn eta_secs(&self) -> Option<f64> {
        if self.speed_bps <= 1.0 || self.bytes_done >= self.total {
            return None;
        }
        Some((self.total - self.bytes_done) as f64 / self.speed_bps)
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
    /// One-line status shown at the bottom.
    pub status: String,
}

impl UiModel {
    /// A populated model for design work and screenshots. Never used at runtime.
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
        }
    }
}

/// Commands the UI sends to the engine.
#[derive(Debug)]
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

/// Launch the window. Blocks until the user closes it.
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
        .with_min_inner_size([900.0, 580.0])
        .with_title("ZenShare");

    // Keep the progress window above everything else while a transfer runs.
    // Also handy for driving the app from a script or a screenshot harness.
    if on_top || std::env::var_os("ZENSHARE_ON_TOP").is_some() {
        viewport = viewport.with_always_on_top();
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "ZenShare",
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::install_style(&cc.egui_ctx);
            Ok(Box::new(App::new(model, actions, demo)))
        }),
    )
}

struct App {
    model: Arc<RwLock<UiModel>>,
    actions: async_channel::Sender<UiAction>,
    queue: Vec<QueuedFile>,
    selected: Option<DeviceId>,
    demo: bool,
}

impl App {
    fn new(
        model: Arc<RwLock<UiModel>>,
        actions: async_channel::Sender<UiAction>,
        demo: bool,
    ) -> Self {
        Self {
            model,
            actions,
            queue: Vec::new(),
            selected: None,
            demo,
        }
    }

    /// In demo mode the active row advances, so the window visibly animates and
    /// a screenshot taken later proves the paint loop is running.
    fn tick_demo(&mut self) {
        if !self.demo {
            return;
        }
        if let Ok(mut m) = self.model.write()
            && let Some(row) = m
                .transfers
                .iter_mut()
                .find(|t| t.state == TransferState::Active)
        {
            row.bytes_done = (row.bytes_done + 24_000_000).min(row.total);
        }
    }

    fn snapshot(&self) -> UiModel {
        self.model.read().map(|m| m.clone()).unwrap_or_default()
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            if let Some(q) = QueuedFile::new(p)
                && !self.queue.iter().any(|e| e.path == q.path)
            {
                self.queue.push(q);
            }
        }
    }

    fn pick_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Choose files to send")
            .pick_files()
        {
            self.add_files(paths);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, widgets::cr(0), theme::BG);

        // Files dropped onto the window.
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

        let pad = 26.0;
        let header_h = 76.0;

        Self::top_bar(ui, full, pad, header_h, &m);

        let body_top = header_h + 6.0;
        let body_bottom = full.height() - 30.0;
        let left_x = pad;
        let left_w = (full.width() * 0.38).clamp(320.0, 400.0);
        let right_x = left_x + left_w + pad;
        let right_w = full.width() - right_x - pad;

        self.devices_panel(
            ui,
            Rect::from_min_max(
                Pos2::new(left_x, body_top),
                Pos2::new(left_x + left_w, body_bottom),
            ),
            &m,
        );

        self.transfers_panel(
            ui,
            Rect::from_min_max(
                Pos2::new(left_x, body_top),
                Pos2::new(left_x + left_w, body_bottom),
            ),
            &m,
        );

        self.send_panel(
            ui,
            Rect::from_min_max(
                Pos2::new(right_x, body_top),
                Pos2::new(right_x + right_w, body_bottom),
            ),
            &m,
        );

        Self::status_bar(ui, full, pad, &m);

        // egui only repaints on input by default. The engine mutates the model
        // from another thread and cannot poke the window, so we tick: fast while
        // something is moving, slowly when idle. Without this the window shows a
        // stale frame until the mouse moves.
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

impl App {
    fn top_bar(ui: &mut egui::Ui, full: Rect, pad: f32, h: f32, m: &UiModel) {
        let p = ui.painter();
        let cy = h / 2.0;

        // Logo mark.
        let mark = Rect::from_center_size(Pos2::new(pad + 17.0, cy), vec2(34.0, 34.0));
        p.rect_filled(mark, widgets::cr(10), theme::ACCENT.gamma_multiply(0.14));
        p.rect_stroke(
            mark,
            widgets::cr(10),
            Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.30)),
            StrokeKind::Inside,
        );
        icon(p, mark.center(), 17.0, Icon::Send, theme::ACCENT);

        widgets::text_left(
            p,
            Pos2::new(pad + 46.0, cy),
            "ZenShare",
            theme::bold(19.0),
            theme::TEXT,
        );

        // This device, on the right. The port lives in the status bar; repeating
        // it here only ever caused the label to clip.
        let label = ellipsize(&m.my_name, 24);
        let text_w = label.chars().count() as f32 * 6.3;
        let w = (text_w + 44.0).clamp(120.0, 260.0);
        let chip = Rect::from_min_size(Pos2::new(full.width() - pad - w, cy - 16.0), vec2(w, 32.0));
        widgets::pill(p, chip, theme::SURFACE);
        p.circle_filled(Pos2::new(chip.min.x + 15.0, cy), 3.5, theme::ACCENT);
        widgets::text_left(
            p,
            Pos2::new(chip.min.x + 27.0, cy),
            label,
            theme::regular(12.5),
            theme::TEXT_DIM,
        );

        // Hairline under the bar.
        p.line_segment(
            [Pos2::new(pad, h), Pos2::new(full.width() - pad, h)],
            Stroke::new(1.0, theme::BORDER.gamma_multiply(0.7)),
        );
    }

    fn devices_panel(&mut self, ui: &mut egui::Ui, area: Rect, m: &UiModel) {
        let p = ui.painter().clone();
        let mut y = area.min.y + 8.0;

        widgets::text_left(
            &p,
            Pos2::new(area.min.x, y + 8.0),
            "Nearby",
            theme::bold(13.0),
            theme::TEXT,
        );
        let count = format!("{}", m.peers.len());
        let cw = 26.0;
        let chip = Rect::from_min_size(Pos2::new(area.min.x + 66.0, y), vec2(cw, 18.0));
        widgets::pill(&p, chip, theme::SURFACE_HI);
        widgets::text_center(&p, chip.center(), count, theme::mono(11.0), theme::TEXT_DIM);

        y += 30.0;

        let row_h = 70.0;
        let gap = 8.0;
        // Reserve room for the transfer list when there is one.
        let reserve = if m.transfers.is_empty() { 0.0 } else { 232.0 };
        let max_rows = (((area.height() - 30.0 - reserve) / (row_h + gap)) as usize).max(1);

        if m.peers.is_empty() {
            draw_empty_state(&p, area, y);
            return;
        }

        for (i, peer) in m.peers.iter().take(max_rows).enumerate() {
            let rect = Rect::from_min_size(Pos2::new(area.min.x, y), vec2(area.width(), row_h));
            let id = ui.scope_id().with(("peer", i));
            let resp = ui.interact(rect, id, Sense::click());
            let hover = ui
                .ctx()
                .animate_bool_with_time(id.with("h"), resp.hovered(), 0.12);
            let selected = self.selected == Some(peer.id);

            let fill = if selected {
                theme::ACCENT.gamma_multiply(0.10)
            } else {
                theme::SURFACE.gamma_multiply(0.55 + 0.45 * hover)
            };
            let border = if selected {
                theme::ACCENT.gamma_multiply(0.45)
            } else {
                theme::BORDER.gamma_multiply(0.6 + 0.4 * hover)
            };
            widgets::card_bordered(&p, rect, fill, border, theme::RADIUS);

            let cx = rect.min.x + 40.0;
            widgets::avatar(
                &p,
                Pos2::new(cx, rect.center().y),
                21.0,
                u32::from(peer.id.0[0]),
                &initial(&peer.name),
            );

            let name = ellipsize(&peer.name, 24);
            widgets::text_left(
                &p,
                Pos2::new(rect.min.x + 72.0, rect.center().y - 10.0),
                name,
                theme::medium(14.0),
                theme::TEXT,
            );
            let sub = format!("{}  ·  {}", kind_label(peer.kind), peer.addr);
            widgets::text_left(
                &p,
                Pos2::new(rect.min.x + 72.0, rect.center().y + 10.0),
                ellipsize(&sub, 30),
                theme::regular(11.5),
                theme::TEXT_FAINT,
            );

            if selected {
                icon(
                    &p,
                    Pos2::new(rect.max.x - 26.0, rect.center().y),
                    16.0,
                    Icon::Check,
                    theme::ACCENT,
                );
            } else if hover > 0.5 {
                icon(
                    &p,
                    Pos2::new(rect.max.x - 26.0, rect.center().y),
                    14.0,
                    Icon::Chevron,
                    theme::TEXT_DIM,
                );
            }

            if resp.clicked() {
                self.selected = Some(peer.id);
            }

            y += row_h + gap;
        }
    }

    fn transfers_panel(&mut self, ui: &mut egui::Ui, area: Rect, m: &UiModel) {
        if m.transfers.is_empty() {
            return;
        }
        let p = ui.painter().clone();
        let rows = m.transfers.len().min(2);
        let block_h = 30.0 + rows as f32 * 88.0;
        let mut y = area.max.y - block_h;

        widgets::text_left(
            &p,
            Pos2::new(area.min.x, y + 8.0),
            "Transfers",
            theme::bold(13.0),
            theme::TEXT,
        );
        y += 28.0;

        for (i, t) in m.transfers.iter().take(rows).enumerate() {
            let rect = Rect::from_min_size(Pos2::new(area.min.x, y), vec2(area.width(), 80.0));
            widgets::card_bordered(&p, rect, theme::SURFACE, theme::BORDER, theme::RADIUS);

            let accent = if t.state == TransferState::Failed {
                theme::DANGER
            } else {
                theme::ACCENT
            };

            widgets::text_left(
                &p,
                Pos2::new(rect.min.x + 16.0, rect.min.y + 22.0),
                ellipsize(&t.file, 26),
                theme::medium(13.0),
                theme::TEXT,
            );
            widgets::text_right(
                &p,
                Pos2::new(rect.max.x - 16.0, rect.min.y + 22.0),
                format!("{:.0}%", t.fraction() * 100.0),
                theme::mono(12.0),
                accent,
            );

            let bar = Rect::from_min_size(
                Pos2::new(rect.min.x + 16.0, rect.min.y + 40.0),
                vec2(rect.width() - 32.0, 5.0),
            );
            widgets::progress(&p, bar, t.fraction(), accent);

            let detail = match t.state {
                TransferState::Done => format!("{}  ·  complete", fmt_bytes(t.total)),
                TransferState::Failed => format!("{}  ·  failed", fmt_bytes(t.bytes_done)),
                TransferState::Pending => "waiting for the other device…".to_string(),
                TransferState::Active => {
                    let eta = t.eta_secs().map_or_else(|| "—".to_string(), fmt_eta);
                    format!(
                        "{}  ·  {}  ·  {} left",
                        fmt_bytes(t.bytes_done),
                        fmt_speed(t.speed_bps),
                        eta
                    )
                }
            };
            widgets::text_left(
                &p,
                Pos2::new(rect.min.x + 16.0, rect.min.y + 60.0),
                detail,
                theme::mono(11.0),
                theme::TEXT_FAINT,
            );

            let close = Rect::from_center_size(
                Pos2::new(rect.max.x - 22.0, rect.min.y + 60.0),
                vec2(18.0, 18.0),
            );
            let cresp = ui.interact(close, ui.scope_id().with(("cancel", i)), Sense::click());
            if cresp.hovered() {
                p.rect_filled(close, widgets::cr(4), theme::SURFACE_HI);
            }
            icon(&p, close.center(), 12.0, Icon::Close, theme::TEXT_FAINT);
            if cresp.clicked() {
                let _ = self.actions.try_send(UiAction::Cancel(t.id.clone()));
            }

            y += 88.0;
        }
    }

    // A panel is a linear list of draw calls; splitting it would only move
    // parameters around.
    #[allow(clippy::too_many_lines)]
    fn send_panel(&mut self, ui: &mut egui::Ui, area: Rect, m: &UiModel) {
        let p = ui.painter().clone();
        widgets::card_bordered(&p, area, theme::SURFACE, theme::BORDER, 16);

        let pad = 20.0;
        let inner = area.shrink(pad);
        let mut y = inner.min.y;

        widgets::text_left(
            &p,
            Pos2::new(inner.min.x, y + 6.0),
            "Send",
            theme::bold(15.0),
            theme::TEXT,
        );
        let total: u64 = self.queue.iter().map(|q| q.size).sum();
        if !self.queue.is_empty() {
            widgets::text_right(
                &p,
                Pos2::new(inner.max.x, y + 6.0),
                format!("{} files  ·  {}", self.queue.len(), fmt_bytes(total)),
                theme::mono(11.0),
                theme::TEXT_FAINT,
            );
        }
        y += 26.0;

        // Button geometry reserved from the bottom.
        let btn_h = 46.0;
        let btn = Rect::from_min_size(
            Pos2::new(inner.min.x, inner.max.y - btn_h),
            vec2(inner.width(), btn_h),
        );

        let drop_top = y;
        let drop_bottom = if self.queue.is_empty() {
            btn.min.y - 18.0
        } else {
            y + 132.0
        };
        let drop = Rect::from_min_max(
            Pos2::new(inner.min.x, drop_top),
            Pos2::new(inner.max.x, drop_bottom.max(drop_top + 90.0)),
        );

        let did = ui.scope_id().with("dropzone");
        let dresp = ui.interact(drop, did, Sense::click());
        let dhover = ui
            .ctx()
            .animate_bool_with_time(did.with("h"), dresp.hovered(), 0.12);

        let fill = theme::SURFACE_HI.gamma_multiply(0.35 + 0.45 * dhover);
        p.rect_filled(drop, widgets::cr(theme::RADIUS), fill);
        p.rect_stroke(
            drop,
            widgets::cr(theme::RADIUS),
            Stroke::new(1.0, theme::BORDER.gamma_multiply(0.8 + 0.2 * dhover)),
            StrokeKind::Inside,
        );

        if self.queue.is_empty() {
            let c = drop.center();
            icon(
                &p,
                Pos2::new(c.x, c.y - 34.0),
                26.0,
                Icon::Folder,
                theme::TEXT_FAINT,
            );
            widgets::text_center(
                &p,
                Pos2::new(c.x, c.y + 2.0),
                "Drop files here",
                theme::medium(14.5),
                theme::TEXT,
            );
            widgets::text_center(
                &p,
                Pos2::new(c.x, c.y + 24.0),
                "or click to browse",
                theme::regular(12.0),
                theme::TEXT_FAINT,
            );
            if dresp.clicked() {
                self.pick_files();
            }
        } else {
            if dresp.clicked() {
                self.pick_files();
            }
            let mut ry = drop.min.y + 12.0;
            let row_h = 38.0;
            let max_rows = (((drop.height() - 20.0) / row_h) as usize).max(1);
            let shown = self.queue.len().min(max_rows);

            for i in 0..shown {
                let f = &self.queue[i];
                let r = Rect::from_min_size(
                    Pos2::new(drop.min.x + 12.0, ry),
                    vec2(drop.width() - 24.0, row_h),
                );
                widgets::text_left(
                    &p,
                    Pos2::new(r.min.x + 6.0, r.center().y),
                    ellipsize(&f.name(), 30),
                    theme::regular(12.5),
                    theme::TEXT,
                );
                widgets::text_right(
                    &p,
                    Pos2::new(r.max.x - 24.0, r.center().y),
                    fmt_bytes(f.size),
                    theme::mono(10.5),
                    theme::TEXT_FAINT,
                );

                let x = Rect::from_center_size(
                    Pos2::new(r.max.x - 8.0, r.center().y),
                    vec2(16.0, 16.0),
                );
                let xr = ui.interact(x, ui.scope_id().with(("rm", i)), Sense::click());
                icon(
                    &p,
                    x.center(),
                    11.0,
                    Icon::Close,
                    if xr.hovered() {
                        theme::DANGER
                    } else {
                        theme::TEXT_FAINT
                    },
                );
                if xr.clicked() {
                    self.queue.remove(i);
                    break;
                }
                ry += row_h;
            }

            if self.queue.len() > shown {
                widgets::text_left(
                    &p,
                    Pos2::new(drop.min.x + 18.0, ry + 6.0),
                    format!("+{} more", self.queue.len() - shown),
                    theme::regular(11.0),
                    theme::TEXT_FAINT,
                );
            }
        }

        // Destination line above the button.
        let dest_y = btn.min.y - 22.0;
        let (dest_text, dest_color) = match self
            .selected
            .and_then(|id| m.peers.iter().find(|x| x.id == id))
        {
            Some(peer) => (format!("to {}", peer.name), theme::TEXT_DIM),
            None if m.peers.is_empty() => ("no device selected".to_string(), theme::TEXT_FAINT),
            None => ("select a device above".to_string(), theme::TEXT_FAINT),
        };
        widgets::text_left(
            &p,
            Pos2::new(inner.min.x, dest_y),
            dest_text,
            theme::regular(11.5),
            dest_color,
        );

        let enabled = !self.queue.is_empty() && self.selected.is_some();
        let bid = ui.scope_id().with("sendbtn");
        let bresp = ui.interact(
            btn,
            bid,
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let bhover =
            ui.ctx()
                .animate_bool_with_time(bid.with("h"), bresp.hovered() && enabled, 0.10);

        let fill = if enabled {
            theme::ACCENT.gamma_multiply(0.88 + 0.12 * bhover)
        } else {
            theme::SURFACE_HI
        };
        p.rect_filled(btn, widgets::cr(12), fill);
        let label_color = if enabled {
            Color32::from_rgb(0x05, 0x14, 0x0F)
        } else {
            theme::TEXT_FAINT
        };
        let label = if enabled {
            "Send"
        } else {
            "Select a device and files"
        };
        widgets::text_center(&p, btn.center(), label, theme::medium(14.0), label_color);

        if bresp.clicked()
            && enabled
            && let Some(peer) = self.selected
        {
            let files: Vec<PathBuf> = self.queue.iter().map(|q| q.path.clone()).collect();
            let _ = self.actions.try_send(UiAction::Send { peer, files });
            self.queue.clear();
        }
    }

    fn status_bar(ui: &mut egui::Ui, full: Rect, pad: f32, m: &UiModel) {
        let p = ui.painter();
        let y = full.height() - 15.0;
        let text = if m.status.is_empty() {
            "encrypted end to end · no account · no telemetry"
        } else {
            m.status.as_str()
        };
        widgets::text_left(
            p,
            Pos2::new(pad, y),
            text,
            theme::regular(10.5),
            theme::TEXT_FAINT,
        );

        let hint = "drop files anywhere";
        widgets::text_right(
            p,
            Pos2::new(full.width() - pad, y),
            hint,
            theme::regular(10.5),
            theme::TEXT_FAINT,
        );
    }
}

/// What the device list shows before anything is discovered. Deliberately tells
/// the user what to do rather than only that something is empty.
fn draw_empty_state(p: &egui::Painter, area: Rect, y: f32) {
    let r = Rect::from_min_size(Pos2::new(area.min.x, y), vec2(area.width(), 104.0));
    widgets::card_bordered(p, r, theme::SURFACE, theme::BORDER, theme::RADIUS);

    let lines = [
        (
            "Looking for devices…",
            theme::medium(13.5),
            theme::TEXT_DIM,
            30.0,
        ),
        (
            "Both devices must be on the same Wi-Fi.",
            theme::regular(12.0),
            theme::TEXT_FAINT,
            54.0,
        ),
        (
            "Ask the other device to open ZenShare.",
            theme::regular(12.0),
            theme::TEXT_FAINT,
            74.0,
        ),
    ];
    for (text, font, color, dy) in lines {
        widgets::text_left(
            p,
            Pos2::new(r.min.x + 18.0, r.min.y + dy),
            text,
            font,
            color,
        );
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
