//! Raw JSON inspector for a selected message part.

use super::RightTab;
use ade_core::Store;
use opencode_codes::protocol_generated::types::Part;

pub fn right_panel(
    ui: &mut egui::Ui,
    tab: &mut RightTab,
    store: &Store,
    selected: &mut Option<Part>,
) {
    egui::Panel::right("inspector")
        .resizable(true)
        .default_size(380.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(tab, RightTab::Context, "Context");
                ui.selectable_value(tab, RightTab::Inspector, "Inspector");
                ui.selectable_value(tab, RightTab::Diff, "Diff");
            });
            ui.separator();

            match *tab {
                RightTab::Context => super::context_panel::context_panel(ui, store),
                RightTab::Inspector => inspector_panel(ui, selected),
                RightTab::Diff => super::diff_view::diff_panel(ui, store),
            }
        });
}

fn inspector_panel(ui: &mut egui::Ui, selected: &mut Option<Part>) {
    ui.add_space(4.0);
    let Some(part) = selected.clone() else {
        ui.weak("Click {} on any part of the timeline to inspect its raw JSON.");
        return;
    };

    ui.horizontal(|ui| {
        ui.heading("Raw part");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("copy").clicked()
                && let Ok(json) = serde_json::to_string_pretty(&part)
            {
                ui.ctx().copy_text(json);
            }
            if ui.button("×").on_hover_text("Clear selection").clicked() {
                *selected = None;
            }
        });
    });
    ui.separator();

    let Ok(value) = serde_json::to_value(&part) else {
        return;
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            json_tree(ui, &value, "$", true);
        });
}

/// Recursive JSON viewer with collapsible objects/arrays.
fn json_tree(ui: &mut egui::Ui, value: &serde_json::Value, path: &str, open_by_default: bool) {
    match value {
        serde_json::Value::Object(map) if map.is_empty() => {
            kv_row(ui, path, "{}");
        }
        serde_json::Value::Object(_) => {
            egui::CollapsingHeader::new(egui::RichText::new(path).monospace())
                .default_open(open_by_default)
                .show(ui, |ui| {
                    for (k, v) in value.as_object().unwrap() {
                        json_tree(ui, v, k, false);
                    }
                });
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                kv_row(ui, path, "[]");
                return;
            }
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{path} [{}]", items.len())).monospace(),
            )
            .default_open(open_by_default)
            .show(ui, |ui| {
                for (i, v) in items.iter().enumerate() {
                    json_tree(ui, v, &format!("[{i}]"), false);
                }
            });
        }
        serde_json::Value::String(s) => {
            if s.chars().count() > 80 {
                kv_collapsible(ui, path, s);
            } else {
                kv_row(ui, path, &format!("\"{s}\""));
            }
        }
        other => kv_row(ui, path, &other.to_string()),
    }
}

fn kv_row(ui: &mut egui::Ui, key: &str, val: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.monospace(egui::RichText::new(key).color(egui::Color32::from_rgb(0x5e, 0xb1, 0xf0)));
        ui.monospace(val);
    });
}

fn kv_collapsible(ui: &mut egui::Ui, key: &str, long: &str) {
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("{key} ({} chars)", long.chars().count())).monospace(),
    )
    .show(ui, |ui| {
        ui.set_max_width(ui.available_width());
        ui.monospace(long);
    });
}
