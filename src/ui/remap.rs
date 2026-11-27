//! Remap page: layer selection, per-key inspector and the assignment builder.

use egui::RichText;

use crate::app::App;
use crate::model::action::{MediaKey, RgbCommand};
use crate::model::layout::{KEY_COUNT, layout};
use crate::model::profile::KnobTurn;
use crate::model::{Action, Layer};
use crate::ui::{ActionKind, UiState, theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let profile_index = app.cfg.active_profile;

    ui.horizontal_wrapped(|ui| {
        if ui.button("Reset whole keymap").clicked() {
            app.profile_mut().reset_keymap();
            app.ui.dirty = true;
            app.notify("Keymap reset to the stock LK67 layout");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            theme::hint(
                ui,
                format!(
                    "{} remapped",
                    app.cfg.profiles[profile_index].remap_count()
                ),
            );
        });
    });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        selected_card(app, &mut cols[0], profile_index);
        let height = cols[0].min_rect().height();
        assignment_card(app, &mut cols[1], profile_index, height);
    });

    ui.add_space(8.0);
    knob_card(app, ui, profile_index);
}

fn knob_card(app: &mut App, ui: &mut egui::Ui, profile_index: usize) {
    theme::card(ui, "Volume knob", |ui| {
        let Some(knob) = layout().index_of_vk(0xB3) else {
            return;
        };
        let macros = app.cfg.profiles[profile_index].macros.clone();
        let press = app.cfg.profiles[profile_index].normal[knob].clone();
        let mut set_media: Option<MediaKey> = None;
        let mut turn = app.cfg.settings.knob_turn;

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("PRESS").size(10.0).color(theme::MUTED));
            ui.label(RichText::new(press.summary(&macros)).size(11.5));
            if ui.button("Play/Pause").clicked() {
                set_media = Some(MediaKey::PlayPause);
            }
            if ui.button("Mute").clicked() {
                set_media = Some(MediaKey::Mute);
            }
            theme::hint(
                ui,
                "The press is a normal key: select Knob on the canvas to bind a combo, macro or lighting command.",
            );
        });

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("TURN").size(10.0).color(theme::MUTED));
            egui::ComboBox::from_id_salt("knob-turn")
                .selected_text(turn.label())
                .width(180.0)
                .show_ui(ui, |ui| {
                    for mode in KnobTurn::ALL {
                        if ui.selectable_value(&mut turn, mode, mode.label()).clicked() {
                            ui.close();
                        }
                    }
                });
            theme::hint(
                ui,
                "Volume sends volume keys to Windows. The other modes need software remapping on.",
            );
        });

        let press_ago = app
            .ui
            .knob_press_at
            .map(|at| format!("{:.0}s ago", at.elapsed().as_secs_f32()))
            .unwrap_or_else(|| "never".into());
        let turn_ago = app
            .ui
            .knob_turn_at
            .map(|at| format!("{:.0}s ago", at.elapsed().as_secs_f32()))
            .unwrap_or_else(|| "never".into());
        ui.label(
            RichText::new(format!("Seen: press {press_ago}  -  turn {turn_ago}"))
                .size(10.5)
                .color(theme::MUTED),
        );

        if turn != app.cfg.settings.knob_turn {
            app.cfg.settings.knob_turn = turn;
            // The turn modes only work through software remapping, so selecting
            // one turns it on instead of silently doing nothing.
            if turn != KnobTurn::Volume {
                app.cfg.settings.hook_enabled = true;
                app.cfg.settings.remap_enabled = true;
            }
            app.ui.dirty = true;
            app.notify(format!("Knob turn: {}", turn.label()));
        }
        if let Some(media) = set_media {
            app.cfg.profiles[profile_index].normal[knob] = Action::Media { media };
            app.cfg.settings.hook_enabled = true;
            app.cfg.settings.remap_enabled = true;
            app.ui.dirty = true;
            app.notify(format!("Knob press -> {}", media.label()));
        }
    });
}

