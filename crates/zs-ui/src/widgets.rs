//! Painted primitives.
//!
//! Everything here draws straight onto the canvas. No widget library is
//! involved, which is why the app does not look like one.

use crate::theme;
use egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, vec2,
};

/// Round a rect by `r`.
#[must_use]
pub fn cr(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

/// A surface card.
pub fn card(p: &Painter, rect: Rect, fill: Color32, radius: u8) {
    p.rect_filled(rect, cr(radius), fill);
}

/// A card with a hairline border.
pub fn card_bordered(p: &Painter, rect: Rect, fill: Color32, border: Color32, radius: u8) {
    p.rect_filled(rect, cr(radius), fill);
    p.rect_stroke(
        rect,
        cr(radius),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
}

/// A pill / chip.
pub fn pill(p: &Painter, rect: Rect, fill: Color32) {
    p.rect_filled(rect, cr(theme::R_PILL), fill);
}

/// Text with a left anchor.
pub fn text_left(p: &Painter, pos: Pos2, s: impl ToString, font: FontId, color: Color32) -> Rect {
    p.text(pos, Align2::LEFT_CENTER, s, font, color)
}

/// Text with a right anchor.
pub fn text_right(p: &Painter, pos: Pos2, s: impl ToString, font: FontId, color: Color32) -> Rect {
    p.text(pos, Align2::RIGHT_CENTER, s, font, color)
}

/// Text centred on a point.
pub fn text_center(p: &Painter, pos: Pos2, s: impl ToString, font: FontId, color: Color32) -> Rect {
    p.text(pos, Align2::CENTER_CENTER, s, font, color)
}

/// Progress bar: track + fill, fully rounded.
pub fn progress(p: &Painter, rect: Rect, fraction: f32, color: Color32) {
    let r = cr((rect.height() / 2.0) as u8);
    p.rect_filled(rect, r, theme::SEP);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        // Never narrower than the track height: a 0.4% transfer should read as
        // "started", not as an empty bar.
        let w = (rect.width() * f).max(rect.height());
        let filled = Rect::from_min_size(rect.min, vec2(w, rect.height()));
        p.rect_filled(filled, r, color);
    }
}

/// The little identity disc shown for each peer.
pub fn avatar(p: &Painter, center: Pos2, radius: f32, seed: u32, initials: &str) {
    // A hue derived from the device id, so every peer gets its own colour and
    // keeps it. Two fixed accents meant half the devices looked identical.
    let base = hue_color(seed);
    p.circle_filled(center, radius, base.gamma_multiply(0.14));
    p.circle_stroke(center, radius, Stroke::new(1.0, base.gamma_multiply(0.35)));
    p.text(
        center,
        Align2::CENTER_CENTER,
        initials,
        theme::medium(radius * 0.72),
        base.gamma_multiply(0.95),
    );
}

/// Icon set. Drawn as strokes so they stay crisp at any DPI and cost no assets.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Paper plane.
    Send,
    /// Downward arrow into a tray.
    Receive,
    /// Clock face.
    History,
    /// Gear.
    Settings,
    /// Plus sign.
    Plus,
    /// Folder.
    Folder,
    /// Check mark.
    Check,
    /// Cross.
    Close,
    /// Chevron pointing right.
    Chevron,
}

