//! Root view: layout shell of the ADE window.

use ade_core::RuntimeHandle;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _, label::Label};

pub struct AdeApp {
    #[allow(dead_code)]
    rt: RuntimeHandle,
}

impl AdeApp {
    pub fn new(rt: RuntimeHandle) -> Self {
        Self { rt }
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
            // top bar placeholder
            .child(
                div()
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(border)
                    .child(Label::new("ADE").text_size(px(14.)))
                    .child(
                        Icon::new(IconName::CircleCheck)
                            .small()
                            .text_color(rgb(0x3fd17c)),
                    ),
            )
            // body
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Label::new("GPUI skeleton — panels port next")),
            )
    }
}
