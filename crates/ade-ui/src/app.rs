//! Application shell: layout, panels wiring, shared UI state.

use std::sync::Arc;

use ade_core::{Command, ConnState, PermissionResponse, RuntimeHandle, Store};
use egui::Color32;
use opencode_codes::protocol_generated::types::Part;

mod context_panel;
mod diff_view;
mod inspector;
mod timeline;

pub struct AdeApp {
    rt: RuntimeHandle,
    snapshot: Arc<Store>,
    md_cache: egui_commonmark::CommonMarkCache,
    composer: String,
    new_session_title: String,
    selected_part: Option<Part>,
    right_tab: RightTab,
    model_query: Option<(String, String)>, // (provider_id, model_id)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RightTab {
    Context,
    Inspector,
    Diff,
}

impl AdeApp {
    pub fn new(_cc: &eframe::CreationContext, rt: RuntimeHandle) -> Self {
        let snapshot = rt.snapshot();
        Self {
            rt,
            snapshot,
            md_cache: Default::default(),
            composer: String::new(),
            new_session_title: String::new(),
            selected_part: None,
            right_tab: RightTab::Context,
            model_query: None,
        }
    }

    fn refresh(&mut self) {
        self.snapshot = self.rt.snapshot();
    }
}

impl eframe::App for AdeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.refresh();
        let busy = self.snapshot.is_busy();

        top_bar(self, ui);
        sessions_sidebar(self, ui);

        egui::Panel::bottom("composer").show(ui, |ui| {
            composer(self, ui);
        });

        inspector::right_panel(
            ui,
            &mut self.right_tab,
            &self.snapshot,
            &mut self.selected_part,
        );

        egui::CentralPanel::default().show(ui, |ui| {
            error_strip(self, ui);
            if let Some(sid) = self.snapshot.active_session.clone() {
                timeline::timeline(
                    ui,
                    &self.snapshot,
                    &sid,
                    &mut self.selected_part,
                    &mut self.md_cache,
                );
            } else {
                welcome(ui);
            }
        });

        permission_modal(self, ui);

        // Drive repaints while the runtime is live; idle is cheap.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(if busy {
                120
            } else {
                400
            }));
    }

    fn on_exit(&mut self) {
        // Dropping the handle closes the command channel and stops the server.
    }
}

fn top_bar(app: &mut AdeApp, ui: &mut egui::Ui) {
    egui::Panel::top("top_bar").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let (dot, label, reason) = match &app.snapshot.conn {
                ConnState::Connected => (
                    Color32::from_rgb(0x3f, 0xd1, 0x7c),
                    "connected",
                    String::new(),
                ),
                ConnState::Connecting => (
                    Color32::from_rgb(0xf5, 0xc5, 0x42),
                    "connecting…",
                    String::new(),
                ),
                ConnState::Disconnected(e) => (
                    Color32::from_rgb(0xe5, 0x48, 0x4d),
                    "disconnected",
                    e.clone(),
                ),
            };
            ui.colored_label(dot, "●");
            ui.label(label);
            if !reason.is_empty() {
                ui.label(egui::RichText::new(reason).small().color(Color32::GRAY));
            }

            ui.separator();
            model_picker(app, ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let t = &app.snapshot.totals;
                ui.colored_label(
                    Color32::GRAY,
                    format!(
                        "ctx≈{:.0}k tok · ${:.4}",
                        t.total_context() / 1000.0,
                        t.cost
                    ),
                );
                ui.separator();
                ui.label(format!("events: {}", app.snapshot.event_count));
            });
        });
        ui.add_space(2.0);
    });
}

fn model_picker(app: &mut AdeApp, ui: &mut egui::Ui) {
    let current = app
        .model_query
        .as_ref()
        .map(|(p, m)| format!("{p}/{m}"))
        .unwrap_or_else(|| "model: default".to_string());

    egui::ComboBox::from_id_salt("model_pick")
        .selected_text(current)
        .width(240.0)
        .show_ui(ui, |ui| {
            if app.snapshot.providers.is_empty() {
                ui.label("(providers not loaded yet)");
            }
            for prov in &app.snapshot.providers {
                ui.separator();
                ui.weak(&prov.provider_name);
                for (mid, name) in &prov.models {
                    let key = (prov.provider_id.clone(), mid.clone());
                    if ui
                        .selectable_label(
                            app.model_query.as_ref() == Some(&key),
                            format!("{mid} — {name}"),
                        )
                        .clicked()
                    {
                        app.model_query = Some(key.clone());
                        app.rt.send(Command::SetModel {
                            provider_id: key.0,
                            model_id: key.1,
                        });
                    }
                }
            }
        });
}

