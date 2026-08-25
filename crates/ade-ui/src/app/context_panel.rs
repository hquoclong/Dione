//! "Context window" panel: what the model sees, in order, with token estimates.

use ade_core::context::{compile, SectionKind};
use ade_core::Store;

pub fn context_panel(ui: &mut egui::Ui, store: &Store) {
    let view = compile(store);

    ui.heading("Context window");
    ui.add_space(2.0);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!(
            "est ≈ {} tok",
            format_usize(view.est_total_tokens)
        )));
        if let Some(actual) = view.actual_total {
            ui.separator();
            ui.label(
                egui::RichText::new(format!("last step: {}", format_f64(actual)))
                    .color(egui::Color32::from_rgb(0x3f, 0xd1, 0x7c)),
            );
        }
    });
    ui.horizontal(|ui| {
        if let Some(input) = view.actual_input_tokens {
            ui.weak(format!("in {}", format_f64(input)));
        }
        if let Some(cache) = view.actual_cache_read {
            ui.weak(format!("cache {}", format_f64(cache)));
        }
        if let Some(out) = view.actual_output_tokens {
            ui.weak(format!("out {}", format_f64(out)));
        }
    });

    ui.add_space(6.0);
    ui.separator();

    // Render newest-last like the wire order.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut last_kind: Option<SectionKind> = None;
            for s in &view.sections {
                let is_flow = matches!(
                    s.kind,
                    SectionKind::Assistant | SectionKind::ToolCall | SectionKind::Reasoning
                );
                if Some(s.kind) != last_kind && !is_flow {
                    ui.add_space(4.0);
                }
                last_kind = Some(s.kind);

                let color = match s.kind {
                    SectionKind::System => egui::Color32::from_rgb(0xb1, 0x8c, 0xf0),
                    SectionKind::User => egui::Color32::from_rgb(0x5e, 0xb1, 0xf0),
                    SectionKind::Assistant => egui::Color32::from_rgb(0x3f, 0xd1, 0x7c),
                    SectionKind::Reasoning => egui::Color32::GRAY,
                    SectionKind::ToolCall => egui::Color32::from_rgb(0xf5, 0xc5, 0x42),
                    SectionKind::Tools | SectionKind::Other => {
                        egui::Color32::from_rgb(0x8b, 0x8b, 0x93)
                    }
                };

                ui.collapsing(
                    egui::RichText::new(format!("{:>6} tok · {}", s.est_tokens, s.label))
                        .color(color),
                    |ui| {
                        ui.set_max_width(ui.available_width());
                        if s.detail.is_empty() {
                            ui.weak("(no content)");
                        } else {
                            ui.monospace(&s.detail);
                        }
                    },
                );
            }
        });
}

fn format_usize(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn format_f64(n: f64) -> String {
    if n >= 10_000.0 {
        format!("{:.1}k", n / 1000.0)
    } else {
        format!("{n:.0}")
    }
}
