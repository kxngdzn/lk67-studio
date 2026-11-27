//! Lighting page: hardware effects, palette, custom per-key layouts and animations.

use egui::RichText;

use crate::app::App;
use crate::driver::lk67;
use crate::model::layout::KEY_COUNT;
use crate::model::lighting::{
    AnimPreset, CustomPreset, Direction, HARDWARE_EFFECTS, Keyframe, Lighting, LightingMode,
};
use crate::ui::Page;
use crate::ui::theme;
use crate::ui::widgets;

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let index = app.cfg.active_profile;
    let mode = app.cfg.profiles[index].lighting.mode;
    let before = app.cfg.profiles[index].lighting.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("OUTPUT")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        for candidate in LightingMode::ALL {
            let selected = mode == candidate;
            if ui.selectable_label(selected, candidate.label()).clicked() && !selected {
                app.cfg.profiles[index].lighting.mode = candidate;
                app.ui.dirty = true;
                app.ui.apply_lighting = true;
                app.notify(format!("Lighting mode: {}", candidate.label()));
            }
        }
        ui.separator();
        match mode {
            LightingMode::Music => {
                if ui.button("Music settings").clicked() {
                    app.ui.page = Page::Music;
                }
            }
            LightingMode::Screen => {
                if ui.button("Screen settings").clicked() {
                    app.ui.page = Page::Screen;
                }
            }
            _ => {}
        }
    });
    ui.add_space(4.0);

    match mode {
        LightingMode::Hardware => hardware(app, ui, index),
        LightingMode::Custom => custom(app, ui, index),
        LightingMode::Music | LightingMode::Screen => live_summary(app, ui, mode),
    }

    // Any output-related edit reaches the keyboard without another Apply click.
    if static_lighting_changed(&before, &app.cfg.profiles[index].lighting) {
        app.ui.dirty = true;
        app.ui.lighting_edit_at = Some(std::time::Instant::now());
    }
}

/// Music and screen modes are configured on their own pages; this page only
/// reports the output state instead of repeating those controls.
fn live_summary(app: &mut App, ui: &mut egui::Ui, mode: LightingMode) {
    let index = app.cfg.active_profile;
    theme::card(ui, mode.label(), |ui| {
        theme::hint(
            ui,
            match mode {
                LightingMode::Music => {
                    "The keyboard draws its own bars from the live band stream. Capture source, sensitivity and visual live on the Music reactive page."
                }
                _ => {
                    "The keyboard paints one blended colour from the captured region. Region, grid and colour shaping live on the Screen follow page."
                }
            },
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let running = match mode {
                LightingMode::Music => app.audio.is_running(),
                _ => app.screen.is_running(),
            };
            theme::badge(
                ui,
                if running {
                    "CAPTURING"
                } else {
                    "NOT CAPTURING"
                },
                if running { theme::OK } else { theme::WARN },
            );
            let (label, page) = match mode {
                LightingMode::Music => ("Open music reactive", Page::Music),
                _ => ("Open screen follow", Page::Screen),
            };
            if ui.button(label).clicked() {
                app.ui.page = page;
            }
        });
        ui.add_space(4.0);
        let lighting = &mut app.cfg.profiles[index].lighting;
        if widgets::slider_percent(ui, "Brightness", &mut lighting.brightness) {
            app.ui.dirty = true;
        }
    });
}

fn static_lighting_changed(before: &Lighting, after: &Lighting) -> bool {
    before.mode != after.mode
        || before.effect != after.effect
        || before.colors != after.colors
        || before.brightness != after.brightness
        || before.speed != after.speed
        || before.direction != after.direction
        || before.custom.keys != after.custom.keys
}

