//! Compiles the visible "context window" view-model from the mirrored state.
//!
//! This is the ADE's core inspection feature: reconstruct, in order, what the
//! model is likely seeing for the active session, with per-section token
//! estimates (chars/4 heuristic) anchored by real usage numbers from the last
//! assistant step (`StepFinishPartTokens`).

use crate::state::Store;
use opencode_codes::protocol_generated::types::{Message, Part};

#[derive(Debug, Clone)]
pub struct ContextSection {
    pub label: String,
    pub kind: SectionKind,
    pub detail: String,
    pub est_tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    System,
    Tools,
    User,
    Assistant,
    Reasoning,
    ToolCall,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct ContextView {
    pub sections: Vec<ContextSection>,
    pub est_total_tokens: usize,
    /// Real input-token figure from the most recent assistant message.
    pub actual_input_tokens: Option<f64>,
    pub actual_cache_read: Option<f64>,
    pub actual_output_tokens: Option<f64>,
    pub actual_total: Option<f64>,
}

pub fn est_tokens(text: &str) -> usize {
    text.chars().count() / 4 + if text.is_empty() { 0 } else { 1 }
}

pub fn compile(store: &Store) -> ContextView {
    let Some(messages) = store.active_messages() else {
        return ContextView::default();
    };

    let mut view = ContextView::default();

    // System prompt section: opencode injects its own system prompt; we cannot
    // read it verbatim from the protocol, so surface it as a fixed-cost block.
    view.sections.push(ContextSection {
        label: "system prompt".into(),
        kind: SectionKind::System,
        detail: "injected by opencode (not exposed over the wire)".into(),
        est_tokens: 2_000,
    });

    for m in messages {
        match &m.info {
            Message::User(u) => {
                let text = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                view.sections.push(ContextSection {
                    label: format!("user · {}", u.model.model_id),
                    kind: SectionKind::User,
                    detail: truncate(&text, 400),
                    est_tokens: est_tokens(&text),
                });
            }
            Message::Assistant(a) => {
                // Track real usage from the newest assistant message.
                view.actual_input_tokens = Some(a.tokens.input);
                view.actual_cache_read = Some(a.tokens.cache.read);
                view.actual_output_tokens = Some(a.tokens.output);
                view.actual_total = a.tokens.total.or(Some(
                    a.tokens.input + a.tokens.cache.read + a.tokens.output + a.tokens.reasoning,
                ));

                for part in &m.parts {
                    view.sections.push(part_section(part));
                }
            }
        }
    }

    view.est_total_tokens = view.sections.iter().map(|s| s.est_tokens).sum();
    view
}

fn part_section(part: &Part) -> ContextSection {
    match part {
        Part::Text(t) => ContextSection {
            label: "assistant text".into(),
            kind: SectionKind::Assistant,
            detail: truncate(&t.text, 400),
            est_tokens: est_tokens(&t.text),
        },
        Part::Reasoning(r) => ContextSection {
            label: "reasoning".into(),
            kind: SectionKind::Reasoning,
            detail: truncate(&r.text, 400),
            est_tokens: est_tokens(&r.text),
        },
        Part::Tool(t) => {
            use opencode_codes::protocol_generated::types::ToolState as TS;
            let (status, input_json, output) = match &t.state {
                TS::Pending(s) => ("pending", &s.input, String::new()),
                TS::Running(s) => ("running", &s.input, String::new()),
                TS::Completed(s) => ("completed", &s.input, s.output.clone()),
                TS::Error(s) => ("error", &s.input, s.error.clone()),
            };
            let mut body = serde_json::to_string(input_json).unwrap_or_default();
            if !output.is_empty() {
                body.push_str("\n→ ");
                body.push_str(output.trim());
            }
            ContextSection {
                label: format!("tool:{} [{}]", t.tool, status),
                kind: SectionKind::ToolCall,
                detail: truncate(&body, 500),
                est_tokens: est_tokens(&body),
            }
        }
        Part::StepStart(_) => ContextSection {
            label: "step-start".into(),
            kind: SectionKind::Other,
            detail: String::new(),
            est_tokens: 4,
        },
        Part::StepFinish(s) => ContextSection {
            label: format!(
                "step-finish ({:.0} in / {:.0} out tok)",
                s.tokens.input, s.tokens.output
            ),
            kind: SectionKind::Other,
            detail: format!("cost ${:.4}", s.cost),
            est_tokens: 8,
        },
        Part::File(f) => ContextSection {
            label: "file attachment".into(),
            kind: SectionKind::Other,
            detail: f.filename.clone().unwrap_or_else(|| f.url.clone()),
            est_tokens: 64,
        },
        Part::Patch(p) => ContextSection {
            label: "patch".into(),
            kind: SectionKind::Other,
            detail: p.files.join(", "),
            est_tokens: p.files.iter().map(|f| est_tokens(f)).sum::<usize>() + 16,
        },
        other => ContextSection {
            label: other_part_kind(other).to_string(),
            kind: SectionKind::Other,
            detail: String::new(),
            est_tokens: 8,
        },
    }
}

fn other_part_kind(p: &Part) -> &'static str {
    match p {
        Part::Subtask(_) => "subtask",
        Part::Snapshot(_) => "snapshot",
        Part::Agent(_) => "agent",
        Part::Retry(_) => "retry",
        Part::Compaction(_) => "compaction",
        _ => "part",
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}
