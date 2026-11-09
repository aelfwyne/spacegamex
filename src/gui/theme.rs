//! Colors, fonts and small reusable widgets for the dark "cockpit" theme.

use crate::sim::{Level, Rgb};
use eframe::egui::{
    self, Color32, FontId, ProgressBar, RichText, Stroke, TextStyle, Theme, ThemePreference, Ui,
};

pub const BG_DEEP: Color32 = Color32::from_rgb(5, 7, 13);
pub const BG_PANEL: Color32 = Color32::from_rgb(13, 17, 27);
pub const BG_CARD: Color32 = Color32::from_rgb(20, 26, 40);
pub const ACCENT: Color32 = Color32::from_rgb(90, 200, 235);
pub const DIM: Color32 = Color32::from_rgb(120, 130, 150);
pub const TEXT: Color32 = Color32::from_rgb(214, 222, 235);
pub const GOOD: Color32 = Color32::from_rgb(90, 215, 130);
pub const WARN: Color32 = Color32::from_rgb(240, 200, 70);
pub const BAD: Color32 = Color32::from_rgb(240, 95, 85);
pub const CRIT: Color32 = Color32::from_rgb(255, 60, 60);
pub const CIV: Color32 = Color32::from_rgb(220, 120, 240);
pub const KEY: Color32 = Color32::from_rgb(250, 210, 90);

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(ThemePreference::Dark);
    ctx.style_mut_of(Theme::Dark, |s| {
        let v = &mut s.visuals;
        v.panel_fill = BG_PANEL;
        v.window_fill = BG_CARD;
        v.extreme_bg_color = BG_DEEP;
        v.faint_bg_color = Color32::from_rgb(18, 23, 35);
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = Color32::from_rgb(30, 90, 120);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.override_text_color = Some(TEXT);
        s.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(14.0));
        s.spacing.item_spacing = egui::vec2(8.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

pub fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

/// Red / amber / green by how full something is.
pub fn frac_color(frac: f64) -> Color32 {
    if frac < 0.2 {
        BAD
    } else if frac < 0.5 {
        WARN
    } else {
        GOOD
    }
}

pub fn level_style(level: Option<Level>) -> (&'static str, Color32) {
    match level {
        Some(Level::Info) => ("", Color32::from_rgb(175, 185, 200)),
        Some(Level::Good) => ("+ ", GOOD),
        Some(Level::Warn) => ("! ", WARN),
        Some(Level::Bad) => ("# ", Color32::from_rgb(255, 130, 110)),
        Some(Level::Critical) => ("X ", CRIT),
        None => ("> ", ACCENT),
    }
}

/// A colored fill bar with text on it.
pub fn bar(ui: &mut Ui, frac: f64, text: impl Into<String>, width: f32) -> egui::Response {
    let f = frac.clamp(0.0, 1.0) as f32;
    ui.add(
        ProgressBar::new(f)
            .desired_width(width)
            .desired_height(18.0)
            .fill(frac_color(frac).gamma_multiply(0.75))
            .text(RichText::new(text.into()).color(Color32::WHITE).size(13.0)),
    )
}

/// Bar with an explicit fill color.
pub fn bar_colored(ui: &mut Ui, frac: f64, text: impl Into<String>, width: f32, color: Color32) -> egui::Response {
    ui.add(
        ProgressBar::new(frac.clamp(0.0, 1.0) as f32)
            .desired_width(width)
            .desired_height(18.0)
            .fill(color.gamma_multiply(0.75))
            .text(RichText::new(text.into()).color(Color32::WHITE).size(13.0)),
    )
}

/// `[K]` keyboard hint chip.
pub fn key_chip(key: &str) -> RichText {
    RichText::new(format!("[{key}]")).monospace().color(KEY).size(12.0)
}

/// Button label with its keyboard shortcut.
pub fn with_key(label: &str, key: &str) -> String {
    format!("{label}  [{key}]")
}

pub fn dim(s: impl Into<String>) -> RichText {
    RichText::new(s.into()).color(DIM)
}

pub fn section(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).strong().color(ACCENT).size(14.0));
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, Color32::from_rgb(40, 52, 75)))
        .corner_radius(8.0)
        .inner_margin(10.0)
}
