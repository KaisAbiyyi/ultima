//! Offline-only design tokens translated from minimalist_codex_wrapper_ui.html.
//! No Tailwind runtime, remote fonts, browser process or permanent animation.
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    RichText, Stroke,
};
use std::sync::Arc;

pub const CANVAS: Color32 = Color32::from_rgb(9, 9, 11);
pub const SURFACE: Color32 = Color32::from_rgb(14, 14, 17);
pub const RAISED: Color32 = Color32::from_rgb(18, 18, 21);
pub const HOVER: Color32 = Color32::from_rgb(31, 31, 35);
pub const BORDER: Color32 = Color32::from_rgb(39, 39, 43);
pub const TEXT: Color32 = Color32::from_rgb(238, 238, 240);
pub const SECONDARY: Color32 = Color32::from_rgb(158, 158, 166);
pub const MUTED: Color32 = Color32::from_rgb(105, 105, 114);
pub const GREEN: Color32 = Color32::from_rgb(104, 211, 159);

/// Read already-installed Windows font files at runtime, never vendor fonts or
/// fetch remote assets. egui default bundled fonts remain the safe fallback.
/// Segoe UI and Cascadia Mono approximate the reference's Inter/JetBrains.
pub fn use_system_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let root = std::path::Path::new(r"C:\Windows\Fonts");
    let prop = root.join("segoeui.ttf");
    let mono = root.join("CascadiaMono.ttf");
    if let Ok(bytes) = std::fs::read(prop) {
        fonts.font_data.insert(
            "ultima-proportional".to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ultima-proportional".to_owned());
    }
    if let Ok(bytes) = std::fs::read(mono) {
        fonts.font_data.insert(
            "ultima-mono".to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "ultima-mono".to_owned());
    }
    ctx.set_fonts(fonts);
}

pub fn configure(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CANVAS;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = CANVAS;
    visuals.faint_bg_color = SURFACE;
    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_gray(67));
    visuals.widgets.active.bg_fill = HOVER;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_gray(88));
    visuals.selection.bg_fill = Color32::from_rgb(59, 59, 66);
    visuals.selection.stroke = Stroke::new(1.0, TEXT);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(9);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(9);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(9);
    visuals.widgets.active.corner_radius = CornerRadius::same(9);
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(5.0, 5.0);
    style.spacing.button_padding = egui::vec2(7.0, 4.0);
    style.spacing.window_margin = Margin::same(12);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, FontId::monospace(11.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    style.animation_time = 0.0;
    ctx.set_style(style);
}

pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(11.0).color(MUTED)
}

pub fn body(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(12.5).color(SECONDARY)
}

pub fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text.into())
        .font(FontId::monospace(10.5))
        .color(SECONDARY)
}

pub fn panel() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(15))
        .inner_margin(Margin::same(10))
}

pub fn compose() -> Frame {
    Frame::new()
        .fill(RAISED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(13, 11))
}
