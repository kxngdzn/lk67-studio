//! The interactive 65% keymap canvas.
//!
//! Every key shows the physical legend, the assignment for the active layer, a live RGB
//! strip from the lighting engine, press highlights and a marker when it is remapped.
//!
//! The board is drawn as a top-down product render: a bevelled case with a front lip,
//! sculpted keycaps, light bleed from the LEDs and a round knurled knob in the top-right.

use std::f32::consts::TAU;

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};

use crate::model::layout::{KeyDef, Layout};
use crate::model::macro_def::MacroDef;
use crate::model::{Action, Layer};
use crate::ui::theme::{self, blend};
use crate::ui::{Tool, widgets};

const CASE_BODY: Color32 = Color32::from_rgb(19, 21, 25);
const CASE_TOP: Color32 = Color32::from_rgb(28, 31, 36);
const CASE_EDGE: Color32 = Color32::from_rgb(62, 68, 78);
const DECK: Color32 = Color32::from_rgb(9, 11, 14);
const CAP_BODY: Color32 = Color32::from_rgb(23, 25, 29);
const CAP_FACE: Color32 = Color32::from_rgb(44, 47, 53);
const LEGEND: Color32 = Color32::from_rgb(232, 236, 242);

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

    // The canvas includes the case around the keys, so the unit is slightly smaller
    // than available_width / layout.width.
    let pad_x = 0.30;
    let pad_top = 0.26;
    let pad_bottom = 0.48;
    let unit = width / (layout.width + pad_x * 2.0);
    let height = (layout.height + pad_top + pad_bottom) * unit;
    let (canvas, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter_at(canvas);

    let max_heat = view
        .heat
        .map(|h| h.iter().copied().max().unwrap_or(1))
        .unwrap_or(1) as f32;
    let gap = (unit * 0.055).clamp(1.5, 4.5);
    let hot = ui.input(|i| i.pointer.primary_down());
    let origin = canvas.min + vec2(pad_x * unit, pad_top * unit);

    let glow = view
        .colors
        .iter()
        .copied()
        .filter(|color| color.iter().map(|c| *c as u32).sum::<u32>() > 24)
        .max_by_key(|color| color.iter().map(|c| *c as u32).sum::<u32>());
    let deck = Rect::from_min_max(
        origin - vec2(gap * 0.5 + unit * 0.02, gap * 0.5 + unit * 0.02),
        origin
            + vec2(
                layout.width * unit + gap * 0.5 + unit * 0.02,
                layout.height * unit + gap * 0.5 + unit * 0.02,
            ),
    );
    draw_case(
        &painter,
        canvas,
        deck,
        unit,
        if view.show_rgb { glow } else { None },
    );

    for key in &layout.keys {
        let rect = key_rect(origin, key, unit, gap);
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

        let response = ui.interact(
            rect,
            ui.scope_id().with(("lk67-key", key.index)),
            Sense::click(),
        );
        let hovered = response.hovered();
        if hovered {
            result.hovered = Some(key.index);
        }

        let mut fill = if view.show_rgb {
            blend(CAP_FACE, led, 0.30)
        } else {
            CAP_FACE
        };
        if let Some(heat) = view.heat {
            let count = heat.get(key.index).copied().unwrap_or(0);
            if count > 0 {
                let t = (count as f32 / max_heat.max(1.0)).powf(0.6) * 0.55;
                fill = blend(fill, [255, 170, 60], t);
            }
        }
        if pressed {
            fill = blend(fill, [255, 255, 255], 0.16);
        }
        if hovered {
            fill = fill.gamma_multiply(1.12);
        }

        if key.code == "Knob" {
            draw_knob(
                &painter,
                rect,
                unit,
                led,
                view.show_rgb,
                pressed,
                hovered,
                selected,
                remapped || matches!(action, Action::Macro { .. }),
            );
        } else {
            draw_keycap(
                &painter,
                rect,
                unit,
                fill,
                led,
                view.show_rgb,
                pressed,
                hovered,
                selected,
            );
            draw_legends(
                &painter,
                rect,
                unit,
                layout,
                key,
                view,
                &action,
                &base_action,
                remapped,
            );
            if matches!(action, Action::Macro { .. }) {
                painter.circle_filled(
                    pos2(rect.right() - unit * 0.13, rect.top() + unit * 0.13),
                    (unit * 0.035).clamp(1.6, 2.6),
                    theme::ACCENT,
                );
            }
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

fn draw_case(painter: &egui::Painter, canvas: Rect, deck: Rect, unit: f32, glow: Option<[u8; 3]>) {
    let case = Rect::from_min_max(
        canvas.min + vec2(unit * 0.05, unit * 0.03),
        canvas.max - vec2(unit * 0.05, unit * 0.02),
    );
    let radius = (unit * 0.20).clamp(5.0, 14.0);

    painter.rect_filled(
        case.translate(vec2(0.0, unit * 0.035)).expand(unit * 0.012),
        radius,
        Color32::from_black_alpha(110),
    );
    if let Some(glow) = glow {
        painter.rect_filled(case.expand(unit * 0.022), radius, rgba(glow, 20));
        painter.rect_filled(case.expand(unit * 0.010), radius, rgba(glow, 14));
    }
    painter.rect_filled(case, radius, CASE_BODY);
    painter.rect_stroke(
        case,
        radius,
        Stroke::new(1.0, Color32::from_rgb(48, 53, 61)),
        egui::StrokeKind::Inside,
    );

    let bevel = Rect::from_min_max(
        case.min + vec2(unit * 0.035, unit * 0.030),
        case.max - vec2(unit * 0.035, unit * 0.030),
    );
    painter.rect_filled(bevel, radius * 0.9, CASE_TOP);
    painter.rect_stroke(
        bevel,
        radius * 0.9,
        Stroke::new(1.0, CASE_EDGE),
        egui::StrokeKind::Inside,
    );

    let deck_radius = (unit * 0.09).clamp(3.0, 6.0);
    painter.rect_filled(deck, deck_radius, DECK);
    painter.rect_stroke(
        deck,
        deck_radius,
        Stroke::new(1.0, Color32::from_rgb(37, 41, 48)),
        egui::StrokeKind::Inside,
    );

    if let Some(glow) = glow {
        // Light spilling out of the front edge of the case.
        let spill = Rect::from_min_max(
            pos2(deck.left() + unit * 0.18, deck.bottom() + unit * 0.012),
            pos2(deck.right() - unit * 0.18, deck.bottom() + unit * 0.030),
        );
        painter.rect_filled(spill, unit * 0.010, rgba(glow, 55));
    }
}

fn draw_keycap(
    painter: &egui::Painter,
    rect: Rect,
    unit: f32,
    fill: Color32,
    led: [u8; 3],
    show_rgb: bool,
    pressed: bool,
    hovered: bool,
    selected: bool,
) {
    let radius = (unit * 0.11).clamp(2.5, 7.0);
    let sink = if pressed { unit * 0.014 } else { 0.0 };
    let cap = rect.translate(vec2(0.0, sink));
    let lit = show_rgb && led.iter().map(|c| *c as u32).sum::<u32>() > 24;

    if lit {
        painter.rect_filled(
            cap.expand(unit * 0.045),
            radius * 1.5,
            rgba(led, if pressed { 58 } else { 30 }),
        );
    }
    // The wall under the cap and its contact shadow.
    painter.rect_filled(
        cap.translate(vec2(0.0, unit * 0.022)),
        radius,
        Color32::from_black_alpha(125),
    );
    painter.rect_filled(cap, radius, CAP_BODY);

    let inset = (unit * 0.045).clamp(1.2, 2.4);
    let face = Rect::from_min_max(
        cap.min + vec2(inset, inset * 0.8),
        cap.max - vec2(inset, inset * 1.1),
    );
    let face_radius = (radius - inset * 0.6).max(2.0);
    painter.rect_filled(face, face_radius, fill);

    // Top light and bottom shade read as a sculpted cap without a texture.
    let top_y = face.top() + 0.7;
    painter.line_segment(
        [
            pos2(face.left() + face_radius * 0.6, top_y),
            pos2(face.right() - face_radius * 0.6, top_y),
        ],
        Stroke::new(
            1.0,
            Color32::from_white_alpha(if pressed { 10 } else { 26 }),
        ),
    );
    let bottom_y = face.bottom() - 0.8;
    painter.line_segment(
        [
            pos2(face.left() + face_radius * 0.6, bottom_y),
            pos2(face.right() - face_radius * 0.6, bottom_y),
        ],
        Stroke::new(1.0, Color32::from_black_alpha(70)),
    );

    if lit {
        let slit = Rect::from_min_max(
            pos2(face.left() + face_radius * 0.7, face.bottom() - 2.0),
            pos2(face.right() - face_radius * 0.7, face.bottom() - 1.0),
        );
        painter.rect_filled(slit, 0.5, rgba(led, 150));
    }

    if selected {
        painter.rect_stroke(
            cap.expand(0.5),
            radius + 0.5,
            Stroke::new(2.0, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_stroke(
            cap,
            radius,
            Stroke::new(1.0, theme::ACCENT_DIM),
            egui::StrokeKind::Inside,
        );
    } else {
        painter.rect_stroke(
            cap,
            radius,
            Stroke::new(1.0, Color32::from_black_alpha(90)),
            egui::StrokeKind::Inside,
        );
    }
}

fn draw_knob(
    painter: &egui::Painter,
    rect: Rect,
    unit: f32,
    led: [u8; 3],
    show_rgb: bool,
    pressed: bool,
    hovered: bool,
    selected: bool,
    marked: bool,
) {
    let sink = if pressed { unit * 0.02 } else { 0.0 };
    let center = rect.center() + vec2(0.0, sink);
    let radius = rect.width().min(rect.height()) * 0.44;
    let lit = show_rgb && led.iter().map(|c| *c as u32).sum::<u32>() > 24;

    if lit {
        painter.circle_filled(
            center,
            radius * 1.14,
            rgba(led, if pressed { 55 } else { 28 }),
        );
    }
    painter.circle_filled(
        center + vec2(0.0, unit * 0.03),
        radius,
        Color32::from_black_alpha(130),
    );
    painter.circle_filled(center, radius, Color32::from_rgb(22, 24, 28));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_rgb(72, 78, 88)),
    );

    // Knurling around the edge.
    let ticks = 28;
    for tick in 0..ticks {
        let angle = tick as f32 / ticks as f32 * TAU;
        let (sin, cos) = angle.sin_cos();
        painter.line_segment(
            [
                center + vec2(cos, sin) * (radius * 0.86),
                center + vec2(cos, sin) * (radius * 0.97),
            ],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(126, 132, 142, 85)),
        );
    }

    painter.circle_filled(center, radius * 0.78, Color32::from_rgb(38, 41, 47));
    painter.circle_stroke(
        center,
        radius * 0.78,
        Stroke::new(1.0, Color32::from_rgb(58, 63, 72)),
    );
    // A fixed pointer notch, as on the stock knob.
    let angle: f32 = -0.72;
    let (sin, cos) = angle.sin_cos();
    painter.line_segment(
        [
            center + vec2(cos, sin) * radius * 0.18,
            center + vec2(cos, sin) * radius * 0.66,
        ],
        Stroke::new(
            (unit * 0.035).clamp(1.5, 3.0),
            Color32::from_rgb(214, 219, 228),
        ),
    );

    if selected {
        painter.circle_stroke(
            center,
            radius + unit * 0.035,
            Stroke::new(2.0, theme::ACCENT),
        );
    } else if hovered {
        painter.circle_stroke(
            center,
            radius + unit * 0.02,
            Stroke::new(1.0, theme::ACCENT_DIM),
        );
    }
    if marked {
        let at = center + vec2(radius * 0.52, -radius * 0.52);
        painter.circle_filled(at, (unit * 0.035).clamp(1.6, 2.6), theme::ACCENT);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_legends(
    painter: &egui::Painter,
    rect: Rect,
    unit: f32,
    layout: &Layout,
    key: &KeyDef,
    view: &KeymapView<'_>,
    action: &Action,
    base_action: &Action,
    remapped: bool,
) {
    let base_label = layout.label(key.index);
    let mapped_label = action.short(view.macros);
    let center_label = if remapped {
        widgets::truncate(&mapped_label, if key.w > 1.4 { 12 } else { 6 })
    } else {
        widgets::truncate(&base_label, if key.w > 1.4 { 14 } else { 7 })
    };
    let font_size = (unit * 0.22).clamp(8.0, 14.0);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        center_label,
        FontId::proportional(font_size),
        LEGEND,
    );

    // Base legend + FN hint.
    if remapped {
        let small = (unit * 0.145).clamp(6.0, 9.5);
        painter.text(
            pos2(rect.left() + unit * 0.07, rect.top() + unit * 0.06),
            Align2::LEFT_TOP,
            widgets::truncate(&base_label, 4),
            FontId::proportional(small),
            theme::MUTED,
        );
    }
    if view.layer == Layer::Normal && view.tool == Tool::Select {
        let fn_action = view.func.get(key.index).cloned().unwrap_or_default();
        if fn_action != *base_action {
            let small = (unit * 0.135).clamp(5.5, 9.0);
            painter.text(
                pos2(rect.right() - unit * 0.08, rect.bottom() - unit * 0.07),
                Align2::RIGHT_BOTTOM,
                widgets::truncate(&fn_action.short(view.macros), 4),
                FontId::proportional(small),
                theme::ACCENT.gamma_multiply(0.75),
            );
        }
    }
}

fn key_rect(origin: egui::Pos2, key: &KeyDef, unit: f32, gap: f32) -> Rect {
    Rect::from_min_size(
        egui::pos2(
            origin.x + key.x * unit + gap * 0.5,
            origin.y + key.y * unit + gap * 0.5,
        ),
        egui::vec2(key.w * unit - gap, key.h * unit - gap),
    )
}

fn rgba(color: [u8; 3], alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color[0], color[1], color[2], alpha)
}
