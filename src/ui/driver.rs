//! Driver page: connection status and onboard keymap maintenance.

use egui::RichText;

use crate::app::App;
use crate::driver::{DeviceStatus, KeyboardDriver};
use crate::ui::{theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let mut rescan = false;
    let mut apply_keymap = false;
    let mut restore_stock = false;
    let mut select_slot = false;

    let status = app.driver.status().clone();
    let last_error = app.driver.last_error().map(str::to_string);

    theme::card(ui, "Connection", |ui| {
        ui.horizontal_wrapped(|ui| {
            let (color, badge) = match &status {
                DeviceStatus::Connected { .. } => (theme::OK, "CONNECTED"),
                DeviceStatus::Simulation => (theme::WARN, "DRY-RUN"),
                DeviceStatus::Disconnected => (theme::DANGER, "OFFLINE"),
            };
            theme::badge(ui, badge, color);
            ui.label(RichText::new(status.label()).size(13.0).strong());
            if let DeviceStatus::Connected { path, .. } = &status {
                theme::hint(ui, widgets::truncate(path, 60));
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Rescan / reconnect").clicked() {
                rescan = true;
            }
            if ui
                .button("Write keymap")
                .on_hover_text(
                    "Onboard persistence for simple keys. Disabled while software remapping is on so a key is never remapped twice.",
                )
                .clicked()
            {
                apply_keymap = true;
            }
            if ui
                .button("Clear onboard remaps")
                .on_hover_text(
                    "Read all four firmware profile slots and write stock entries where they differ. Removes swaps written by earlier builds.",
                )
                .clicked()
            {
                restore_stock = true;
            }
            if ui.button("Select profile slot").clicked() {
                select_slot = true;
            }
        });
        if let Some(err) = &last_error {
            widgets::status_text(ui, err, false);
        }
        theme::hint(
            ui,
            "The keyboard is matched automatically by VID/PID. Dry-run is controlled from the top bar.",
        );
    });

    if rescan {
        app.driver.rescan();
        let label = app.driver.status().label();
        app.notify(label);
    }
    if apply_keymap {
        app.request_keymap_write();
    }
    if restore_stock {
        app.clear_onboard_keymap();
    }
    if select_slot {
        if app.cfg.settings.remap_enabled {
            app.notify(
                "Software remap is active: firmware profile slots can contain onboard remaps, so the slot was not switched.",
            );
        } else {
            let slot = app.cfg.active_profile as u8;
            match app.driver.select_profile(slot) {
                Ok(()) => app.notify(format!("Firmware profile slot {}", slot + 1)),
                Err(err) => app.notify(format!("Profile select failed: {err}")),
            }
        }
    }

    if app.driver.is_dry_run() != app.cfg.settings.dry_run {
        app.driver.set_dry_run(app.cfg.settings.dry_run);
        app.ui.dirty = true;
    }
}
