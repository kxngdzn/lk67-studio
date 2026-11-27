//! Visual language: deep navy surfaces, teal accent, amber warnings.

use egui::{Color32, Stroke};

pub const BG: Color32 = Color32::from_rgb(16, 20, 28);
pub const PANEL: Color32 = Color32::from_rgb(23, 29, 41);
pub const PANEL_2: Color32 = Color32::from_rgb(29, 36, 50);
pub const PANEL_3: Color32 = Color32::from_rgb(38, 47, 64);
pub const STROKE: Color32 = Color32::from_rgb(45, 55, 73);
pub const TEXT: Color32 = Color32::from_rgb(224, 231, 242);
pub const MUTED: Color32 = Color32::from_rgb(139, 152, 175);
pub const ACCENT: Color32 = Color32::from_rgb(52, 206, 170);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(26, 104, 88);
pub const WARN: Color32 = Color32::from_rgb(255, 183, 92);
pub const DANGER: Color32 = Color32::from_rgb(238, 104, 110);
pub const OK: Color32 = Color32::from_rgb(120, 210, 140);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(12, 16, 23);
    visuals.faint_bg_color = Color32::from_rgb(25, 31, 44);
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, STROKE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    visuals.widgets.inactive.bg_fill = PANEL_2;
    visuals.widgets.inactive.weak_bg_fill = PANEL_2;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, STROKE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = PANEL_3;
    visuals.widgets.hovered.weak_bg_fill = PANEL_3;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.active.weak_bg_fill = ACCENT_DIM;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.window_corner_radius = 10.into();
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.slider_width = 150.0;
        style.spacing.indent = 14.0;
        style.visuals.widgets.inactive.corner_radius = 7.into();
        style.visuals.widgets.hovered.corner_radius = 7.into();
        style.visuals.widgets.active.corner_radius = 7.into();
    });
}

const CARD_PAD_X: i8 = 14;
const CARD_PAD_Y: i8 = 12;

/// Rounded panel used for every section. It always spans the available width,
/// so cards on the same page line up instead of ending where their content does.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_fill(ui, title, 0.0, add)
}

/// A card that is at least `min_height` tall. Pages use it for the shorter of
/// two side-by-side cards so the row has no hole in it.
pub fn card_fill<R>(
    ui: &mut egui::Ui,
    title: &str,
    min_height: f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let width = ui.available_width();
    egui::Frame::NONE
        .fill(PANEL)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(10.0)
        .inner_margin(egui::Margin {
            left: CARD_PAD_X,
            right: CARD_PAD_X,
            top: CARD_PAD_Y,
            bottom: CARD_PAD_Y,
        })
        .show(ui, |ui| {
            ui.set_min_width((width - f32::from(CARD_PAD_X) * 2.0).max(0.0));
            if min_height > 0.0 {
                ui.set_min_height((min_height - f32::from(CARD_PAD_Y) * 2.0).max(0.0));
            }
            if !title.is_empty() {
                ui.label(
                    egui::RichText::new(title.to_ascii_uppercase())
                        .size(11.0)
                        .strong()
                        .color(MUTED),
                );
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

pub fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(egui::RichText::new(text.into()).size(11.0).color(MUTED)).wrap());
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(
        egui::RichText::new(format!(" {text} "))
            .size(10.5)
            .strong()
            .color(Color32::from_rgb(12, 16, 22))
            .background_color(color),
    );
}

pub fn blend(base: Color32, overlay: [u8; 3], amount: f32) -> Color32 {
    let t = amount.clamp(0.0, 1.0);
    Color32::from_rgb(
        (base.r() as f32 * (1.0 - t) + overlay[0] as f32 * t) as u8,
        (base.g() as f32 * (1.0 - t) + overlay[1] as f32 * t) as u8,
        (base.b() as f32 * (1.0 - t) + overlay[2] as f32 * t) as u8,
    )
}

pub fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}
