//! Design tokens. One place to change how the whole app looks.
//!
//! The palette is deliberately narrow: a near-black ground, three surface
//! levels, one text ramp, and exactly two accents (send / receive). Everything
//! else is derived. Narrow palettes are what make an interface feel designed
//! rather than assembled.

use egui::{Color32, FontFamily, FontId, Stroke};

/// App background, behind everything.
pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x0F);
/// Cards and panels.
pub const SURFACE: Color32 = Color32::from_rgb(0x12, 0x15, 0x1A);
/// Raised / hovered surfaces.
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x18, 0x1C, 0x23);
/// Hairlines.
pub const BORDER: Color32 = Color32::from_rgb(0x24, 0x2A, 0x33);

/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(0xE9, 0xEC, 0xF0);
/// Secondary text.
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x93, 0x9C, 0xAB);
/// Tertiary text, hints.
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x5C, 0x66, 0x75);

/// Send accent — mint.
pub const ACCENT: Color32 = Color32::from_rgb(0x3B, 0xE8, 0xB0);
/// Receive accent — periwinkle.
pub const ACCENT_ALT: Color32 = Color32::from_rgb(0x7C, 0x8C, 0xFF);
/// Failure.
pub const DANGER: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);

/// Corner radius for cards.
pub const RADIUS: u8 = 14;
/// Corner radius for pills.
pub const RADIUS_PILL: u8 = 255;

/// Hairline stroke.
#[must_use]
pub fn hairline() -> Stroke {
    Stroke::new(1.0, BORDER)
}

/// The custom bold family, registered in [`crate::install_fonts`].
#[must_use]
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("InterSemiBold".into()))
}

/// Medium weight family.
#[must_use]
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("InterMedium".into()))
}

/// Regular proportional.
#[must_use]
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Monospace, for byte counters and hashes.
#[must_use]
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Register the embedded typefaces.
///
/// Two files do most of the work: Inter for UI, JetBrains Mono for numbers.
/// Tabular-ish figures in the mono face keep speed readouts from twitching as
/// digits change, which is the single most noticeable polish detail in a
/// progress-heavy app.
pub fn install_fonts(ctx: &egui::Context) {
    use egui::FontData;
    use std::sync::Arc;

    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "inter-regular".to_owned(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf"))),
    );
    fonts.font_data.insert(
        "inter-medium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-Medium.ttf"))),
    );
    fonts.font_data.insert(
        "inter-semibold".to_owned(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"))),
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

    fonts
        .families
        .insert(FontFamily::Name("InterMedium".into()), vec!["inter-medium".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Name("InterSemiBold".into()), vec!["inter-semibold".to_owned()]);

    ctx.set_fonts(fonts);
}

/// Apply spacing and widget styling.
pub fn install_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = SURFACE;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = SURFACE;
        style.visuals.widgets.noninteractive.bg_fill = SURFACE;
        style.visuals.widgets.inactive.bg_fill = SURFACE_HI;
        style.visuals.widgets.hovered.bg_fill = SURFACE_HI;
        style.visuals.widgets.active.bg_fill = SURFACE_HI;
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.28);
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    });
}
