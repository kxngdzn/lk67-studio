//! Screen follow page: capture region, sampling grid and colour shaping.

use egui::RichText;

use crate::app::App;
use crate::model::lighting::{LightingMode, ScreenRegion};
use crate::ui::{theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let index = app.cfg.active_profile;
    let mode = app.cfg.profiles[index].lighting.mode;
    let mut start = false;
    let mut stop = false;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("SCREEN FOLLOW")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        ui.label(
            RichText::new("sample screen zones and map them onto the 65% layout")
                .size(11.5)
                .color(theme::MUTED),
        );
        if mode != LightingMode::Screen {
            if ui
                .add(egui::Button::new("Use screen mode").fill(theme::ACCENT_DIM))
                .clicked()
            {
                app.cfg.profiles[index].lighting.mode = LightingMode::Screen;
                app.ui.apply_lighting = true;
                app.ui.dirty = true;
                app.notify("Lighting mode: screen follow");
            }
        } else {
            theme::badge(ui, "ACTIVE MODE", theme::ACCENT);
        }
        if app.ui.screen_requested {
            theme::badge(ui, "CAPTURING", theme::OK);
        }
    });

    ui.add_space(6.0);
    let (screen_w, screen_h) = {
        let snap = &app.screen_snap;
        (
            if snap.source_width > 0 {
                snap.source_width
            } else {
                1920
            },
            if snap.source_height > 0 {
                snap.source_height
            } else {
                1080
            },
        )
    };

    let mut restart = false;
    ui.columns(2, |cols| {
        theme::card(&mut cols[1], "Sampling grid", |ui| {
            widgets::screen_preview(ui, &app.screen_snap);
            ui.add_space(4.0);
            egui::Grid::new("screen-grid")
                .num_columns(2)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    widgets::kv(
                        ui,
                        "Grid",
                        format!("{} x {}", app.screen_snap.gw, app.screen_snap.gh),
                    );
                    widgets::kv(ui, "Source", format!("{screen_w} x {screen_h} px"));
                    widgets::kv(ui, "Capture rate", format!("{} FPS", app.screen_snap.fps));
                });
            theme::hint(
                ui,
                "Each key samples the cell under its position, so the grid shape sets the look.",
            );
        });
        let right_height = cols[1].min_rect().height();
        theme::card_fill(&mut cols[0], "Capture region", right_height, |ui| {
            let screen = &mut app.cfg.profiles[index].lighting.screen;
            ui.horizontal_wrapped(|ui| {
                if ui.button("Full screen").clicked() {
                    screen.region = ScreenRegion::full();
                    restart = true;
                }
                if ui.button("Left half").clicked() {
                    screen.region = ScreenRegion {
                        x: 0,
                        y: 0,
                        width: screen_w / 2,
                        height: screen_h,
                    };
                    restart = true;
                }
                if ui.button("Right half").clicked() {
                    screen.region = ScreenRegion {
                        x: screen_w / 2,
                        y: 0,
                        width: screen_w / 2,
                        height: screen_h,
                    };
                    restart = true;
                }
                if ui.button("Top half").clicked() {
                    screen.region = ScreenRegion {
                        x: 0,
                        y: 0,
                        width: screen_w,
                        height: screen_h / 2,
                    };
                    restart = true;
                }
                if ui.button("Bottom half").clicked() {
                    screen.region = ScreenRegion {
                        x: 0,
                        y: screen_h / 2,
                        width: screen_w,
                        height: screen_h / 2,
                    };
                    restart = true;
                }
            });
            ui.add_space(4.0);
            egui::Grid::new("screen-region")
                .num_columns(4)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut screen.region.x).range(0..=8000));
                    ui.label("Y");
                    let mut y = screen.region.y;
                    if ui
                        .add(egui::DragValue::new(&mut y).range(0..=8000))
                        .changed()
                    {
                        screen.region.y = y;
                        restart = true;
                    }
                    ui.end_row();
                    ui.label("W");
                    let mut w = screen.region.width;
                    if ui
                        .add(egui::DragValue::new(&mut w).range(0..=8000))
                        .changed()
                    {
                        screen.region.width = w;
                        restart = true;
                    }
                    ui.label("H");
                    let mut h = screen.region.height;
                    if ui
                        .add(egui::DragValue::new(&mut h).range(0..=8000))
                        .changed()
                    {
                        screen.region.height = h;
                        restart = true;
                    }
                    ui.end_row();
                });
            theme::hint(
                ui,
                "Width or height 0 means the whole primary display. Coordinates are screen pixels.",
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("Start capture").fill(theme::ACCENT_DIM))
                    .clicked()
                {
                    start = true;
                }
                if ui.button("Stop capture").clicked() {
                    stop = true;
                }
                ui.label(
                    RichText::new(if app.screen.is_running() {
                        "running"
                    } else {
                        "idle"
                    })
                    .size(11.0)
                    .color(if app.screen.is_running() {
                        theme::OK
                    } else {
                        theme::MUTED
                    }),
                );
            });
            if let Some(err) = &app.screen_snap.error {
                widgets::status_text(ui, err, false);
            }
        });
    });

    ui.add_space(8.0);
    theme::card(ui, "Motion and colour", |ui| {
        let screen = &mut app.cfg.profiles[index].lighting.screen;
        let mut fps = screen.fps as f32;
        if ui
            .add(egui::Slider::new(&mut fps, 5.0..=30.0).text("FPS"))
            .changed()
        {
            screen.fps = fps.round() as u8;
            app.ui.dirty = true;
            restart = true;
        }
        let mut gx = screen.grid_x as f32;
        if ui
            .add(egui::Slider::new(&mut gx, 4.0..=32.0).text("Grid columns"))
            .changed()
        {
            screen.grid_x = gx.round() as u8;
            app.ui.dirty = true;
            restart = true;
        }
        let mut gy = screen.grid_y as f32;
        if ui
            .add(egui::Slider::new(&mut gy, 2.0..=16.0).text("Grid rows"))
            .changed()
        {
            screen.grid_y = gy.round() as u8;
            app.ui.dirty = true;
            restart = true;
        }
        let mut smoothing = screen.smoothing;
        if ui
            .add(
                egui::Slider::new(&mut smoothing, 0.0..=0.95)
                    .text("Smoothing")
                    .step_by(0.01),
            )
            .changed()
        {
            screen.smoothing = smoothing;
            app.ui.dirty = true;
        }
        let mut saturation = screen.saturation;
        if ui
            .add(
                egui::Slider::new(&mut saturation, 0.0..=3.0)
                    .text("Saturation")
                    .step_by(0.05),
            )
            .changed()
        {
            screen.saturation = saturation;
            app.ui.dirty = true;
        }
        let mut gain = screen.gain;
        if ui
            .add(
                egui::Slider::new(&mut gain, 0.2..=2.5)
                    .text("Gain")
                    .step_by(0.05),
            )
            .changed()
        {
            screen.gain = gain;
            app.ui.dirty = true;
        }
        if !app.ui.stream_to_device {
            theme::hint(
                ui,
                "Enable 'Stream colours to keys' (left panel) for live keyboard output.",
            );
        } else {
            theme::hint(
                ui,
                "On the keyboard this mode paints one blended colour for the whole board at the screen's pace; the grid above is the app preview.",
            );
        }
    });

    if restart {
        app.ui.screen_requested = app.screen.is_running();
    }
    if start {
        app.cfg.profiles[index].lighting.mode = LightingMode::Screen;
        app.ui.apply_lighting = true;
        app.ui.screen_requested = true;
        app.ui.dirty = true;
        app.ui.notify("Screen capture starting");
    }
    if stop {
        app.ui.screen_requested = false;
        app.screen.stop();
        app.last_screen_cfg = None;
        app.ui.notify("Screen capture stopped");
    }
}
