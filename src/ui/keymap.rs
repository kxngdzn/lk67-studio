//! The interactive 65% keymap canvas.
//!
//! Every key shows the physical legend, the assignment for the active layer, a live RGB
//! strip from the lighting engine, press highlights and a marker when it is remapped.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke};

use crate::model::layout::{KeyDef, Layout};
use crate::model::macro_def::MacroDef;
use crate::model::{Action, Layer};
use crate::ui::theme::{self, blend};
use crate::ui::{Tool, widgets};

pub struct KeymapView<'a> {
    pub layout: &'a Layout,
    pub layer: Layer,
    pub normal: &'a [Action],
    pub func: &'a [Action],
    pub macros: &'a [MacroDef],
    pub colors: &'a [[u8; 3]],
    pub show_rgb: bool,
    pub pressed: &'a [bool],
    pub selected: Option<usize>,
    pub heat: Option<&'a [u32]>,
    pub tool: Tool,
}

#[derive(Default)]
pub struct KeymapResponse {
    pub clicked: Option<usize>,
    pub right_clicked: Option<usize>,
    pub painted: Vec<usize>,
    pub hovered: Option<usize>,
}

pub fn show(ui: &mut egui::Ui, view: &KeymapView<'_>) -> KeymapResponse {
    let mut result = KeymapResponse::default();
    let layout = view.layout;
    let width = ui.available_width().min(1120.0).max(320.0);
    let unit = width / layout.width;
    let height = (layout.height + 0.12) * unit;
    let (canvas, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter_at(canvas);

    let max_heat = view
        .heat
        .map(|h| h.iter().copied().max().unwrap_or(1))
        .unwrap_or(1) as f32;
    let gap = (unit * 0.055).clamp(1.5, 5.0);
    let hot = ui.input(|i| i.pointer.primary_down());

    for key in &layout.keys {
        let rect = key_rect(canvas, key, unit, gap);
        let action = if view.layer == Layer::Normal {
            view.normal.get(key.index)
        } else {
            view.func.get(key.index)
        };
        let action = action.cloned().unwrap_or_default();
        let base_action = view.normal.get(key.index).cloned().unwrap_or_default();
        let remapped = action.is_remapped(key.vk);
        let pressed = view.pressed.get(key.index).copied().unwrap_or(false);
        let led = view.colors.get(key.index).copied().unwrap_or([0, 0, 0]);
        let selected = view.selected == Some(key.index);

        let mut fill = if view.show_rgb {
            blend(theme::PANEL_2, led, 0.30)
        } else {
            theme::PANEL_2
        };
        if let Some(heat) = view.heat {
            let count = heat.get(key.index).copied().unwrap_or(0);
            if count > 0 {
                let t = (count as f32 / max_heat.max(1.0)).powf(0.6) * 0.55;
                fill = blend(fill, [255, 170, 60], t);
            }
        }
        if pressed {
            fill = blend(fill, [255, 255, 255], 0.22);
            painter.rect_filled(
                rect.expand(2.0),
                7.0,
                Color32::from_rgba_unmultiplied(led[0], led[1], led[2], 70),
            );
        }
        painter.rect_filled(rect, 5.0, fill);
        painter.rect_stroke(
            rect,
            5.0,
            Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected {
                    theme::ACCENT
                } else if pressed {
                    theme::ACCENT_DIM
                } else {
                    theme::STROKE
                },
            ),
            egui::StrokeKind::Inside,
        );

        // Live LED strip along the bottom edge.
        if view.show_rgb {
            let strip = Rect::from_min_max(
                egui::pos2(rect.left() + 5.0, rect.bottom() - 4.0),
                egui::pos2(rect.right() - 5.0, rect.bottom() - 2.0),
            );
            painter.rect_filled(strip, 1.0, theme::rgb(led));
        }

        let base_label = layout.label(key.index);
        let mapped_label = action.short(view.macros);
        let center_label = if remapped {
            widgets::truncate(&mapped_label, if key.w > 1.4 { 12 } else { 6 })
        } else {
            widgets::truncate(&base_label, if key.w > 1.4 { 14 } else { 7 })
        };
        let font_size = (unit * 0.24).clamp(8.0, 15.0);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            center_label,
            FontId::proportional(font_size),
            theme::TEXT,
        );

        // Base legend + FN hint.
        if remapped {
            let small = (unit * 0.155).clamp(6.5, 9.5);
            painter.text(
                egui::pos2(rect.left() + 4.0, rect.top() + 3.0),
                Align2::LEFT_TOP,
                widgets::truncate(&base_label, 4),
                FontId::proportional(small),
                theme::MUTED,
            );
        }
        if view.layer == Layer::Normal && view.tool == Tool::Select {
            let fn_action = view.func.get(key.index).cloned().unwrap_or_default();
            if fn_action != base_action {
                let small = (unit * 0.14).clamp(6.0, 9.0);
                painter.text(
                    egui::pos2(rect.right() - 4.0, rect.bottom() - 6.0),
                    Align2::RIGHT_BOTTOM,
                    widgets::truncate(&fn_action.short(view.macros), 4),
                    FontId::proportional(small),
                    theme::ACCENT.gamma_multiply(0.75),
                );
            }
        }

        if matches!(action, Action::Macro { .. }) {
            painter.circle_filled(
                egui::pos2(rect.right() - 6.0, rect.top() + 6.0),
                2.2,
                theme::ACCENT,
            );
        }

        let response = ui.interact(
            rect,
            ui.scope_id().with(("lk67-key", key.index)),
            Sense::click(),
        );
        if response.hovered() {
            result.hovered = Some(key.index);
        }
        let tooltip = format!(
            "{} ({})\n{}\nBase: {}",
            layout.label(key.index),
            key.code,
            action.summary(view.macros),
            base_action.summary(view.macros)
        );
        let response = response.on_hover_text(tooltip);

        if view.tool == Tool::Paint {
            if response.clicked() || (hot && response.hovered()) {
                result.painted.push(key.index);
            }
            // Keep clicks selectable while painting, so a key can be picked and
            // remapped without switching back to the Select tool.
            if response.clicked() {
                result.clicked = Some(key.index);
            }
        } else if response.clicked() {
            result.clicked = Some(key.index);
        }
        if response.secondary_clicked() {
            result.right_clicked = Some(key.index);
        }
    }

    result
}

fn key_rect(canvas: Rect, key: &KeyDef, unit: f32, gap: f32) -> Rect {
    Rect::from_min_size(
        egui::pos2(
            canvas.left() + key.x * unit + gap * 0.5,
            canvas.top() + key.y * unit + gap * 0.5,
        ),
        egui::vec2(key.w * unit - gap, key.h * unit - gap),
    )
}
