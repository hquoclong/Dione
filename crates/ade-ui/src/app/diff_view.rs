//! Session diff panel: renders `GET /session/{id}/diff` results.

use ade_core::Store;

#[derive(serde::Deserialize, Debug)]
struct FileDiff {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    additions: Option<f64>,
    #[serde(default)]
    deletions: Option<f64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    patch: Option<String>,
}

pub fn diff_panel(ui: &mut egui::Ui, store: &Store) {
    ui.add_space(4.0);
    ui.heading("Session diff");
    ui.separator();

    if store.diffs.is_empty() {
        ui.weak("No diff fetched yet — press ⇄ next to the active session.");
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (sid, value) in &store.diffs {
                let diffs: Vec<FileDiff> = if let Ok(list) = serde_json::from_value(value.clone()) {
                    list
                } else if let Ok(one) = serde_json::from_value::<FileDiff>(value.clone()) {
                    vec![one]
                } else {
                    ui.weak(format!("(unparsed diff for {sid})"));
                    continue;
                };

                ui.label(
                    egui::RichText::new(short_sid(sid))
                        .small()
                        .color(egui::Color32::GRAY),
                );
                for d in &diffs {
                    render_file_diff(ui, d);
                }
                ui.add_space(6.0);
            }
        });
}

fn render_file_diff(ui: &mut egui::Ui, d: &FileDiff) {
    let name = d.file.clone().unwrap_or_else(|| "(unknown file)".into());
    let header = format!(
        "{}  +{} −{}{}",
        name,
        d.additions.unwrap_or(0.0),
        d.deletions.unwrap_or(0.0),
        d.status
            .as_ref()
            .map(|s| format!("  [{s}]"))
            .unwrap_or_default(),
    );

    match &d.patch {
        Some(patch) => {
            ui.collapsing(egui::RichText::new(header).monospace(), |ui| {
                ui.set_max_width(ui.available_width());
                for line in patch.lines() {
                    let color = match () {
                        _ if line.starts_with('+') && !line.starts_with("+++") => {
                            egui::Color32::from_rgb(0x3f, 0xd1, 0x7c)
                        }
                        _ if line.starts_with('-') && !line.starts_with("---") => {
                            egui::Color32::from_rgb(0xe5, 0x48, 0x4d)
                        }
                        _ if line.starts_with("@@") => egui::Color32::from_rgb(0xb1, 0x8c, 0xf0),
                        _ => egui::Color32::GRAY,
                    };
                    ui.monospace(egui::RichText::new(line).color(color));
                }
            });
        }
        None => {
            ui.monospace(header);
        }
    }
}

fn short_sid(sid: &str) -> String {
    sid.chars().take(12).collect()
}