fn sessions_sidebar(app: &mut AdeApp, ui: &mut egui::Ui) {
    egui::Panel::left("sessions")
        .resizable(true)
        .default_size(230.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading("Sessions");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let title_edit = ui.add_sized(
                    [ui.available_width() - 62.0, 22.0],
                    egui::TextEdit::singleline(&mut app.new_session_title)
                        .hint_text("new session…"),
                );
                let create = ui.button("+").on_hover_text("Create session");
                if create.clicked()
                    || (title_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    let title = std::mem::take(&mut app.new_session_title);
                    app.rt.send(Command::CreateSession { title });
                }
            });

            ui.add_space(8.0);
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // newest first
                let mut ids: Vec<_> = app.snapshot.sessions.keys().cloned().collect();
                ids.sort_by_key(|id| {
                    std::cmp::Reverse(
                        app.snapshot
                            .sessions
                            .get(id)
                            .map(|s| s.time.updated)
                            .unwrap_or(0),
                    )
                });
                for id in ids {
                    let Some(s) = app.snapshot.sessions.get(&id) else {
                        continue;
                    };
                    let active = app.snapshot.active_session.as_deref() == Some(id.as_str());
                    let status = app
                        .snapshot
                        .statuses
                        .get(&id)
                        .map(|st| match st {
                            opencode_codes::protocol_generated::types::SessionStatus::Idle => "",
                            opencode_codes::protocol_generated::types::SessionStatus::Busy => " ●",
                            opencode_codes::protocol_generated::types::SessionStatus::Retry {
                                ..
                            } => " ⟳",
                        })
                        .unwrap_or("");
                    ui.horizontal(|ui| {
                        let label = ui.selectable_label(active, truncate(s.title.as_str(), 26));
                        if !status.is_empty() {
                            ui.colored_label(Color32::from_rgb(0x3f, 0xd1, 0x7c), status.trim());
                        }
                        if label.clicked() {
                            app.rt.send(Command::SelectSession(id.clone()));
                        }
                        if active {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                        app.rt.send(Command::DeleteSession(id.clone()));
                                    }
                                    if ui.small_button("⑂").on_hover_text("Fork").clicked() {
                                        app.rt.send(Command::ForkSession(id.clone()));
                                    }
                                    if ui.small_button("⇄").on_hover_text("Fetch diff").clicked()
                                    {
                                        app.rt.send(Command::FetchDiff(id.clone()));
                                        app.right_tab = RightTab::Diff;
                                    }
                                },
                            );
                        }
                    });
                }
            });
        });
}

fn composer(app: &mut AdeApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let busy = app.snapshot.is_busy();
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let send_enabled = !busy && !app.composer.trim().is_empty();
        let input = ui.add_sized(
            [ui.available_width() - 150.0, 64.0],
            egui::TextEdit::multiline(&mut app.composer)
                .hint_text(if busy {
                    "agent working…"
                } else {
                    "Message the agent… (Ctrl+Enter to send)"
                })
                .desired_rows(2),
        );
        // Multiline eats plain Enter; send on Ctrl/Cmd+Enter while focused.
        let wants_send = input.has_focus()
            && ui.input(|i| {
                i.key_pressed(egui::Key::Enter) && (i.modifiers.ctrl || i.modifiers.command)
            });
        if wants_send && send_enabled {
            let text = std::mem::take(&mut app.composer);
            app.rt.send(Command::Prompt { text });
        }
        ui.vertical(|ui| {
            if busy {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("Abort").color(Color32::from_rgb(0xe5, 0x48, 0x4d)),
                    ))
                    .clicked()
                {
                    app.rt.send(Command::Abort);
                }
                ui.colored_label(Color32::from_rgb(0x3f, 0xd1, 0x7c), "working…");
            } else {
                let btn = ui.add_enabled(send_enabled, egui::Button::new("Send ⏎"));
                if btn.clicked() && send_enabled {
                    let text = std::mem::take(&mut app.composer);
                    app.rt.send(Command::Prompt { text });
                }
            }
        });
    });
    ui.add_space(4.0);
}

fn error_strip(app: &AdeApp, ui: &mut egui::Ui) {
    if app.snapshot.errors.is_empty() {
        return;
    }
    let last = app.snapshot.errors.back().cloned().unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(Color32::from_rgb(0xe5, 0x48, 0x4d), "⚠");
        ui.label(egui::RichText::new(last).small());
    });
    ui.separator();
}

fn permission_modal(app: &mut AdeApp, ui: &mut egui::Ui) {
    let Some(pending) = app.snapshot.pending_permissions.values().next().cloned() else {
        return;
    };
    if pending.session_id != app.snapshot.active_session.clone().unwrap_or_default() {
        return;
    }

    let mut action: Option<PermissionResponse> = None;
    egui::Window::new("🔒 Permission required")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, -60.0])
        .show(ui.ctx(), |ui| {
            ui.label("The agent wants to use a protected capability:".to_string());
            ui.monospace(egui::RichText::new(&pending.kind).heading());
            if !pending.patterns.is_empty() {
                ui.weak("targets:");
                for p in &pending.patterns {
                    ui.monospace(p);
                }
            }
            if let Some(meta_title) = pending.metadata.get("title").and_then(|t| t.as_str()) {
                ui.weak(meta_title);
            }
            if let Some(cmdline) = pending.metadata.get("command").and_then(|c| c.as_str()) {
                ui.monospace(truncate(cmdline, 200));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Allow once").clicked() {
                    action = Some(PermissionResponse::Once);
                }
                if ui.button("Always allow").clicked() {
                    action = Some(PermissionResponse::Always);
                }
                if ui
                    .button(
                        egui::RichText::new("Reject").color(Color32::from_rgb(0xe5, 0x48, 0x4d)),
                    )
                    .clicked()
                {
                    action = Some(PermissionResponse::Reject);
                }
            });
        });
    if let Some(a) = action {
        app.rt.send(Command::PermissionReply {
            permission_id: pending.permission_id,
            response: a,
        });
    }
}

fn welcome(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("ADE — Agent Development Environment");
            ui.weak("Run an agent and look inside its context window.");
            ui.add_space(16.0);
            ui.label("Create a session in the sidebar to begin.");
        });
    });
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}
