//! Key tester: live switch activity and heatmap from real key presses.

use egui::RichText;

use crate::app::App;
use crate::model::layout::layout;
use crate::ui::{theme, widgets};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    let mut reset = false;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("KEY TESTER")
                .size(10.0)
                .strong()
                .color(theme::MUTED),
        );
        theme::badge(
            ui,
            &format!("{} PRESSES", app.ui.tester_total),
            theme::ACCENT_DIM,
        );
        ui.checkbox(&mut app.ui.tester_heatmap, "Heatmap");
        if ui.button("Reset counters").clicked() {
            reset = true;
        }
        ui.separator();
        theme::hint(
            ui,
            "Type on the keyboard: presses light up the canvas and the table below.",
        );
    });

    ui.add_space(6.0);
    let used_keys = app.ui.tester_counts.iter().filter(|c| **c > 0).count();
    let average_hold = {
        let values: Vec<u32> = app
            .ui
            .tester_hold_ms
            .iter()
            .copied()
            .filter(|ms| *ms > 0)
            .collect();
        if values.is_empty() {
            0
        } else {
            values.iter().sum::<u32>() / values.len() as u32
        }
    };

    ui.columns(3, |cols| {
        theme::card(&mut cols[2], "Notes", |ui| {
            theme::hint(
                ui,
                "Presses are counted from the Windows keyboard hook, so keys typed into other apps show up too.",
            );
            theme::hint(
                ui,
                "While this window has focus the app also reads its own key input, so the tester keeps working even if the hook could not start.",
            );
            if let Some(err) = &app.hook.error {
                theme::hint(ui, format!("Hook unavailable: {err}"));
            }
            theme::hint(ui, "Counters reset when the app restarts.");
        });
        let notes_height = cols[2].min_rect().height();
        theme::card_fill(&mut cols[0], "Session", notes_height, |ui| {
            egui::Grid::new("tester-session")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    widgets::kv(ui, "Total presses", app.ui.tester_total.to_string());
                    widgets::kv(ui, "Keys used", format!("{used_keys} / 68"));
                    widgets::kv(ui, "Average hold", format!("{average_hold} ms"));
                    widgets::kv(ui, "Layer", app.ui.layer.label());
                });
        });
        theme::card_fill(&mut cols[1], "Selected key", notes_height, |ui| {
            let Some(index) = app.ui.selected else {
                theme::hint(ui, "Click a keycap above.");
                return;
            };
            let key = layout().key(index);
            egui::Grid::new("tester-key-details")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    widgets::kv(ui, "Key", format!("{} ({})", layout().label(index), key.code));
                    widgets::kv(ui, "Presses", app.ui.tester_counts[index].to_string());
                    widgets::kv(ui, "Last hold", format!("{} ms", app.ui.tester_hold_ms[index]));
                    widgets::kv(
                        ui,
                        "Last press",
                        app.ui.tester_last[index]
                            .map(|at| format!("{:.1}s ago", at.elapsed().as_secs_f32()))
                            .unwrap_or_else(|| "never".into()),
                    );
                });
        });
    });

    ui.add_space(8.0);
    theme::card(ui, "Per-key statistics", |ui| {
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("tester-table")
                    .num_columns(4)
                    .striped(true)
                    .spacing([18.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("KEY").size(10.0).color(theme::MUTED));
                        ui.label(RichText::new("PRESSES").size(10.0).color(theme::MUTED));
                        ui.label(RichText::new("LAST").size(10.0).color(theme::MUTED));
                        ui.label(RichText::new("HOLD").size(10.0).color(theme::MUTED));
                        ui.end_row();
                        let mut indices: Vec<usize> =
                            (0..68).filter(|i| app.ui.tester_counts[*i] > 0).collect();
                        indices.sort_by_key(|i| std::cmp::Reverse(app.ui.tester_counts[*i]));
                        if indices.is_empty() {
                            ui.label("No presses recorded yet.");
                            ui.end_row();
                        }
                        for index in indices.into_iter().take(40) {
                            ui.label(layout().label(index));
                            ui.label(app.ui.tester_counts[index].to_string());
                            ui.label(
                                app.ui.tester_last[index]
                                    .map(|at| format!("{:.1}s ago", at.elapsed().as_secs_f32()))
                                    .unwrap_or_else(|| "-".into()),
                            );
                            ui.label(format!("{} ms", app.ui.tester_hold_ms[index]));
                            ui.end_row();
                        }
                    });
            });
    });

    if reset {
        app.ui.reset_tester();
        app.notify("Tester counters cleared");
    }
}