/// Draw an icon centred on `center` with a nominal size of `size`.
// One match arm per icon, each a handful of strokes. Splitting it into nine
// functions would scatter closely-related geometry for no structural gain.
#[allow(clippy::too_many_lines)]
pub fn icon(painter: &Painter, center: Pos2, size: f32, which: Icon, color: Color32) {
    let w = Stroke::new(1.7, color);
    let h = size * 0.5;
    match which {
        Icon::Send => {
            painter.line_segment(
                [
                    Pos2::new(center.x - h, center.y + h * 0.35),
                    Pos2::new(center.x + h, center.y - h * 0.55),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + h, center.y - h * 0.55),
                    Pos2::new(center.x + h * 0.15, center.y + h * 0.55),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + h * 0.15, center.y + h * 0.55),
                    Pos2::new(center.x - h * 0.15, center.y - h * 0.05),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.15, center.y - h * 0.05),
                    Pos2::new(center.x + h, center.y - h * 0.55),
                ],
                w,
            );
        }
        Icon::Receive => {
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - h),
                    Pos2::new(center.x, center.y + h * 0.3),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.45, center.y - h * 0.15),
                    Pos2::new(center.x, center.y + h * 0.3),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + h * 0.45, center.y - h * 0.15),
                    Pos2::new(center.x, center.y + h * 0.3),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.7, center.y + h * 0.7),
                    Pos2::new(center.x + h * 0.7, center.y + h * 0.7),
                ],
                w,
            );
        }
        Icon::History => {
            painter.circle_stroke(center, h * 0.8, w);
            painter.line_segment([center, Pos2::new(center.x, center.y - h * 0.45)], w);
            painter.line_segment(
                [center, Pos2::new(center.x + h * 0.4, center.y + h * 0.15)],
                w,
            );
        }
        Icon::Settings => {
            painter.circle_stroke(center, h * 0.42, w);
            for i in 0..8 {
                let a = std::f32::consts::TAU * (i as f32) / 8.0;
                let (sn, cs) = a.sin_cos();
                painter.line_segment(
                    [
                        Pos2::new(center.x + cs * h * 0.62, center.y + sn * h * 0.62),
                        Pos2::new(center.x + cs * h * 0.92, center.y + sn * h * 0.92),
                    ],
                    w,
                );
            }
        }
        Icon::Plus => {
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.7, center.y),
                    Pos2::new(center.x + h * 0.7, center.y),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - h * 0.7),
                    Pos2::new(center.x, center.y + h * 0.7),
                ],
                w,
            );
        }
        Icon::Folder => {
            let r = Rect::from_center_size(
                Pos2::new(center.x, center.y + h * 0.08),
                Vec2::new(h * 1.6, h * 1.1),
            );
            painter.rect_stroke(r, cr(2), w, StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(r.min.x, r.min.y),
                    Pos2::new(r.min.x + h * 0.55, r.min.y),
                ],
                w,
            );
        }
        Icon::Check => {
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.55, center.y),
                    Pos2::new(center.x - h * 0.1, center.y + h * 0.45),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.1, center.y + h * 0.45),
                    Pos2::new(center.x + h * 0.6, center.y - h * 0.45),
                ],
                w,
            );
        }
        Icon::Close => {
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.5, center.y - h * 0.5),
                    Pos2::new(center.x + h * 0.5, center.y + h * 0.5),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + h * 0.5, center.y - h * 0.5),
                    Pos2::new(center.x - h * 0.5, center.y + h * 0.5),
                ],
                w,
            );
        }
        Icon::Chevron => {
            painter.line_segment(
                [
                    Pos2::new(center.x - h * 0.25, center.y - h * 0.45),
                    Pos2::new(center.x + h * 0.25, center.y),
                ],
                w,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + h * 0.25, center.y),
                    Pos2::new(center.x - h * 0.25, center.y + h * 0.45),
                ],
                w,
            );
        }
    }
}

/// Human byte size, binary units, one decimal above 1 KiB.
#[must_use]
pub fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Human throughput.
#[must_use]
pub fn fmt_speed(bps: f64) -> String {
    if bps < 1.0 {
        return "—".to_string();
    }
    format!("{}/s", fmt_bytes(bps as u64))
}

/// Remaining time, coarse on purpose.
#[must_use]
pub fn fmt_eta(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "—".to_string();
    }
    let s = secs as u64;
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    }
}

/// First letter of the device name, upper-cased.
#[must_use]
pub fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(|| "?".into(), |c| c.to_uppercase().to_string())
}

/// Truncate to `max` characters, appending an ellipsis.
#[must_use]
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// Primitives added for the HIG refactor
// ---------------------------------------------------------------------------

