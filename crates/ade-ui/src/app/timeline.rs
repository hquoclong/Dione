//! Live conversation timeline for the active session.

use ade_core::state::{part_id, MessageEntry};
use opencode_codes::protocol_generated::types::SessionStatus;
use opencode_codes::protocol_generated::types::{Message, Part, ToolState};

const GREEN: egui::Color32 = egui::Color32::from_rgb(0x3f, 0xd1, 0x7c);
const RED: egui::Color32 = egui::Color32::from_rgb(0xe5, 0x48, 0x4d);
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xf5, 0xc5, 0x42);
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x8b, 0x8b, 0x93);

pub fn timeline(
    ui: &mut egui::Ui,
    store: &ade_core::Store,
    _session_id: &str,
    selected: &mut Option<Part>,
    md_cache: &mut egui_commonmark::CommonMarkCache,
) {
    let Some(messages) = store.active_messages() else {
        return;
    };
    let status = store
        .active_session
        .as_ref()
        .and_then(|id| store.statuses.get(id))
        .cloned();

    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            if messages.is_empty() {
                ui.weak("No messages yet — say something below.");
            }
            for m in messages {
                render_message(ui, m, selected, md_cache);
            }

            // Busy indicator pinned to the end of the stream.
            match status {
                Some(SessionStatus::Busy) | Some(SessionStatus::Retry { .. }) => {
                    ui.add_space(6.0);
                    ui.colored_label(GREEN, "▌ agent working…");
                }
                _ => {}
            }
            ui.add_space(12.0);
        });
}

fn render_message(
    ui: &mut egui::Ui,
    entry: &MessageEntry,
    selected: &mut Option<Part>,
    md_cache: &mut egui_commonmark::CommonMarkCache,
) {
    match &entry.info {
        Message::User(u) => {
            let text = entry
                .parts
                .iter()
                .filter_map(|p| match p {
                    Part::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                return;
            }
            ui.add_space(10.0);
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(0x24, 0x28, 0x38))
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    ui.label(egui::RichText::new("YOU").small().color(MUTED));
                    ui.label(text);
                });
            let _ = u;
        }
        Message::Assistant(a) => {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("AGENT · {}", a.model_id))
                        .small()
                        .color(MUTED),
                );
                if a.cost > 0.0 {
                    ui.label(
                        egui::RichText::new(format!("${:.4}", a.cost))
                            .small()
                            .color(MUTED),
                    );
                }
            });
            for part in &entry.parts {
                render_part(ui, part, selected, md_cache);
            }
        }
    }
}

fn render_part(
    ui: &mut egui::Ui,
    part: &Part,
    selected: &mut Option<Part>,
    md_cache: &mut egui_commonmark::CommonMarkCache,
) {
    match part {
        Part::StepStart(_) => {}
        Part::Text(t) => {
            if t.synthetic.unwrap_or(false) || t.text.trim().is_empty() {
                return;
            }
            inspect_button(ui, part, selected, |ui| {
                egui_commonmark::CommonMarkViewer::new().show(ui, md_cache, t.text.as_str());
            });
        }
        Part::Reasoning(r) => {
            if r.text.trim().is_empty() {
                return;
            }
            inspect_button(ui, part, selected, |ui| {
                ui.collapsing(egui::RichText::new("thinking…").weak().italics(), |ui| {
                    ui.set_max_width(ui.available_width());
                    ui.label(egui::RichText::new(&r.text).weak().italics());
                });
            });
        }
        Part::Tool(t) => {
            let (status_icon, status_color) = match &t.state {
                ToolState::Pending(_) => ("◌", AMBER),
                ToolState::Running(_) => ("◐", AMBER),
                ToolState::Completed(_) => ("●", GREEN),
                ToolState::Error(_) => ("●", RED),
            };
            let title = tool_title(&t.tool, &t.state);
            inspect_button(ui, part, selected, |ui| {
                ui.collapsing(
                    egui::RichText::new(format!("{status_icon} {} · {}", t.tool, title))
                        .color(status_color),
                    |ui| {
                        ui.set_max_width(ui.available_width());
                        match &t.state {
                            ToolState::Completed(s) => {
                                ui.monospace(format_args_input(&s.input));
                                if !s.output.is_empty() {
                                    ui.separator();
                                    ui.push_id(part_id(part).to_string(), |ui| {
                                        scrollable_output(ui, &s.output, 320.0);
                                    });
                                }
                            }
                            ToolState::Error(s) => {
                                ui.monospace(format_args_input(&s.input));
                                ui.colored_label(RED, &s.error);
                            }
                            ToolState::Running(s) => {
                                ui.monospace(format_args_input(&s.input));
                                if let Some(title_now) = &s.title {
                                    ui.weak(title_now);
                                }
                            }
                            ToolState::Pending(s) => {
                                ui.monospace(format_args_input(&s.input));
                            }
                        }
                    },
                );
            });
        }
        Part::Patch(p) => {
            inspect_button(ui, part, selected, |ui| {
                ui.label(
                    egui::RichText::new(format!("patch · {} file(s)", p.files.len()))
                        .small()
                        .color(MUTED),
                );
            });
        }
        Part::StepFinish(_) => {}
        other => {
            inspect_button(ui, part, selected, |ui| {
                ui.label(
                    egui::RichText::new(format!("[{}]", kind_name(other)))
                        .small()
                        .color(MUTED),
                );
            });
        }
    }
}

/// Wraps a part's rendering with a tiny `{}` button that opens the raw JSON
/// inspector for this part.
fn inspect_button(
    ui: &mut egui::Ui,
    part: &Part,
    selected: &mut Option<Part>,
    content: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        let width = ui.available_width() - 34.0;
        ui.vertical(|ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            content(ui);
        });
        if ui
            .small_button("{}")
            .on_hover_text("Inspect raw part JSON")
            .clicked()
        {
            *selected = Some(part.clone());
        }
    });
}

fn tool_title(tool: &str, state: &ToolState) -> String {
    let input = match state {
        ToolState::Pending(s) => Some(&s.input),
        ToolState::Running(s) => Some(&s.input),
        ToolState::Completed(s) => Some(&s.input),
        ToolState::Error(s) => Some(&s.input),
    };
    let candidate = input.and_then(|i| {
        i.get("filePath")
            .or_else(|| i.get("path"))
            .or_else(|| i.get("command"))
            .or_else(|| i.get("pattern"))
            .or_else(|| i.get("query"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });
    candidate.unwrap_or_else(|| tool.to_string())
}

fn format_args_input(input: &serde_json::Map<String, serde_json::Value>) -> String {
    serde_json::to_string(input).unwrap_or_default()
}

fn scrollable_output(ui: &mut egui::Ui, output: &str, max_height: f32) {
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.monospace(output);
        });
}

fn kind_name(p: &Part) -> &'static str {
    match p {
        Part::Subtask(_) => "subtask",
        Part::File(_) => "file",
        Part::Snapshot(_) => "snapshot",
        Part::Agent(_) => "agent",
        Part::Retry(_) => "retry",
        Part::Compaction(_) => "compaction",
        _ => "part",
    }
}
