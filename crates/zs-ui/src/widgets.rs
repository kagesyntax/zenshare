//! Painted primitives.
//!
//! Everything here draws straight onto the canvas. No widget library is
//! involved, which is why the app does not look like one.

use crate::theme;
use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, vec2};

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
    p.rect_stroke(rect, cr(radius), Stroke::new(1.0, border), StrokeKind::Inside);
}

/// A pill / chip.
pub fn pill(p: &Painter, rect: Rect, fill: Color32) {
    p.rect_filled(rect, cr(theme::RADIUS_PILL), fill);
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
    p.rect_filled(rect, cr((rect.height() / 2.0) as u8), theme::SURFACE_HI);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let w = (rect.width() * f).max(rect.height());
        let filled = Rect::from_min_size(rect.min, vec2(w, rect.height()));
        p.rect_filled(filled, cr((rect.height() / 2.0) as u8), color);
    }
}

/// The little identity disc shown for each peer.
pub fn avatar(p: &Painter, center: Pos2, radius: f32, seed: u32, initials: &str) {
    // Two accents, chosen deterministically so a device keeps its colour.
    let base = if seed % 2 == 0 { theme::ACCENT } else { theme::ACCENT_ALT };
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

/// Draw an icon centred on `c` with a nominal size of `s`.
pub fn icon(p: &Painter, c: Pos2, s: f32, which: Icon, color: Color32) {
    let w = Stroke::new(1.7, color);
    let h = s * 0.5;
    match which {
        Icon::Send => {
            p.line_segment([Pos2::new(c.x - h, c.y + h * 0.35), Pos2::new(c.x + h, c.y - h * 0.55)], w);
            p.line_segment([Pos2::new(c.x + h, c.y - h * 0.55), Pos2::new(c.x + h * 0.15, c.y + h * 0.55)], w);
            p.line_segment([Pos2::new(c.x + h * 0.15, c.y + h * 0.55), Pos2::new(c.x - h * 0.15, c.y - h * 0.05)], w);
            p.line_segment([Pos2::new(c.x - h * 0.15, c.y - h * 0.05), Pos2::new(c.x + h, c.y - h * 0.55)], w);
        }
        Icon::Receive => {
            p.line_segment([Pos2::new(c.x, c.y - h), Pos2::new(c.x, c.y + h * 0.3)], w);
            p.line_segment([Pos2::new(c.x - h * 0.45, c.y - h * 0.15), Pos2::new(c.x, c.y + h * 0.3)], w);
            p.line_segment([Pos2::new(c.x + h * 0.45, c.y - h * 0.15), Pos2::new(c.x, c.y + h * 0.3)], w);
            p.line_segment([Pos2::new(c.x - h * 0.7, c.y + h * 0.7), Pos2::new(c.x + h * 0.7, c.y + h * 0.7)], w);
        }
        Icon::History => {
            p.circle_stroke(c, h * 0.8, w);
            p.line_segment([c, Pos2::new(c.x, c.y - h * 0.45)], w);
            p.line_segment([c, Pos2::new(c.x + h * 0.4, c.y + h * 0.15)], w);
        }
        Icon::Settings => {
            p.circle_stroke(c, h * 0.42, w);
            for i in 0..8 {
                let a = std::f32::consts::TAU * (i as f32) / 8.0;
                let (sn, cs) = a.sin_cos();
                p.line_segment(
                    [
                        Pos2::new(c.x + cs * h * 0.62, c.y + sn * h * 0.62),
                        Pos2::new(c.x + cs * h * 0.92, c.y + sn * h * 0.92),
                    ],
                    w,
                );
            }
        }
        Icon::Plus => {
            p.line_segment([Pos2::new(c.x - h * 0.7, c.y), Pos2::new(c.x + h * 0.7, c.y)], w);
            p.line_segment([Pos2::new(c.x, c.y - h * 0.7), Pos2::new(c.x, c.y + h * 0.7)], w);
        }
        Icon::Folder => {
            let r = Rect::from_center_size(Pos2::new(c.x, c.y + h * 0.08), Vec2::new(h * 1.6, h * 1.1));
            p.rect_stroke(r, cr(2), w, StrokeKind::Inside);
            p.line_segment(
                [Pos2::new(r.min.x, r.min.y), Pos2::new(r.min.x + h * 0.55, r.min.y)],
                w,
            );
        }
        Icon::Check => {
            p.line_segment([Pos2::new(c.x - h * 0.55, c.y), Pos2::new(c.x - h * 0.1, c.y + h * 0.45)], w);
            p.line_segment([Pos2::new(c.x - h * 0.1, c.y + h * 0.45), Pos2::new(c.x + h * 0.6, c.y - h * 0.45)], w);
        }
        Icon::Close => {
            p.line_segment([Pos2::new(c.x - h * 0.5, c.y - h * 0.5), Pos2::new(c.x + h * 0.5, c.y + h * 0.5)], w);
            p.line_segment([Pos2::new(c.x + h * 0.5, c.y - h * 0.5), Pos2::new(c.x - h * 0.5, c.y + h * 0.5)], w);
        }
        Icon::Chevron => {
            p.line_segment([Pos2::new(c.x - h * 0.25, c.y - h * 0.45), Pos2::new(c.x + h * 0.25, c.y)], w);
            p.line_segment([Pos2::new(c.x + h * 0.25, c.y), Pos2::new(c.x - h * 0.25, c.y + h * 0.45)], w);
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
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
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
