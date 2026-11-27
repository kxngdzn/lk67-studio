//! Small reusable widgets shared by the pages.

use egui::{Color32, RichText, Sense, Stroke};

use crate::model::keycode::{KEY_TABLE, KeyCode};
use crate::model::lighting::PALETTE_SLOTS;
use crate::rgb::music::BANDS;
use crate::rgb::screen::ScreenSnapshot;
use crate::ui::theme;

/// Dropdown over every key code, with a filter row for fast picking.
pub fn key_picker(ui: &mut egui::Ui, id: &str, value: &mut KeyCode) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.name())
        .width(170.0)
        .show_ui(ui, |ui| {
            ui.set_min_width(240.0);
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (vk, name) in KEY_TABLE {
                        if ui.selectable_label(value.vk() == *vk, *name).clicked() {
                            *value = KeyCode(*vk);
                            changed = true;
                            ui.close();
                        }
                    }
                });
        });
    changed
}

pub fn mods_row(ui: &mut egui::Ui, mods: &mut crate::model::action::Mods) -> bool {
    let mut changed = false;
    changed |= ui.checkbox(&mut mods.ctrl, "Ctrl").changed();
    changed |= ui.checkbox(&mut mods.shift, "Shift").changed();
    changed |= ui.checkbox(&mut mods.alt, "Alt").changed();
    changed |= ui.checkbox(&mut mods.win, "Win").changed();
    changed
}

/// Clickable palette swatches; returns the slot the user clicked.
pub fn palette_swatches(
    ui: &mut egui::Ui,
    colors: &[[u8; 3]; PALETTE_SLOTS],
    selected: usize,
) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for (index, color) in colors.iter().enumerate() {
            let size = egui::vec2(24.0, 24.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let painter = ui.painter();
            painter.rect_filled(rect.shrink(1.0), 5.0, theme::rgb(*color));
            if index == selected {
                painter.rect_stroke(
                    rect,
                    5.0,
                    Stroke::new(2.0, theme::ACCENT),
                    egui::StrokeKind::Inside,
                );
            } else if response.hovered() {
                painter.rect_stroke(
                    rect,
                    5.0,
                    Stroke::new(1.0, theme::MUTED),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                clicked = Some(index);
            }
            let response = response.on_hover_text(format!("Colour slot {}", index + 1));
            let _ = response;
        }
    });
    clicked
}

/// Editable 14-colour palette (each slot opens a colour picker).
pub fn palette_editor(ui: &mut egui::Ui, colors: &mut [[u8; 3]; PALETTE_SLOTS]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (index, color) in colors.iter_mut().enumerate() {
            let response = ui.color_edit_button_srgb(color);
            if response.changed() {
                changed = true;
            }
            response.on_hover_text(format!("Palette slot {}", index + 1));
        }
    });
    changed
}

pub fn slider_percent(ui: &mut egui::Ui, label: &str, value: &mut u8) -> bool {
    let mut v = *value as f32;
    let changed = ui
        .add(
            egui::Slider::new(&mut v, 0.0..=100.0)
                .text(label)
                .suffix("%"),
        )
        .changed();
    if changed {
        *value = v.round().clamp(0.0, 100.0) as u8;
    }
    changed
}

/// 16-band spectrum meter with a level line.
pub fn spectrum_meter(ui: &mut egui::Ui, bands: &[f32; BANDS], level: f32, beat: f32) {
    let width = ui.available_width().max(120.0);
    let height = 90.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, Color32::from_rgb(13, 17, 24));
    let gap = 3.0;
    let bar_width = ((rect.width() - gap * (BANDS as f32 + 1.0)) / BANDS as f32).max(2.0);
    for (index, value) in bands.iter().enumerate() {
        let x = rect.left() + gap + index as f32 * (bar_width + gap);
        let h = (value.clamp(0.0, 1.2) * (rect.height() - 12.0)).max(1.0);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x, rect.bottom() - 6.0 - h),
            egui::pos2(x + bar_width, rect.bottom() - 6.0),
        );
        let hue = index as f32 / BANDS as f32;
        let color = crate::rgb::effects::hsv(hue, 0.85, 0.95);
        painter.rect_filled(bar, 2.0, theme::rgb(color));
    }
    let level_y = rect.bottom() - 6.0 - level.clamp(0.0, 1.0) * (rect.height() - 12.0);
    painter.line_segment(
        [
            egui::pos2(rect.left() + 4.0, level_y),
            egui::pos2(rect.right() - 4.0, level_y),
        ],
        Stroke::new(1.0, theme::WARN),
    );
    if beat > 0.05 {
        painter.circle_filled(
            egui::pos2(rect.right() - 12.0, rect.top() + 12.0),
            4.0,
            theme::WARN.gamma_multiply(beat.clamp(0.0, 1.0)),
        );
    }
}

/// Thumbnail of the screen-follow sampling grid.
pub fn screen_preview(ui: &mut egui::Ui, snapshot: &ScreenSnapshot) {
    let width = ui.available_width().max(100.0);
    let height = 100.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, Color32::from_rgb(10, 14, 20));
    if snapshot.grid.is_empty() || snapshot.gw == 0 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "no screen data",
            egui::FontId::proportional(12.0),
            theme::MUTED,
        );
        return;
    }
    let gw = snapshot.gw;
    let gh = snapshot.gh;
    let cell_w = rect.width() / gw as f32;
    let cell_h = rect.height() / gh as f32;
    for y in 0..gh {
        for x in 0..gw {
            let color = snapshot.grid.get(y * gw + x).copied().unwrap_or([0, 0, 0]);
            let cell = egui::Rect::from_min_size(
                egui::pos2(
                    rect.left() + x as f32 * cell_w,
                    rect.top() + y as f32 * cell_h,
                ),
                egui::vec2(cell_w - 1.0, cell_h - 1.0),
            );
            painter.rect_filled(cell, 1.0, theme::rgb(color));
        }
    }
}

pub fn kv(ui: &mut egui::Ui, key: impl Into<String>, value: impl Into<String>) {
    ui.label(RichText::new(key.into()).size(11.5).color(theme::MUTED));
    ui.label(RichText::new(value.into()).size(11.5));
    ui.end_row();
}

pub fn status_text(ui: &mut egui::Ui, text: impl AsRef<str>, ok: bool) {
    let color = if ok { theme::OK } else { theme::DANGER };
    ui.label(RichText::new(text.as_ref()).size(11.5).color(color));
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}
