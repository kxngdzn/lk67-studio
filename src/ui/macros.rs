//! Macro library and editor.

use egui::RichText;

use crate::app::App;
use crate::model::macro_def::{MacroDef, MacroStep, RepeatMode};
use crate::ui::{ActionKind, StepKindChoice, theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let index = app.cfg.active_profile;
    let mut create = false;
    let mut duplicate = false;
    let mut delete_macro: Option<u64> = None;
    let mut start_record = false;
    let mut stop_record = false;
    let mut test_macro: Option<u64> = None;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("MACRO LIBRARY")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        if ui.button("New macro").clicked() {
            create = true;
        }
        if ui.button("Duplicate").clicked() {
            duplicate = true;
        }
        if ui.button("Delete").clicked() {
            delete_macro = app.ui.selected_macro;
        }
        ui.separator();
        if app.recording_id().is_some() {
            if ui
                .add(egui::Button::new("Stop & save recording").fill(theme::DANGER))
                .clicked()
            {
                stop_record = true;
            }
            theme::badge(ui, "RECORDING", theme::DANGER);
            ui.label(
                RichText::new(format!("{} steps", app.recording_steps()))
                    .size(11.0)
                    .color(theme::DANGER),
            );
            if let Some((vk, _)) = app.recording_last_key() {
                ui.label(
                    RichText::new(format!(
                        "last {}",
                        crate::model::keycode::KeyCode(vk).display()
                    ))
                    .size(11.0)
                    .color(theme::MUTED),
                );
            }
        } else if ui
            .add(egui::Button::new("Record keys").fill(theme::ACCENT_DIM))
            .clicked()
        {
            start_record = true;
        }
        if let Some(id) = app.ui.selected_macro {
            if ui.button("Test playback").clicked() {
                test_macro = Some(id);
            }
            if ui.button("Stop playback").clicked() {
                app.player.stop_all();
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            theme::hint(
                ui,
                "Test playback injects real keystrokes - focus a scratch document.",
            );
        });
    });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Macros", |ui| {
            let macros = app.cfg.profiles[index].macros.clone();
            if macros.is_empty() {
                theme::hint(ui, "No macros yet. Create one or record your keys.");
            }
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for def in &macros {
                        let selected = app.ui.selected_macro == Some(def.id);
                        let label = format!("{}  ({})", def.name, def.repeat.label());
                        if ui.selectable_label(selected, label).clicked() {
                            app.ui.selected_macro = Some(def.id);
                        }
                        ui.label(
                            RichText::new(widgets::truncate(&def.summary(), 36))
                                .size(10.5)
                                .color(theme::MUTED),
                        );
                        ui.add_space(2.0);
                    }
                });
            ui.add_space(4.0);
            theme::hint(
                ui,
                "Bind a macro from Remap -> Assignment -> Macro, or from the canvas.",
            );
        });

        theme::card(&mut cols[1], "Editor", |ui| {
            let Some(id) = app.ui.selected_macro else {
                theme::hint(ui, "Select a macro to edit it.");
                theme::hint(ui, "Steps are recorded with real delays, then replayable.");
                return;
            };
            let Some(macro_index) = app.cfg.profiles[index]
                .macros
                .iter()
                .position(|m| m.id == id)
            else {
                theme::hint(ui, "This macro no longer exists.");
                return;
            };

            let mut dirty = false;
            {
                let def = &mut app.cfg.profiles[index].macros[macro_index];
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if ui
                        .add(egui::TextEdit::singleline(&mut def.name).desired_width(200.0))
                        .changed()
                    {
                        dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Repeat");
                    egui::ComboBox::from_id_salt("macro-repeat")
                        .selected_text(def.repeat.label())
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut def.repeat, RepeatMode::Once, "Once")
                                .clicked()
                            {
                                dirty = true;
                                ui.close();
                            }
                            if ui
                                .selectable_value(
                                    &mut def.repeat,
                                    RepeatMode::Count { count: 3 },
                                    "Repeat N times",
                                )
                                .clicked()
                            {
                                dirty = true;
                                ui.close();
                            }
                            if ui
                                .selectable_value(
                                    &mut def.repeat,
                                    RepeatMode::Toggle,
                                    "Toggle loop",
                                )
                                .clicked()
                            {
                                dirty = true;
                                ui.close();
                            }
                            if ui
                                .selectable_value(
                                    &mut def.repeat,
                                    RepeatMode::Hold,
                                    "Loop while held",
                                )
                                .clicked()
                            {
                                dirty = true;
                                ui.close();
                            }
                        });
                    if let RepeatMode::Count { count } = &mut def.repeat {
                        if ui
                            .add(egui::DragValue::new(count).range(1..=999).suffix("x"))
                            .changed()
                        {
                            dirty = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Default step delay");
                    if ui
                        .add(
                            egui::DragValue::new(&mut def.default_delay_ms)
                                .range(0..=5000)
                                .suffix(" ms"),
                        )
                        .changed()
                    {
                        dirty = true;
                    }
                    theme::hint(ui, "Used when a step delay is 0.");
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;
                let mut delete_step: Option<usize> = None;
                let step_count = def.steps.len();
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("macro-steps")
                            .num_columns(5)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("#").size(10.0).color(theme::MUTED));
                                ui.label(RichText::new("STEP").size(10.0).color(theme::MUTED));
                                ui.label(RichText::new("DELAY").size(10.0).color(theme::MUTED));
                                ui.label("");
                                ui.label("");
                                ui.end_row();
                                for (step_index, step) in def.steps.iter_mut().enumerate() {
                                    ui.label(
                                        RichText::new((step_index + 1).to_string())
                                            .size(10.5)
                                            .color(theme::MUTED),
                                    );
                                    ui.label(RichText::new(step.label()).size(11.5));
                                    let mut delay = step.delay();
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut delay)
                                                .range(0..=5000)
                                                .suffix(" ms"),
                                        )
                                        .changed()
                                    {
                                        step.set_delay(delay);
                                        dirty = true;
                                    }
                                    if ui.small_button("^").clicked() && step_index > 0 {
                                        move_up = Some(step_index);
                                    }
                                    if ui.small_button("v").clicked() && step_index + 1 < step_count
                                    {
                                        move_down = Some(step_index);
                                    }
                                    if ui.small_button("x").clicked() {
                                        delete_step = Some(step_index);
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(step_index) = move_up {
                    def.steps.swap(step_index, step_index - 1);
                    dirty = true;
                }
                if let Some(step_index) = move_down {
                    def.steps.swap(step_index, step_index + 1);
                    dirty = true;
                }
                if let Some(step_index) = delete_step {
                    def.steps.remove(step_index);
                    dirty = true;
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("ADD STEP").size(10.0).color(theme::MUTED));
                    egui::ComboBox::from_id_salt("step-kind")
                        .selected_text(app.ui.step_kind.label())
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            for kind in StepKindChoice::ALL {
                                if ui
                                    .selectable_value(&mut app.ui.step_kind, kind, kind.label())
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        });
                    widgets::key_picker(ui, "step-key", &mut app.ui.step_key);
                    ui.add(
                        egui::DragValue::new(&mut app.ui.step_delay)
                            .range(0..=5000)
                            .suffix(" ms"),
                    );
                    if ui.button("Add").clicked() {
                        let step = match app.ui.step_kind {
                            StepKindChoice::Down => MacroStep::Down {
                                key: app.ui.step_key,
                                delay_ms: app.ui.step_delay,
                            },
                            StepKindChoice::Up => MacroStep::Up {
                                key: app.ui.step_key,
                                delay_ms: app.ui.step_delay,
                            },
                            StepKindChoice::Tap => MacroStep::Tap {
                                key: app.ui.step_key,
                                delay_ms: app.ui.step_delay,
                            },
                            StepKindChoice::Combo => MacroStep::Combo {
                                mods: app.ui.combo_mods,
                                key: app.ui.step_key,
                                hold_ms: 30,
                                delay_ms: app.ui.step_delay,
                            },
                        };
                        def.steps.push(step);
                        dirty = true;
                    }
                    if app.ui.step_kind == StepKindChoice::Combo {
                        widgets::mods_row(ui, &mut app.ui.combo_mods);
                    }
                });
                ui.add_space(2.0);
                theme::hint(
                    ui,
                    format!(
                        "{} steps, about {} ms per pass",
                        def.steps.len(),
                        def.duration_ms()
                    ),
                );
            }

            if dirty {
                app.ui.dirty = true;
                app.player.sync_macros(&app.cfg.profiles[index].macros);
            }

            if !app.cfg.profiles[index].macros[macro_index].steps.is_empty() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Send to macro player").clicked() {
                        app.player.sync_macros(&app.cfg.profile().macros);
                        app.player
                            .play_once(app.cfg.profiles[index].macros[macro_index].id);
                    }
                    if ui.button("Clear steps").clicked() {
                        app.cfg.profiles[index].macros[macro_index].steps.clear();
                        app.ui.dirty = true;
                    }
                });
            }
        });
    });

    if create {
        let id = app.cfg.profiles[index].next_macro_id();
        let name = if app.ui.new_macro_name.trim().is_empty() {
            format!("Macro {id}")
        } else {
            app.ui.new_macro_name.trim().to_string()
        };
        app.cfg.profiles[index].macros.push(MacroDef::new(id, name));
        app.ui.selected_macro = Some(id);
        app.ui.new_macro_name.clear();
        app.ui.dirty = true;
        app.notify("Macro created");
    }
    if duplicate {
        if let Some(id) = app.ui.selected_macro {
            if let Some(source) = app.cfg.profiles[index].macro_by_id(id).cloned() {
                let new_id = app.cfg.profiles[index].next_macro_id();
                let mut copy = source;
                copy.id = new_id;
                copy.name = format!("{} copy", copy.name);
                app.cfg.profiles[index].macros.push(copy);
                app.ui.selected_macro = Some(new_id);
                app.ui.dirty = true;
            }
        }
    }
    if let Some(id) = delete_macro {
        app.cfg.profiles[index].macros.retain(|m| m.id != id);
        {
            let profile = &mut app.cfg.profiles[index];
            for action in profile.normal.iter_mut().chain(profile.func.iter_mut()) {
                if matches!(action, crate::model::Action::Macro { macro_id } if *macro_id == id) {
                    *action = crate::model::Action::None;
                }
            }
        }
        app.ui.selected_macro = app.cfg.profiles[index].macros.first().map(|m| m.id);
        app.ui.dirty = true;
        app.notify("Macro deleted; bound keys were disabled");
    }
    if start_record {
        app.start_recording();
    }
    if stop_record {
        app.stop_recording();
    }
    if let Some(id) = test_macro {
        app.player.sync_macros(&app.cfg.profile().macros);
        app.player.play_once(id);
        app.notify("Macro playback started");
    }

    // Binding hint with quick-assign helper.
    ui.add_space(8.0);
    theme::card(ui, "Bind this macro", |ui| {
        let Some(id) = app.ui.selected_macro else {
            theme::hint(ui, "Select a macro first.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            theme::hint(ui, "Choose a key, then bind:");
            let mut key_choice = app.ui.assign_key;
            widgets::key_picker(ui, "bind-macro-key", &mut key_choice);
            if ui.button("Bind on Normal layer").clicked() {
                if let Some(key_index) = crate::model::layout::layout().index_of_vk(key_choice.vk())
                {
                    app.cfg.profiles[index].normal[key_index] =
                        crate::model::Action::Macro { macro_id: id };
                    app.ui.assign_kind = ActionKind::Macro;
                    app.ui.assign_macro = id;
                    app.ui.selected = Some(key_index);
                    app.ui.dirty = true;
                    app.notify("Macro bound to key");
                } else {
                    app.notify("Pick a key that exists on the LK67");
                }
            }
            theme::hint(ui, "or use Remap for combos and the FN layer.");
        });
    });
}