fn hardware(app: &mut App, ui: &mut egui::Ui, index: usize) {
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "13 firmware effects", |ui| {
            egui::ScrollArea::vertical()
                .max_height(330.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (effect_index, def) in HARDWARE_EFFECTS.iter().enumerate() {
                        let selected = app.cfg.profiles[index].lighting.effect == effect_index;
                        let label = format!("{:02}  {}", effect_index + 1, def.name);
                        if ui
                            .selectable_label(selected, RichText::new(label).size(13.0))
                            .clicked()
                        {
                            app.cfg.profiles[index].lighting.effect = effect_index;
                            app.ui.dirty = true;
                            app.notify(format!("Effect: {}", def.name));
                        }
                        ui.label(
                            RichText::new(def.description)
                                .size(10.0)
                                .color(theme::MUTED),
                        );
                        ui.add_space(3.0);
                    }
                });
            theme::hint(
                ui,
                "Effect indices are mapped to firmware bytes in driver/lk67.rs.",
            );
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(
            &mut cols[1],
            "Palette, brightness and motion",
            left_height,
            |ui| {
                let driver_cfg = app.cfg.driver.clone();
                let lighting = &mut app.cfg.profiles[index].lighting;
                ui.horizontal(|ui| {
                    ui.label("Primary colour");
                    ui.color_edit_button_srgb(&mut lighting.colors[0]);
                    theme::hint(
                        ui,
                        "Slot 1 is the primary colour for single-colour effects.",
                    );
                });
                ui.add_space(2.0);
                if widgets::palette_editor(ui, &mut lighting.colors) {
                    app.ui.dirty = true;
                }
                if let Some(slot) =
                    widgets::palette_swatches(ui, &lighting.colors, app.ui.palette_index)
                {
                    app.ui.palette_index = slot;
                    app.ui.paint_color = lighting.colors[slot];
                }
                ui.add_space(4.0);
                if widgets::slider_percent(ui, "Brightness", &mut lighting.brightness) {
                    app.ui.dirty = true;
                }
                if widgets::slider_percent(ui, "Speed", &mut lighting.speed) {
                    app.ui.dirty = true;
                }
                ui.horizontal(|ui| {
                    ui.label("Direction");
                    egui::ComboBox::from_id_salt("light-dir")
                        .selected_text(lighting.direction.label())
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            for direction in Direction::ALL {
                                if ui
                                    .selectable_value(
                                        &mut lighting.direction,
                                        direction,
                                        direction.label(),
                                    )
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        });
                });
                ui.add_space(4.0);
                let effect = &HARDWARE_EFFECTS[lighting.effect];
                theme::hint(
                    ui,
                    format!(
                        "{} - {}{}",
                        lighting.effect_def().name,
                        if effect.animated {
                            "animated: speed and direction apply"
                        } else {
                            "static: speed and direction are ignored"
                        },
                        if effect.uses_color {
                            ", palette colours used"
                        } else {
                            ""
                        }
                    ),
                );
                let report = lk67::build_effect_report(&driver_cfg, lighting);
                theme::hint(
                    ui,
                    format!(
                        "Packet ({} bytes): {}",
                        report.len(),
                        widgets::truncate(&lk67::hex_dump(&report), 64)
                    ),
                );
                ui.horizontal(|ui| {
                    if ui.button("Reset palette").clicked() {
                        lighting.colors = crate::model::lighting::default_palette();
                        app.ui.dirty = true;
                    }
                });
            },
        );
    });
}

