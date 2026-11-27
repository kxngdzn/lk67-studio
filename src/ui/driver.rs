//! Driver page: HID discovery, placeholder protocol knobs and the packet inspector.

use egui::RichText;

use crate::app::App;
use crate::driver::lk67::{self, DriverConfig, ReportTransport};
use crate::driver::{DeviceStatus, KeyboardDriver};
use crate::ui::{theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let mut cfg = app.cfg.driver.clone();
    let mut rescan = false;
    let mut refresh = false;
    let mut clear_log = false;
    let mut copy_log = false;
    let mut use_device: Option<usize> = None;
    let mut test_packet = false;
    let mut apply_keymap = false;
    let mut restore_stock = false;
    let mut select_slot = false;

    let status = app.driver.status().clone();
    let writes = app.driver.writes();
    let last_error = app.driver.last_error().map(str::to_string);
    let devices = app.devices.clone();

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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{writes} writes"))
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Rescan / reconnect").clicked() {
                rescan = true;
            }
            if ui.button("Refresh HID list").clicked() {
                refresh = true;
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
            if ui.button("Send test packet").clicked() {
                test_packet = true;
            }
        });
        if let Some(err) = &last_error {
            widgets::status_text(ui, err, false);
        }
    });

    ui.add_space(8.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Protocol (ROYUAN yc500)", |ui| {
            let mut vid = cfg.vid as u32;
            let mut pid = cfg.pid as u32;
            egui::Grid::new("driver-ids")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    ui.label("VID");
                    if ui
                        .add(egui::DragValue::new(&mut vid).range(0..=0xFFFF))
                        .changed()
                    {
                        cfg.vid = vid as u16;
                    }
                    ui.end_row();
                    ui.label("PID");
                    if ui
                        .add(egui::DragValue::new(&mut pid).range(0..=0xFFFF))
                        .changed()
                    {
                        cfg.pid = pid as u16;
                    }
                    ui.end_row();
                });
            theme::hint(
                ui,
                format!("current match: {:04X}:{:04X}", cfg.vid, cfg.pid),
            );
            let mut interface = cfg.interface;
            ui.horizontal(|ui| {
                ui.label("Interface");
                ui.add(egui::DragValue::new(&mut interface).range(-1..=32));
                theme::hint(ui, "-1 = any");
            });
            cfg.interface = interface;

            let mut report_len = cfg.report_len as u32;
            if ui
                .add(egui::Slider::new(&mut report_len, 65..=256).text("Report length"))
                .changed()
            {
                cfg.report_len = report_len as usize;
            }
            let mut report_id = cfg.report_id as u32;
            ui.horizontal(|ui| {
                ui.label("Report ID");
                if ui
                    .add(egui::DragValue::new(&mut report_id).range(0..=0xFF))
                    .changed()
                {
                    cfg.report_id = report_id as u8;
                }
                theme::hint(ui, "LK67: 0 (opcode is data byte 0)");
            });
            let mut slots = cfg.profile_slots as u32;
            if ui
                .add(
                    egui::DragValue::new(&mut slots)
                        .range(1..=4)
                        .prefix("profile slots "),
                )
                .changed()
            {
                cfg.profile_slots = slots as u8;
            }
            theme::hint(ui, "Checksum: 0xFF - sum, byte 7 (byte 8 for LED params).");
            ui.horizontal(|ui| {
                ui.label("Transport");
                egui::ComboBox::from_id_salt("report-transport")
                    .selected_text(cfg.transport.label())
                    .width(225.0)
                    .show_ui(ui, |ui| {
                        for transport in ReportTransport::ALL {
                            if ui
                                .selectable_value(
                                    &mut cfg.transport,
                                    transport,
                                    transport.label(),
                                )
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    });
            });
            theme::hint(
                ui,
                "LK67 vendor interface 2 exposes a 65-byte feature report and no output report: keep Feature report.",
            );
            ui.add_space(2.0);
            egui::Grid::new("driver-report-ids")
                .num_columns(2)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    let mut keymap_id = cfg.opcodes.keymap as u32;
                    ui.label("Keymap");
                    if ui
                        .add(egui::DragValue::new(&mut keymap_id).range(0..=0xFF))
                        .changed()
                    {
                        cfg.opcodes.keymap = keymap_id as u8;
                    }
                    ui.end_row();
                    let mut colors_id = cfg.opcodes.key_colors as u32;
                    ui.label("Key colours");
                    if ui
                        .add(egui::DragValue::new(&mut colors_id).range(0..=0xFF))
                        .changed()
                    {
                        cfg.opcodes.key_colors = colors_id as u8;
                    }
                    ui.end_row();
                    let mut effect_id = cfg.opcodes.effect as u32;
                    ui.label("Effect");
                    if ui
                        .add(egui::DragValue::new(&mut effect_id).range(0..=0xFF))
                        .changed()
                    {
                        cfg.opcodes.effect = effect_id as u8;
                    }
                    ui.end_row();
                    let mut profile_id = cfg.opcodes.profile as u32;
                    ui.label("Profile");
                    if ui
                        .add(egui::DragValue::new(&mut profile_id).range(0..=0xFF))
                        .changed()
                    {
                        cfg.opcodes.profile = profile_id as u8;
                    }
                    ui.end_row();
                    let mut fn_id = cfg.opcodes.fn_keymap as u32;
                    ui.label("Fn keymap");
                    if ui
                        .add(egui::DragValue::new(&mut fn_id).range(0..=0xFF))
                        .changed()
                    {
                        cfg.opcodes.fn_keymap = fn_id as u8;
                    }
                    ui.end_row();
                });
            theme::hint(ui, "These are data-byte opcodes, not report IDs. LK67 defaults: keymap 0x13, Fn 0x15, colours 0x0C, effect 0x07, profile 0x05.");
            if ui.button("Reset to defaults").clicked() {
                cfg = DriverConfig::default();
            }
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "HID discovery", left_height, |ui| {
            ui.horizontal(|ui| {
                theme::hint(
                    ui,
                    "Pick a vendor collection (usage 0xFF00+). Keyboard/mouse collections are read-only under Windows.",
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Refresh").clicked() {
                        refresh = true;
                    }
                    if ui
                        .small_button("Use vendor")
                        .on_hover_text("Open the first vendor collection for this VID/PID")
                        .clicked()
                        && let Some(index) = devices
                            .iter()
                            .position(|r| r.is_match && r.is_vendor() && r.is_writable())
                            .or_else(|| {
                                devices
                                    .iter()
                                    .position(|r| r.is_vendor() && r.is_writable())
                            })
                            .or_else(|| devices.iter().position(|r| r.is_vendor()))
                    {
                        use_device = Some(index);
                    }
                });
            });
            ui.add_space(2.0);
            if devices.is_empty() {
                theme::hint(ui, "No HID devices enumerated.");
            }
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, record) in devices.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if record.is_match {
                                theme::badge(ui, "MATCH", theme::ACCENT);
                            }
                            if record.is_vendor() && record.is_writable() {
                                theme::badge(ui, "VENDOR", theme::OK);
                            } else if record.is_vendor() {
                                theme::badge(ui, "INPUT-ONLY", theme::MUTED);
                            } else if record.is_write_restricted() {
                                theme::badge(ui, "READ-ONLY", theme::WARN);
                            }
                            ui.label(RichText::new(record.title()).size(12.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Use").clicked() {
                                        use_device = Some(index);
                                    }
                                },
                            );
                        });
                        theme::hint(ui, &widgets::truncate(&record.subtitle(), 78));
                        ui.add_space(2.0);
                    }
                });
            theme::hint(
                ui,
                "Read-only collections cannot receive RGB packets. 'Use' copies the VID/PID, interface and exact usage page.",
            );
        });
    });

    ui.add_space(8.0);
    let entries: Vec<_> = app.driver.log().iter().rev().take(60).cloned().collect();
    let total_packets = app.driver.log().len();
    theme::card(ui, "Packet log", |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{total_packets} packets"))
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.checkbox(&mut app.ui.packet_hex, "Hex");
            if ui.button("Copy").clicked() {
                copy_log = true;
            }
            if ui.button("Clear").clicked() {
                clear_log = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::hint(ui, "Newest first. Streaming modes fill this quickly.");
            });
        });
        ui.add_space(2.0);
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("packet-log")
                    .num_columns(4)
                    .striped(true)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("AGE").size(10.0).color(theme::MUTED));
                        ui.label(RichText::new("REPORT").size(10.0).color(theme::MUTED));
                        ui.label(RichText::new("OK").size(10.0).color(theme::MUTED));
                        ui.label(RichText::new("BYTES").size(10.0).color(theme::MUTED));
                        ui.end_row();
                        if entries.is_empty() {
                            ui.label("-");
                            ui.label("No packets yet. Switch effects or press Apply.");
                            ui.label("");
                            ui.label("");
                            ui.end_row();
                        }
                        for entry in &entries {
                            ui.label(format!("{:.1}s", entry.at.elapsed().as_secs_f32()));
                            ui.label(entry.label.clone());
                            ui.label(if entry.ok { "yes" } else { "no" });
                            if app.ui.packet_hex {
                                let hex = lk67::hex_dump(&entry.bytes);
                                ui.label(
                                    RichText::new(widgets::truncate(&hex, 58))
                                        .size(10.0)
                                        .monospace(),
                                );
                            } else {
                                ui.label(format!("{} bytes - {}", entry.bytes.len(), entry.note));
                            }
                            ui.end_row();
                        }
                    });
            });
    });

    if refresh {
        app.refresh_devices();
    }
    if rescan {
        app.driver.rescan();
        app.devices = app.driver.discover();
        let label = app.driver.status().label();
        app.notify(label);
    }
    if clear_log {
        app.driver.clear_log();
        app.notify("Packet log cleared");
    }
    if copy_log {
        let mut text = String::new();
        for entry in app.driver.log() {
            text.push_str(&format!(
                "{:>8.1}s  {:<28} {}  {}\n",
                entry.at.elapsed().as_secs_f32(),
                entry.label,
                if entry.ok { "OK " } else { "ERR" },
                lk67::hex_dump(&entry.bytes)
            ));
        }
        ui.ctx().copy_text(text);
        app.notify("Packet log copied to clipboard");
    }
    if let Some(device_index) = use_device {
        if let Some(record) = app.devices.get(device_index).cloned() {
            if !record.is_writable() {
                app.notify(
                    "That collection exposes no writable report. Choose a row marked VENDOR (usage 0xFFFF/0002 has the 65-byte feature report).",
                );
            } else {
                cfg.vid = record.vendor_id;
                cfg.pid = record.product_id;
                cfg.interface = record.interface;
                cfg.usage_page = record.usage_page;
                cfg.usage = record.usage;
                cfg.transport = if record.feature_len > 0 {
                    ReportTransport::FeatureReport
                } else {
                    ReportTransport::OutputReport
                };
                app.cfg.settings.dry_run = false;
                app.driver.set_dry_run(false);
                app.cfg.driver = cfg.clone();
                app.driver.set_config(cfg.clone());
                match app.driver.open_path(&record.path) {
                    Ok(()) => app.notify(format!(
                        "Opened {} (usage {:04X}/{:04X})",
                        record.title(),
                        record.usage_page,
                        record.usage
                    )),
                    Err(err) => app.notify(format!("Open failed: {err}")),
                }
                app.devices = app.driver.discover();
                app.ui.dirty = true;
            }
        }
    }
    if test_packet {
        let lighting = app.cfg.profile().lighting.clone();
        let report = lk67::build_effect_report(&app.cfg.driver, &lighting);
        match app.driver.send_report("Test effect packet", &report) {
            Ok(()) => app.notify("Test packet sent (or logged in dry-run)"),
            Err(err) => app.notify(format!("Test packet failed: {err}")),
        }
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
    if cfg != app.cfg.driver {
        app.cfg.driver = cfg.clone();
        app.driver.set_config(cfg);
        app.ui.notify("Driver configuration updated");
        app.ui.dirty = true;
    }
}
