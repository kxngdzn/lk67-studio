//! Lighting page: hardware effects, palette and custom per-key layouts.

use egui::RichText;

use crate::app::App;
use crate::driver::lk67;
use crate::model::layout::{KEY_COUNT, layout};
use crate::model::lighting::{
    CustomPreset, Direction, HARDWARE_EFFECTS, ImageFit, Lighting, LightingMode,
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
        LightingMode::Picture => picture(app, ui, index),
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
                    "The keyboard draws its own bars from the live band stream. Choose the capture source and start or stop it on the Music reactive page."
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

fn picture(app: &mut App, ui: &mut egui::Ui, index: usize) {
    let mut choose = false;
    let mut reload = false;
    let mut clear = false;
    let mut send = false;
    let uploading = app.ui.lighting_uploading;
    let has_picture = app.picture.is_some();
    let info = app
        .picture
        .as_ref()
        .map(|picture| (picture.width, picture.height));
    let error = app.picture_error.clone();
    let path_label = {
        let path = &app.cfg.profiles[index].lighting.picture.path;
        if path.trim().is_empty() {
            "No picture selected".to_string()
        } else {
            widgets::truncate(path, 58)
        }
    };

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Picture effect", |ui| {
            let lighting = &mut app.cfg.profiles[index].lighting;
            ui.horizontal_wrapped(|ui| {
                if ui.button("Choose image...").clicked() {
                    choose = true;
                }
                if ui
                    .add_enabled(
                        !lighting.picture.path.trim().is_empty(),
                        egui::Button::new("Reload"),
                    )
                    .clicked()
                {
                    reload = true;
                }
                if ui.button("Clear").clicked() {
                    clear = true;
                }
            });
            ui.label(
                RichText::new(path_label.clone())
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label("Fit");
                egui::ComboBox::from_id_salt("picture-fit")
                    .selected_text(lighting.picture.fit.label())
                    .width(210.0)
                    .show_ui(ui, |ui| {
                        for fit in ImageFit::ALL {
                            if ui
                                .selectable_value(
                                    &mut lighting.picture.fit,
                                    fit,
                                    fit.label(),
                                )
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    });
            });
            if widgets::slider_percent(ui, "Brightness", &mut lighting.brightness) {
                app.ui.dirty = true;
            }
            ui.add_space(4.0);
            let button = if uploading {
                egui::Button::new("Uploading picture...").fill(theme::ACCENT_DIM)
            } else {
                egui::Button::new("Send picture").fill(theme::ACCENT_DIM)
            };
            if ui.add_enabled(!uploading && has_picture, button).clicked() {
                send = true;
            }
            theme::hint(
                ui,
                "PNG, JPEG, GIF, BMP and WebP. Animated GIFs use their first frame.",
            );
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "Picture preview", left_height, |ui| {
            if let Some((width, height)) = info {
                ui.label(
                    RichText::new(format!("{width} x {height} px"))
                        .size(14.0)
                        .strong(),
                );
                ui.add_space(4.0);
                theme::hint(
                    ui,
                    "The keyboard canvas above is the live preview. Changing Fit or the image reloads it here.",
                );
            } else if let Some(err) = &error {
                widgets::status_text(ui, err, false);
                theme::hint(ui, "Choose another file or check that it still exists.");
            } else {
                theme::hint(ui, "Choose an image to map it onto the 68 keys.");
            }
            ui.add_space(4.0);
            theme::hint(
                ui,
                "The LK67's per-key picture is flash-backed and holds one still image.",
            );
        });
    });

    if choose {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file()
        {
            let lighting = &mut app.cfg.profiles[index].lighting;
            lighting.picture.path = path.display().to_string();
            lighting.mode = LightingMode::Picture;
            app.load_picture();
            app.ui.dirty = true;
        }
    }
    if reload {
        app.load_picture();
    }
    if clear {
        let lighting = &mut app.cfg.profiles[index].lighting;
        lighting.picture.path.clear();
        lighting.mode = LightingMode::Hardware;
        app.picture = None;
        app.picture_error = None;
        app.picture_attempt = None;
        app.ui.dirty = true;
        app.ui.apply_lighting = true;
        app.notify("Picture cleared; switched to hardware effects");
    }
    if send {
        app.upload_picture();
    }
}

fn static_lighting_changed(before: &Lighting, after: &Lighting) -> bool {
    before.mode != after.mode
        || before.effect != after.effect
        || before.colors != after.colors
        || before.brightness != after.brightness
        || before.speed != after.speed
        || before.direction != after.direction
        || before.custom.keys != after.custom.keys
        || before.picture != after.picture
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
        let uses_color = HARDWARE_EFFECTS[app.cfg.profiles[index].lighting.effect].uses_color;
        let title = if uses_color {
            "Palette, brightness and motion"
        } else {
            "Brightness and motion"
        };
        theme::card_fill(&mut cols[1], title, left_height, |ui| {
            let driver_cfg = app.cfg.driver.clone();
            let lighting = &mut app.cfg.profiles[index].lighting;
            if uses_color {
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
            } else {
                theme::hint(
                    ui,
                    "This effect uses fixed colours, so the palette slots are ignored.",
                );
                ui.add_space(4.0);
            }
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
                        ", fixed colours"
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
            if uses_color {
                ui.horizontal(|ui| {
                    if ui.button("Reset palette").clicked() {
                        lighting.colors = crate::model::lighting::default_palette();
                        app.ui.dirty = true;
                    }
                });
            }
        });
    });
}

fn custom(app: &mut App, ui: &mut egui::Ui, index: usize) {
    let mut send_static = false;
    let mut save_preset = false;
    let mut load_preset: Option<usize> = None;
    let mut delete_preset: Option<usize> = None;

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
                    app.ui.dirty = true;
                }
                if ui.button("Clear").clicked() {
                    lighting.custom.fill([0, 0, 0]);
                    app.ui.dirty = true;
                }
                if ui.button("Sample preview").clicked() {
                    lighting.custom.keys = app.colors.clone();
                    app.ui.dirty = true;
                }
            });
            ui.add_space(4.0);
            let mut gradient = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Gradient").size(11.0).color(theme::MUTED));
                for slot in 0..4 {
                    if ui
                        .color_edit_button_srgb(&mut lighting.colors[slot])
                        .changed()
                    {
                        app.ui.dirty = true;
                    }
                }
                if ui.button("Fill gradient").clicked() {
                    gradient = true;
                }
            });
            if gradient {
                let stops = [
                    lighting.colors[0],
                    lighting.colors[1],
                    lighting.colors[2],
                    lighting.colors[3],
                ];
                lighting.custom.ensure_len(KEY_COUNT);
                lighting.custom.fill_gradient(layout(), &stops);
                app.ui.dirty = true;
            }
            theme::hint(
                ui,
                "Gradient uses palette slots 1-4 as stops and spreads them left to right.",
            );
            ui.add_space(4.0);
            theme::hint(
                ui,
                "Click or drag on the 65% canvas (Paint tool) to set per-key colours. Clicking a key also selects it.",
            );
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
    if send_static {
        app.ui.lighting_edit_at = None;
        app.upload_custom_layout();
    }
}
