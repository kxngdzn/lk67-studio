//! Music reactive page: capture source, FFT meter and response shaping.

use egui::RichText;

use crate::app::App;
use crate::model::lighting::{AudioSource, LightingMode, MusicVisual};
use crate::ui::{theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let index = app.cfg.active_profile;
    let mode = app.cfg.profiles[index].lighting.mode;
    let mut start = false;
    let mut stop = false;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("MUSIC REACTIVE")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        ui.label(
            RichText::new("WASAPI loopback or microphone -> FFT -> per-key colours")
                .size(11.5)
                .color(theme::MUTED),
        );
        if mode != LightingMode::Music {
            if ui
                .add(egui::Button::new("Use music mode").fill(theme::ACCENT_DIM))
                .clicked()
            {
                app.cfg.profiles[index].lighting.mode = LightingMode::Music;
                app.ui.apply_lighting = true;
                app.ui.dirty = true;
                app.notify("Lighting mode: music reactive");
            }
        } else {
            theme::badge(ui, "ACTIVE MODE", theme::ACCENT);
        }
        if app.ui.audio_requested {
            theme::badge(ui, "CAPTURING", theme::OK);
        }
    });

    ui.add_space(6.0);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], "Audio input", |ui| {
            let music = &mut app.cfg.profiles[index].lighting.music;
            ui.horizontal(|ui| {
                ui.label("Source");
                egui::ComboBox::from_id_salt("audio-source")
                    .selected_text(music.source.label())
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for source in [AudioSource::Loopback, AudioSource::Microphone] {
                            if ui
                                .selectable_value(&mut music.source, source, source.label())
                                .clicked()
                            {
                                app.ui.dirty = true;
                                ui.close();
                            }
                        }
                    });
            });
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
                    RichText::new(if app.audio.is_running() {
                        "running"
                    } else {
                        "idle"
                    })
                    .size(11.0)
                    .color(if app.audio.is_running() {
                        theme::OK
                    } else {
                        theme::MUTED
                    }),
                );
            });
            ui.add_space(4.0);
            if let Some(err) = &app.audio_snap.error {
                widgets::status_text(ui, err, false);
            }
            theme::hint(
                ui,
                "Loopback captures whatever the system is playing; microphone uses the default input.",
            );
            theme::hint(
                ui,
                "No data until something is actually playing (or you speak).",
            );
            if !app.ui.stream_to_device {
                ui.add_space(4.0);
                theme::hint(
                    ui,
                    "Enable 'Stream colours to keys' (left panel) so the keyboard follows.",
                );
            } else {
                theme::hint(
                    ui,
                    "The keyboard draws its own bars from a live band stream, so it reacts without flash writes. Start capture to feed it.",
                );
            }
        });

        let left_height = cols[0].min_rect().height();
        theme::card_fill(&mut cols[1], "Spectrum (16 bands)", left_height, |ui| {
            widgets::spectrum_meter(
                ui,
                &app.audio_snap.bands,
                app.audio_snap.level,
                app.audio_snap.beat,
            );
            ui.add_space(4.0);
            egui::Grid::new("music-stats")
                .num_columns(2)
                .spacing([12.0, 3.0])
                .show(ui, |ui| {
                    widgets::kv(
                        ui,
                        "Level",
                        format!("{:.2}", app.audio_snap.level),
                    );
                    widgets::kv(
                        ui,
                        "Peak band",
                        format!("{:.2}", app.audio_snap.peak),
                    );
                    widgets::kv(
                        ui,
                        "Sample rate",
                        format!("{} Hz", app.audio_snap.sample_rate),
                    );
                    widgets::kv(
                        ui,
                        "Beat",
                        format!("{:.2}", app.audio_snap.beat),
                    );
                });
        });
    });

    ui.add_space(8.0);
    theme::card(ui, "Response", |ui| {
        let music = &mut app.cfg.profiles[index].lighting.music;
        ui.horizontal(|ui| {
            ui.label("Visual");
            egui::ComboBox::from_id_salt("music-visual")
                .selected_text(music.visual.label())
                .width(150.0)
                .show_ui(ui, |ui| {
                    for visual in MusicVisual::ALL {
                        if ui
                            .selectable_value(&mut music.visual, visual, visual.label())
                            .clicked()
                        {
                            app.ui.dirty = true;
                            ui.close();
                        }
                    }
                });
            ui.checkbox(&mut music.show_meter, "Show meter");
        });
        ui.add_space(2.0);
        let mut sensitivity = music.sensitivity;
        if ui
            .add(
                egui::Slider::new(&mut sensitivity, 0.2..=4.0)
                    .text("Sensitivity")
                    .step_by(0.05),
            )
            .changed()
        {
            music.sensitivity = sensitivity;
            app.ui.dirty = true;
        }
        let mut gain = music.gain;
        if ui
            .add(
                egui::Slider::new(&mut gain, 0.2..=3.0)
                    .text("Gain")
                    .step_by(0.05),
            )
            .changed()
        {
            music.gain = gain;
            app.ui.dirty = true;
        }
        let mut smoothing = music.smoothing;
        if ui
            .add(
                egui::Slider::new(&mut smoothing, 0.0..=0.95)
                    .text("Smoothing")
                    .step_by(0.01),
            )
            .changed()
        {
            music.smoothing = smoothing;
            app.ui.dirty = true;
        }
        let mut color_speed = music.color_speed;
        if ui
            .add(
                egui::Slider::new(&mut color_speed, 0.0..=2.0)
                    .text("Colour drift")
                    .step_by(0.05),
            )
            .changed()
        {
            music.color_speed = color_speed;
            app.ui.dirty = true;
        }
        let mut beat_boost = music.beat_boost;
        if ui
            .add(
                egui::Slider::new(&mut beat_boost, 0.0..=2.0)
                    .text("Beat boost")
                    .step_by(0.05),
            )
            .changed()
        {
            music.beat_boost = beat_boost;
            app.ui.dirty = true;
        }
        theme::hint(
            ui,
            "Smoothing follows the signal more slowly; sensitivity scales both the bands and beat detection.",
        );
    });

    if start {
        app.cfg.profiles[index].lighting.mode = LightingMode::Music;
        app.ui.apply_lighting = true;
        app.ui.audio_requested = true;
        app.ui.dirty = true;
        let source = app.cfg.profiles[index].lighting.music.source;
        app.audio.start(source);
        app.ui
            .notify(format!("Audio capture started: {}", source.label()));
    }
    if stop {
        app.ui.audio_requested = false;
        app.audio.stop();
        app.ui.notify("Audio capture stopped");
    }
}
