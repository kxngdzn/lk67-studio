//! Settings page: runtime options and profile maintenance.

use egui::RichText;

use crate::app::App;
use crate::driver::KeyboardDriver;
use crate::model::keycode::KeyCode;
use crate::model::store;
use crate::ui::{theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let index = app.cfg.active_profile;
    let mut changed = false;
    let mut reset_profile = false;

    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Software remapping", |ui| {
            changed |= ui
                .checkbox(
                    &mut app.cfg.settings.hook_enabled,
                    "Enable the low-level keyboard hook",
                )
                .changed();
            let remap_toggle = ui
                .checkbox(
                    &mut app.cfg.settings.remap_enabled,
                    "Apply remaps and combos in software",
                )
                .changed();
            if remap_toggle {
                app.ui.notify(if app.cfg.settings.remap_enabled {
                    "Software remap on: onboard keymap writes are disabled"
                } else {
                    "Software remap off: use Driver > Write keymap to persist keys"
                });
            }
            changed |= remap_toggle;
            changed |= ui
                .checkbox(
                    &mut app.cfg.settings.hotkeys_enabled,
                    "Ctrl+Alt+1..4 switches profiles",
                )
                .changed();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("FN key VK");
                let mut key = KeyCode(app.cfg.settings.fn_key_vk);
                if widgets::key_picker(ui, "settings-fn-key", &mut key) {
                    app.cfg.settings.fn_key_vk = key.vk();
                    changed = true;
                }
                theme::hint(ui, "placeholder; most LK67s handle FN in firmware");
            });
            ui.add_space(4.0);
            theme::hint(
                ui,
                "Software remapping applies assigned keys immediately while the app runs and is what makes combos and macros work. While it is on, the app never writes the onboard keymap, so each key is remapped exactly once. Turn it off to use onboard persistence (Driver > Write keymap).",
            );
            theme::hint(
                ui,
                "Old swaps written by earlier builds are cleared with Driver → Clear onboard remaps.",
            );
            theme::hint(
                ui,
                "The hook is installed once at startup; disabling it here makes it pass everything through.",
            );
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "App behaviour", left_height, |ui| {
            changed |= ui
                .checkbox(
                    &mut app.cfg.settings.sync_profile_to_hardware,
                    "Ask the firmware to switch profile slots",
                )
                .changed();
            let mut fps = app.cfg.settings.stream_fps as f32;
            if ui
                .add(egui::Slider::new(&mut fps, 20.0..=60.0).text("Live stream rate"))
                .changed()
            {
                app.cfg.settings.stream_fps = fps.round() as u8;
                changed = true;
            }
            ui.add_space(4.0);
            theme::hint(
                ui,
                "The live stream rate feeds the keyboard's own music (0x0D bands) and screen-colour (0x0E) modes; hardware effects are rendered on the keyboard itself.",
            );
        });
    });

    ui.add_space(8.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Active profile", |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.cfg.profiles[index].name)
                            .desired_width(200.0),
                    )
                    .changed()
                {
                    changed = true;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Reset lighting").clicked() {
                    app.cfg.profiles[index].lighting = crate::model::lighting::Lighting::for_slot(index);
                    changed = true;
                    app.notify("Lighting reset to profile defaults");
                }
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            changed |= ui
                .checkbox(
                    &mut app.ui.confirm_reset,
                    "I understand this erases remaps, macros and lighting in this slot",
                )
                .changed();
            if ui
                .add_enabled(
                    app.ui.confirm_reset,
                    egui::Button::new("Reset this profile completely").fill(theme::DANGER),
                )
                .clicked()
            {
                reset_profile = true;
            }
            theme::hint(
                ui,
                "The other three profile slots are not touched.",
            );
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "Storage", left_height, |ui| {
            let path = store::config_path();
            ui.label(
                RichText::new(path.display().to_string())
                    .size(11.0)
                    .monospace(),
            );
            ui.horizontal(|ui| {
                let exists = store::config_exists();
                if exists {
                    theme::badge(ui, "ON DISK", theme::ACCENT_DIM);
                } else {
                    theme::badge(ui, "NOT WRITTEN YET", theme::WARN);
                }
                if let Some(saved) = app.saved_at {
                    theme::hint(ui, format!("saved {:.0}s ago", saved.elapsed().as_secs_f32()));
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Copy path").clicked() {
                    ui.ctx().copy_text(path.display().to_string());
                    app.notify("Config path copied");
                }
            });
            ui.add_space(4.0);
            theme::hint(
                ui,
                "Profiles are saved as pretty JSON with a temp-file + rename so a crash cannot corrupt them.",
            );
            theme::hint(
                ui,
                format!(
                    "{} of 4 profile slots in use, active profile {}.",
                    app.cfg.profiles.len(),
                    index + 1
                ),
            );
        });
    });

    ui.add_space(8.0);
    theme::card(ui, "About / limitations", |ui| {
        ui.label(
            RichText::new(concat!("LK67 Studio ", env!("CARGO_PKG_VERSION")))
                .size(13.0)
                .strong(),
        );
        theme::hint(
            ui,
            "HID report IDs, matrix slots and packet layout are implemented for the yc500 firmware in src/driver/lk67.rs. Dry-run stays the default until you select your board.",
        );
        theme::hint(
            ui,
            "Mapping, macros, effects and audio/screen analysis all run locally. Nothing is uploaded.",
        );
        theme::hint(
            ui,
            "Profile switching: tabs, Ctrl+Alt+1..4 hotkeys, the tray menu and the firmware slot packet. Closing the window hides it to the tray; Quit is in the tray menu.",
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} keys, {} macros in this profile",
                    crate::model::layout::KEY_COUNT,
                    app.cfg.profiles[index].macros.len()
                ))
                .size(11.0)
                .color(theme::MUTED),
            );
        });
    });

    if changed {
        app.ui.dirty = true;
    }
    if app.driver.is_dry_run() != app.cfg.settings.dry_run {
        app.driver.set_dry_run(app.cfg.settings.dry_run);
    }
    if reset_profile {
        app.cfg.profiles[index].reset_all(index);
        app.ui.selected = None;
        app.ui.selected_macro = None;
        app.ui.confirm_reset = false;
        app.ui.dirty = true;
        app.notify("Profile reset to defaults");
    }
}