fn selected_card(app: &mut App, ui: &mut egui::Ui, profile_index: usize) {
    theme::card(ui, "Selected key", |ui| {
        let Some(index) = app.ui.selected else {
            theme::hint(ui, "Click any key on the canvas above to inspect it.");
            theme::hint(ui, "Keys show the active layer, live RGB and remap state.");
            return;
        };
        let profile = &app.cfg.profiles[profile_index];
        let key = layout().key(index);
        let current = profile.action(app.ui.layer, index).clone();
        let base = profile.normal[index].clone();
        let default_layer = match app.ui.layer {
            Layer::Normal => layout().default_normal()[index].clone(),
            Layer::Fn => layout().default_func()[index].clone(),
        };

        ui.horizontal(|ui| {
            ui.label(RichText::new(layout().label(index)).size(18.0).strong());
            theme::badge(ui, app.ui.layer.label(), theme::ACCENT_DIM);
            if current != default_layer {
                theme::badge(ui, "REMAPPED", theme::WARN);
            }
        });
        ui.add_space(2.0);
        egui::Grid::new("selected-details")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                widgets::kv(
                    ui,
                    "Physical key",
                    format!("{}  (VK {:02X})", key.code, key.vk),
                );
                widgets::kv(ui, "Current action", current.summary(&profile.macros));
                widgets::kv(ui, "Normal layer", base.summary(&profile.macros));
                widgets::kv(
                    ui,
                    "Stock for layer",
                    default_layer.summary(&profile.macros),
                );
                widgets::kv(ui, "LED index", index.to_string());
                widgets::kv(ui, "Matrix slot (placeholder)", key.matrix.to_string());
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Reset to stock").clicked() {
                reset_key(app, index);
            }
            if ui.button("Use base assignment").clicked() {
                let normal_index = index;
                let action = app.cfg.profiles[profile_index].normal[normal_index].clone();
                *app.cfg.profiles[profile_index].action_mut(app.ui.layer, index) = action;
                app.ui.dirty = true;
                app.notify("Copied Normal layer assignment to this layer");
            }
        });
    });
}

fn assignment_card(app: &mut App, ui: &mut egui::Ui, profile_index: usize, height: f32) {
    theme::card_fill(ui, "Assignment", height, |ui| {
        if app.ui.selected.is_none() {
            theme::hint(ui, "Nothing selected.");
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("TYPE").size(10.0).color(theme::MUTED));
            egui::ComboBox::from_id_salt("assign-kind")
                .selected_text(app.ui.assign_kind.label())
                .width(170.0)
                .show_ui(ui, |ui| {
                    for kind in ActionKind::ALL {
                        if ui
                            .selectable_value(&mut app.ui.assign_kind, kind, kind.label())
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                });
        });

        match app.ui.assign_kind {
            ActionKind::Key => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Key");
                    widgets::key_picker(ui, "assign-key", &mut app.ui.assign_key);
                    theme::hint(ui, "Emits this key instead of the physical one.");
                });
            }
            ActionKind::Combo => {
                ui.horizontal(|ui| {
                    widgets::mods_row(ui, &mut app.ui.combo_mods);
                });
                ui.horizontal(|ui| {
                    ui.label("Key");
                    widgets::key_picker(ui, "assign-combo-key", &mut app.ui.combo_key);
                    theme::hint(ui, "e.g. Ctrl + Shift + Esc");
                });
            }
            ActionKind::Macro => {
                let macros = app.cfg.profiles[profile_index].macros.clone();
                if macros.is_empty() {
                    theme::hint(ui, "No macros yet. Create one on the Macros page.");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Macro");
                        let name = macros
                            .iter()
                            .find(|m| m.id == app.ui.assign_macro)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| "select...".into());
                        egui::ComboBox::from_id_salt("assign-macro")
                            .selected_text(widgets::truncate(&name, 22))
                            .width(190.0)
                            .show_ui(ui, |ui| {
                                for def in &macros {
                                    if ui
                                        .selectable_value(
                                            &mut app.ui.assign_macro,
                                            def.id,
                                            format!("{}  ({})", def.name, def.summary()),
                                        )
                                        .clicked()
                                    {
                                        ui.close();
                                    }
                                }
                            });
                    });
                }
            }
            ActionKind::Media => {
                ui.horizontal(|ui| {
                    ui.label("Media");
                    egui::ComboBox::from_id_salt("assign-media")
                        .selected_text(app.ui.assign_media.label())
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for media in MediaKey::ALL {
                                if ui
                                    .selectable_value(
                                        &mut app.ui.assign_media,
                                        media,
                                        media.label(),
                                    )
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        });
                });
            }
            ActionKind::Rgb => {
                ui.horizontal(|ui| {
                    ui.label("Command");
                    egui::ComboBox::from_id_salt("assign-rgb")
                        .selected_text(app.ui.assign_rgb.label())
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for command in RgbCommand::ALL {
                                if ui
                                    .selectable_value(
                                        &mut app.ui.assign_rgb,
                                        command,
                                        command.label(),
                                    )
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        });
                });
            }
            ActionKind::Profile => {
                ui.horizontal(|ui| {
                    ui.label("Slot");
                    for slot in 0..4u8 {
                        ui.selectable_value(
                            &mut app.ui.assign_slot,
                            slot,
                            format!("P{}", slot + 1),
                        );
                    }
                });
            }
            ActionKind::Fn => {
                theme::hint(ui, "Hold this key to activate the FN layer.");
            }
            ActionKind::None => {
                theme::hint(ui, "Key is swallowed (does nothing).");
            }
        }

        ui.add_space(6.0);
        let preview = build_action(&app.ui);
        theme::hint(
            ui,
            format!(
                "Preview: {}",
                preview.summary(&app.cfg.profiles[profile_index].macros)
            ),
        );
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Apply to key").fill(theme::ACCENT_DIM))
                .clicked()
            {
                apply_action(app, profile_index);
            }
            if ui.button("Reload").clicked() {
                if let Some(index) = app.ui.selected {
                    app.load_action_into_editor(index);
                }
            }
        });
    });
}

