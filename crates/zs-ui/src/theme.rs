//! Design tokens.
//!
//! One place to change how the whole app looks. The rules the rest of the UI is
//! written against:
//!
//! * **Eight type sizes, no others.** A ninth size is a component that needs
//!   redesigning, not a new number.
//! * **Spacing is a 4pt grid**, named `S1`..`S7`. No literal pixel gaps in
//!   layout code.
//! * **Accent is only for things you can act on.** Progress, success and danger
//!   have their own colours. If a non-interactive element is accent-coloured,
//!   that is a bug.
//! * **Depth is a surface step plus a hairline**, not a shadow. macOS is flat.
//!
//! Values come from `docs/UI-SYSTEM.md`; the reasoning lives there.

use egui::{Color32, FontFamily, FontId, Stroke};

// ---------------------------------------------------------------------------
// Spacing — 4pt base, 8pt rhythm
// ---------------------------------------------------------------------------

/// 4 — between the two lines of a stacked label.
pub const S1: f32 = 4.0;
/// 8 — icon to label, row vertical padding.
pub const S2: f32 = 8.0;
/// 12 — row horizontal padding, section header to content.
pub const S3: f32 = 12.0;
/// 16 — between panels.
pub const S4: f32 = 16.0;
/// 24 — panel padding.
pub const S5: f32 = 24.0;
/// 32 — between major regions.
pub const S6: f32 = 32.0;
/// 48 — empty-state breathing room.
pub const S7: f32 = 48.0;

// ---------------------------------------------------------------------------
// Radii
// ---------------------------------------------------------------------------

/// Anything you press.
pub const R_CONTROL: u8 = 6;
/// Anything that contains rows.
pub const R_CARD: u8 = 10;
/// Floating surfaces only.
pub const R_PANEL: u8 = 14;
/// Pills and avatars.
pub const R_PILL: u8 = u8::MAX;

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

/// Window ground.
pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0D, 0x10);
/// Sidebar and bottom panel.
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x15, 0x19);
/// Row hover, card.
pub const RAISED: Color32 = Color32::from_rgb(0x19, 0x1D, 0x23);
/// Button rest, field.
pub const CONTROL: Color32 = Color32::from_rgb(0x20, 0x25, 0x2C);

/// 1px hairline.
pub const SEP: Color32 = Color32::from_rgb(0x26, 0x2C, 0x35);
/// Panel edges, where a hairline has to be seen.
pub const SEP_STRONG: Color32 = Color32::from_rgb(0x33, 0x3B, 0x47);

/// Primary text.
pub const LABEL: Color32 = Color32::from_rgb(0xE8, 0xEB, 0xF0);
/// Secondary text.
pub const LABEL2: Color32 = Color32::from_rgb(0x98, 0xA2, 0xB3);
/// Tertiary text, placeholders.
pub const LABEL3: Color32 = Color32::from_rgb(0x62, 0x6C, 0x7C);

/// Interactive accent: buttons, focus, selection.
pub const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x8C, 0xFF);
/// Accent while held.
pub const ACCENT_PRESS: Color32 = Color32::from_rgb(0x6A, 0x79, 0xE8);
/// Text on an accent fill.
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x12);
/// Data in flight. Never interactive.
pub const PROGRESS: Color32 = Color32::from_rgb(0x3B, 0xE8, 0xB0);
/// Failures.
pub const DANGER: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);
/// Warnings, non-fatal problems.
pub const WARN: Color32 = Color32::from_rgb(0xE8, 0xB3, 0x4B);

/// Accent at low alpha, for selection backgrounds.
#[must_use]
pub fn accent_wash() -> Color32 {
    ACCENT.gamma_multiply(0.16)
}

/// Hairline stroke.
#[must_use]
pub fn hairline() -> Stroke {
    Stroke::new(1.0, SEP)
}

// ---------------------------------------------------------------------------
// Type scale
// ---------------------------------------------------------------------------

fn family(name: &str, size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(name.into()))
}

/// App name in the titlebar. 22 / semibold.
#[must_use]
pub fn title1() -> FontId {
    family("InterSemiBold", 22.0)
}

/// Panel titles. 17 / semibold.
#[must_use]
pub fn title2() -> FontId {
    family("InterSemiBold", 17.0)
}

/// Row titles, button labels. 13 / semibold.
#[must_use]
pub fn headline() -> FontId {
    family("InterSemiBold", 13.0)
}

/// Primary text. 13 / regular.
#[must_use]
pub fn body() -> FontId {
    FontId::proportional(13.0)
}

/// Secondary text, hints. 12 / regular.
#[must_use]
pub fn callout() -> FontId {
    FontId::proportional(12.0)
}

/// Metadata, addresses. 11 / regular.
#[must_use]
pub fn caption() -> FontId {
    FontId::proportional(11.0)
}

/// Section tags. 10 / medium, uppercase at the call site.
#[must_use]
pub fn label() -> FontId {
    family("InterMedium", 10.0)
}

/// Numbers: byte counters, speeds, hashes.
#[must_use]
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// The custom bold family, registered in [`install_fonts`].
#[must_use]
pub fn bold(size: f32) -> FontId {
    family("InterSemiBold", size)
}

/// Medium weight family.
#[must_use]
pub fn medium(size: f32) -> FontId {
    family("InterMedium", size)
}

/// Regular proportional.
#[must_use]
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Register the embedded typefaces.
///
/// Two files do most of the work: Inter for UI, `JetBrains` Mono for numbers.
/// Tabular-ish figures in the mono face keep speed readouts from twitching as
/// digits change, which is the single most noticeable polish detail in a
/// progress-heavy app.
pub fn install_fonts(ctx: &egui::Context) {
    use egui::FontData;
    use std::sync::Arc;

    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "inter-regular".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-medium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Medium.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-semibold".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "jb-mono".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMono-Regular.ttf"
        ))),
    );

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "inter-regular".to_owned());

    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, "jb-mono".to_owned());

    fonts.families.insert(
        FontFamily::Name("InterMedium".into()),
        vec!["inter-medium".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("InterSemiBold".into()),
        vec!["inter-semibold".to_owned()],
    );

    ctx.set_fonts(fonts);
}

/// Apply spacing and widget styling.
pub fn install_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        // Named styles, so anything that asks egui for a size gets the scale
        // rather than egui's defaults.
        style.text_styles = [
            (egui::TextStyle::Heading, title2()),
            (egui::TextStyle::Body, body()),
            (egui::TextStyle::Button, body()),
            (egui::TextStyle::Small, caption()),
            (egui::TextStyle::Monospace, mono(12.0)),
        ]
        .into();

        style.spacing.item_spacing = egui::vec2(S2, S2);
        style.spacing.button_padding = egui::vec2(S3, S2);
        style.spacing.interact_size.y = 28.0;

        style.visuals.override_text_color = Some(LABEL);
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = PANEL;
        style.visuals.window_stroke = Stroke::new(1.0, SEP);
        style.visuals.widgets.noninteractive.bg_fill = PANEL;
        style.visuals.widgets.inactive.bg_fill = CONTROL;
        style.visuals.widgets.hovered.bg_fill = RAISED;
        style.visuals.widgets.active.bg_fill = RAISED;
        style.visuals.selection.bg_fill = accent_wash();
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    });
}