fn custom(app: &mut App, ui: &mut egui::Ui, index: usize) {
    let mut send_static = false;
    let mut save_preset = false;
    let mut load_preset: Option<usize> = None;
    let mut delete_preset: Option<usize> = None;
    let mut save_anim = false;
    let mut load_anim: Option<usize> = None;
    let mut delete_anim: Option<usize> = None;

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Paint the keymap", |ui| {
            let lighting = &mut app.cfg.profiles[index].lighting;
            ui.horizontal_wrapped(|ui| {
                if let Some(slot) =
                    widgets::palette_swatches(ui, &lighting.colors, app.ui.palette_index)
                {
                    app.ui.palette_index = slot;
                    app.ui.paint_color = lighting.colors[slot];
                    app.ui.tool = crate::ui::Tool::Paint;
                }
                ui.separator();
                ui.label("Custom");
                ui.color_edit_button_srgb(&mut app.ui.paint_color);
            });
            ui.add_space(4.0);
            if widgets::slider_percent(ui, "Brightness", &mut lighting.brightness) {
                app.ui.dirty = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Fill all").clicked() {
                    let color = app.ui.paint_color;
                    lighting.custom.fill(color);
                    lighting.animation_playing = false;
                    app.ui.dirty = true;
                }
                if ui.button("Clear").clicked() {
                    lighting.custom.fill([0, 0, 0]);
                    lighting.animation_playing = false;
                    app.ui.dirty = true;
                }
                if ui.button("Sample preview").clicked() {
                    lighting.custom.keys = app.colors.clone();
                    lighting.animation_playing = false;
                    app.ui.dirty = true;
                }
            });
            ui.add_space(4.0);
            theme::hint(
                ui,
                "Click or drag on the 65% canvas (Paint tool) to set per-key colours. Clicking a key also selects it.",
            );
            if app.cfg.profiles[index].lighting.animation_playing {
                theme::hint(
                    ui,
                    "A keyframe animation is playing and hides the painted layout; painting or Load turns it off.",
                );
            }
            ui.add_space(4.0);
            let uploading = app.ui.lighting_uploading;
            let button = if uploading {
                egui::Button::new("Uploading picture...").fill(theme::ACCENT_DIM)
            } else {
                egui::Button::new("Send per-key picture").fill(theme::ACCENT_DIM)
            };
            if ui.add_enabled(!uploading, button).clicked() {
                send_static = true;
            }
            theme::hint(
                ui,
                "The patterned picture lives in the keyboard's flash, so it is sent as one paced upload; colours on screen change instantly.",
            );
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "Static presets", left_height, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui.preset_name)
                        .hint_text("Preset name")
                        .desired_width(150.0),
                );
                if ui.button("Save current").clicked() {
                    save_preset = true;
                }
            });
            ui.add_space(4.0);
            let presets = app.cfg.profiles[index].lighting.presets.clone();
            if presets.is_empty() {
                theme::hint(ui, "No presets saved yet.");
            }
            egui::ScrollArea::vertical()
                .max_height(170.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (preset_index, preset) in presets.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(widgets::truncate(&preset.name, 22));
                            if ui.small_button("Load").clicked() {
                                load_preset = Some(preset_index);
                            }
                            if ui.small_button("Delete").clicked() {
                                delete_preset = Some(preset_index);
                            }
                        });
                    }
                });
            theme::hint(
                ui,
                "Presets store all 68 key colours. They live in the active profile.",
            );
        });
    });

    ui.add_space(8.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Keyframe animation", |ui| {
            let lighting = &mut app.cfg.profiles[index].lighting;
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut lighting.animation_playing, "Play animation")
                    .changed()
                {
                    app.ui.dirty = true;
                }
                ui.label(
                    RichText::new(format!("{} frames", lighting.animation.keyframes.len()))
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });
            let mut fps = lighting.animation.fps as f32;
            if ui
                .add(egui::Slider::new(&mut fps, 5.0..=60.0).text("FPS"))
                .changed()
            {
                lighting.animation.fps = fps.round() as u8;
                app.ui.dirty = true;
            }
            if widgets::slider_percent(ui, "Playback speed", &mut lighting.animation.speed) {
                app.ui.dirty = true;
            }
            ui.horizontal(|ui| {
                ui.label("Direction");
                egui::ComboBox::from_id_salt("anim-dir")
                    .selected_text(lighting.animation.direction.label())
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for direction in Direction::ALL {
                            if ui
                                .selectable_value(
                                    &mut lighting.animation.direction,
                                    direction,
                                    direction.label(),
                                )
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                if ui.button("Capture current layout").clicked() {
                    let t = lighting.animation.duration_ms();
                    lighting.animation.keyframes.push(Keyframe {
                        t_ms: t,
                        keys: lighting.custom.keys.clone(),
                    });
                    lighting.animation.ensure_len(KEY_COUNT);
                    lighting.animation_playing = true;
                    app.ui.dirty = true;
                    app.ui.notify("Captured keyframe from the custom layout");
                }
                if ui.button("Capture preview frame").clicked() {
                    let keys = app.colors.clone();
                    let t = lighting.animation.duration_ms();
                    lighting.animation.keyframes.push(Keyframe { t_ms: t, keys });
                    lighting.animation_playing = true;
                    app.ui.dirty = true;
                }
            });
            let frames = lighting.animation.keyframes.clone();
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if frames.is_empty() {
                        theme::hint(ui, "Add at least one keyframe.");
                    }
                    for (frame_index, frame) in frames.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "#{}  t={} ms",
                                    frame_index + 1,
                                    frame.t_ms
                                ))
                                .size(11.0),
                            );
                            if ui.small_button("Delete").clicked() {
                                lighting.animation.keyframes.remove(frame_index);
                                app.ui.dirty = true;
                            }
                        });
                    }
                });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.ui.anim_name)
                        .hint_text("Animation name")
                        .desired_width(140.0),
                );
                if ui.button("Save animation").clicked() {
                    save_anim = true;
                }
            });
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "Animation library", left_height, |ui| {
            let animations = app.cfg.profiles[index].lighting.animations.clone();
            if animations.is_empty() {
                theme::hint(ui, "Saved animations appear here.");
            }
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (anim_index, anim) in animations.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(widgets::truncate(&anim.name, 20));
                            theme::hint(ui, format!("{} fps", anim.fps));
                            if ui.small_button("Load").clicked() {
                                load_anim = Some(anim_index);
                            }
                            if ui.small_button("Delete").clicked() {
                                delete_anim = Some(anim_index);
                            }
                        });
                    }
                });
            ui.add_space(4.0);
            theme::hint(
                ui,
                "Animations interpolate between keyframes per key; speed and direction add a wave sweep.",
            );
            theme::hint(
                ui,
                "Animated previews run in the app only: the LK67 stores one still picture in flash, so send the layout after you finish painting.",
            );
        });
    });

    // Apply deferred edits (avoids holding profile borrows across driver calls).
    if save_preset {
        let name = if app.ui.preset_name.trim().is_empty() {
            format!(
                "Preset {}",
                app.cfg.profiles[index].lighting.presets.len() + 1
            )
        } else {
            app.ui.preset_name.trim().to_string()
        };
        let keys = app.cfg.profiles[index].lighting.custom.keys.clone();
        app.cfg.profiles[index]
            .lighting
            .presets
            .push(CustomPreset { name, keys });
        app.ui.preset_name.clear();
        app.ui.dirty = true;
        app.notify("Preset saved");
    }
    if let Some(preset_index) = load_preset {
        if let Some(preset) = app.cfg.profiles[index]
            .lighting
            .presets
            .get(preset_index)
            .cloned()
        {
            let lighting = &mut app.cfg.profiles[index].lighting;
            lighting.custom.keys = preset.keys;
            lighting.custom.ensure_len(KEY_COUNT);
            lighting.mode = LightingMode::Custom;
            lighting.animation_playing = false;
            app.ui.dirty = true;
            app.notify(format!("Loaded preset {}", preset.name));
        }
    }
    if let Some(preset_index) = delete_preset {
        if preset_index < app.cfg.profiles[index].lighting.presets.len() {
            app.cfg.profiles[index]
                .lighting
                .presets
                .remove(preset_index);
            app.ui.dirty = true;
        }
    }
    if save_anim {
        let name = if app.ui.anim_name.trim().is_empty() {
            format!(
                "Animation {}",
                app.cfg.profiles[index].lighting.animations.len() + 1
            )
        } else {
            app.ui.anim_name.trim().to_string()
        };
        let mut anim: AnimPreset = app.cfg.profiles[index].lighting.animation.clone();
        anim.name = name;
        anim.ensure_len(KEY_COUNT);
        app.cfg.profiles[index].lighting.animations.push(anim);
        app.ui.anim_name.clear();
        app.ui.dirty = true;
        app.notify("Animation saved to the library");
    }
    if let Some(anim_index) = load_anim {
        if let Some(mut anim) = app.cfg.profiles[index]
            .lighting
            .animations
            .get(anim_index)
            .cloned()
        {
            anim.ensure_len(KEY_COUNT);
            let lighting = &mut app.cfg.profiles[index].lighting;
            lighting.animation = anim;
            lighting.animation_playing = true;
            lighting.mode = LightingMode::Custom;
            app.ui.dirty = true;
            app.notify("Animation loaded (preview only)");
        }
    }
    if let Some(anim_index) = delete_anim {
        if anim_index < app.cfg.profiles[index].lighting.animations.len() {
            app.cfg.profiles[index]
                .lighting
                .animations
                .remove(anim_index);
            app.ui.dirty = true;
        }
    }

    if send_static {
        app.ui.lighting_edit_at = None;
        app.upload_custom_layout();
    }
}
