//! M0 root view: top bar + placeholder. Fleet UI lands in M2.

use gpui::*;
use gpui_component::{ActiveTheme, label::Label};

fn muted_color() -> Rgba {
    rgba(0x8b8b93ff)
}

pub struct AdeApp {
    worktree_count: usize,
}

impl AdeApp {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { worktree_count: 0 }
    }
}

impl Render for AdeApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let fg = cx.theme().foreground;
        let border = cx.theme().border;

        div()
            .id("ade-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(bg)
            .text_color(fg)
            .text_size(px(13.))
            .child(
                div()
                    .h(px(38.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(border)
                    .child(Label::new("ADE — Agentic IDE (M0)"))
                    .child(
                        Label::new(format!("{} worktrees", self.worktree_count))
                            .text_color(muted_color()),
                    ),
            )
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Label::new("M0 bootstrap — single-agent lands in M1, fleet in M2.")
                            .text_color(muted_color()),
                    ),
            )
    }
}