/// Deterministic, legible tint for a device, from its seed.
fn hue_color(seed: u32) -> Color32 {
    let hue = (seed.wrapping_mul(2_654_435_761) % 360) as f32;
    let (r, g, b) = hsv_to_rgb(hue, 0.42, 0.92);
    Color32::from_rgb(r, g, b)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

/// 1px hairline from `x0` to `x1` at `y`.
pub fn divider(p: &Painter, x0: f32, x1: f32, y: f32) {
    p.line_segment(
        [Pos2::new(x0, y), Pos2::new(x1, y)],
        Stroke::new(1.0, theme::SEP),
    );
}

/// Visual state of a painted control, derived from its `Response`.
///
/// Four independent booleans rather than an enum, because they are independent:
/// a control can be hovered *and* selected *and* focused at once.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default)]
pub struct States {
    /// Pointer is over it.
    pub hovered: bool,
    /// Primary button is held on it.
    pub pressed: bool,
    /// It is the current selection.
    pub selected: bool,
    /// It can be used.
    pub enabled: bool,
}

impl States {
    /// Derive from a response. `enabled` is separate because a painted control
    /// still receives a response when it is logically disabled.
    #[must_use]
    pub fn of(resp: &egui::Response, selected: bool, enabled: bool) -> Self {
        Self {
            hovered: enabled && resp.hovered(),
            pressed: enabled && resp.is_pointer_button_down_on(),
            selected,
            enabled,
        }
    }
}

/// Button emphasis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// One per view. Accent fill.
    Primary,
    /// Everything else. Control fill.
    Secondary,
    /// Low-stakes inline actions.
    Ghost,
}

/// Paint a button. The caller owns the rect and the response.
// Arms are kept one-per-state rather than merged: the table is the specification
// of how a button looks, and collapsing rows makes it harder to check.
#[allow(clippy::match_same_arms)]
pub fn button(p: &Painter, rect: Rect, states: States, label: &str, kind: ButtonKind) -> Rect {
    let (fill, text_color) = match (kind, states.enabled, states.pressed, states.hovered) {
        (ButtonKind::Primary, false, _, _) => (theme::SEP, theme::LABEL3),
        (ButtonKind::Primary, true, true, _) => (theme::ACCENT_PRESS, theme::ON_ACCENT),
        (ButtonKind::Primary, true, false, true) => {
            (theme::ACCENT.gamma_multiply(0.92), theme::ON_ACCENT)
        }
        (ButtonKind::Primary, true, false, false) => (theme::ACCENT, theme::ON_ACCENT),

        (ButtonKind::Secondary, false, _, _) => (theme::PANEL, theme::LABEL3),
        (ButtonKind::Secondary, true, true, _) => (theme::SEP_STRONG, theme::LABEL),
        (ButtonKind::Secondary, true, false, true) => (theme::RAISED, theme::LABEL),
        (ButtonKind::Secondary, true, false, false) => (theme::CONTROL, theme::LABEL2),

        (ButtonKind::Ghost, false, _, _) => (Color32::TRANSPARENT, theme::LABEL3),
        // Ghost shares its hover treatment with Secondary on purpose: both sit
        // on the same surfaces, so a different hover would look like a bug.
        (ButtonKind::Ghost, true, _, true) => (theme::RAISED, theme::LABEL),
        (ButtonKind::Ghost, true, _, false) => (Color32::TRANSPARENT, theme::LABEL2),
    };

    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect, cr(theme::R_CONTROL), fill);
    }

    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        theme::headline(),
        text_color,
    )
}

/// 2px focus ring, drawn outside the control so it never clips the label.
pub fn focus_ring(p: &Painter, rect: Rect) {
    p.rect_stroke(
        rect.expand(2.0),
        cr(theme::R_CONTROL + 2),
        Stroke::new(2.0, theme::ACCENT),
        egui::StrokeKind::Outside,
    );
}

/// Small count or status pill.
pub fn chip(p: &Painter, rect: Rect, text: &str, color: Color32) {
    p.rect_filled(rect, cr(theme::R_PILL), color.gamma_multiply(0.14));
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme::label(),
        color,
    );
}

/// 1px vertical hairline at `x`, from `y0` to `y1`.
pub fn vdivider(p: &Painter, x: f32, y0: f32, y1: f32) {
    p.line_segment(
        [Pos2::new(x, y0), Pos2::new(x, y1)],
        Stroke::new(1.0, theme::SEP),
    );
}