fn build_action(ui: &UiState) -> Action {
    match ui.assign_kind {
        ActionKind::Key => Action::Key { key: ui.assign_key },
        ActionKind::Combo => Action::Combo {
            mods: ui.combo_mods,
            key: ui.combo_key,
        },
        ActionKind::Macro => Action::Macro {
            macro_id: ui.assign_macro,
        },
        ActionKind::Media => Action::Media {
            media: ui.assign_media,
        },
        ActionKind::Fn => Action::Fn,
        ActionKind::Rgb => Action::Rgb {
            command: ui.assign_rgb,
        },
        ActionKind::Profile => Action::Profile {
            slot: ui.assign_slot.min(3),
        },
        ActionKind::None => Action::None,
    }
}

fn apply_action(app: &mut App, profile_index: usize) {
    let Some(index) = app.ui.selected else {
        return;
    };
    let action = build_action(&app.ui);
    if let Action::Macro { macro_id } = action {
        if app.cfg.profiles[profile_index]
            .macro_by_id(macro_id)
            .is_none()
        {
            app.notify("Pick an existing macro (or create one on the Macros page)");
            return;
        }
    }
    if let Action::Key { key } = action {
        if key.is_none() {
            app.notify("Pick a key for the assignment");
            return;
        }
    }
    let layer = app.ui.layer;
    let summary = action.summary(&app.cfg.profiles[profile_index].macros);
    *app.cfg.profiles[profile_index].action_mut(layer, index) = action;
    // Make the assignment effective right away. Without this the hook keeps
    // passing the physical key through until the user finds the separate
    // Settings toggle or Driver > Write keymap button.
    app.cfg.settings.hook_enabled = true;
    app.cfg.settings.remap_enabled = true;
    app.ui.dirty = true;
    app.notify(format!(
        "{} [{}] -> {} (software remap on)",
        layout().label(index),
        layer.label(),
        summary
    ));
}

fn reset_key(app: &mut App, index: usize) {
    if index >= KEY_COUNT {
        return;
    }
    let layer = app.ui.layer;
    let stock = match layer {
        Layer::Normal => layout().default_normal()[index].clone(),
        Layer::Fn => layout().default_func()[index].clone(),
    };
    *app.cfg.profile_mut().action_mut(layer, index) = stock;
    app.ui.dirty = true;
    app.ui.notify(format!(
        "Reset {} on the {} layer",
        layout().label(index),
        layer.label()
    ));
}
