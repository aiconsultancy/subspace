//! Theme designer controls and live appearance samples.

mod choice;
mod preview;
mod sections;

use gpui_kit::component::Disableable as _;

use gpui_kit::component::accordion::Accordion;
use gpui_kit::component::{Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement, Render,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, px,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, dialog_button};
use subspace_dashboard::icon_button::IconButton;
use subspace_lucide::IconName;

use super::{JIG_CONTEXT, Jig, JigEvent, Section};

const PANEL_WIDTH: f32 = 520.;
const LABEL_SIZE: f32 = 14.;

impl Jig {
    fn header(&self, cx: &mut Context<Self>) -> Div {
        let tokens = Tokens::current(cx);
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .p_3()
            .child(
                div()
                    .flex_1()
                    .font_family(subspace_dashboard::typography::heading_font(cx))
                    .text_size(tokens.heading_size(LABEL_SIZE))
                    .child("Theme designer"),
            )
            .child(
                IconButton::new("jig-close")
                    .icon(IconName::X)
                    .label("Close")
                    .disabled(self.busy)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(JigEvent::Close))),
            )
    }

    fn actions(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_3()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        dialog_button(
                            "jig-reset",
                            Intent::Neutral,
                            Some(IconName::RotateCcw),
                            "Reset all",
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(|jig, _, window, cx| jig.reset(window, cx))),
                    )
                    .child(
                        dialog_button(
                            "jig-copy",
                            Intent::Neutral,
                            Some(IconName::Copy),
                            "Copy JSON",
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(|jig, _, _, cx| jig.copy(cx))),
                    )
                    .child(
                        dialog_button("jig-cancel", Intent::Neutral, Some(IconName::X), "Cancel")
                            .disabled(self.busy)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(JigEvent::Close))),
                    )
                    .child(
                        dialog_button("jig-save", Intent::Primary, Some(IconName::Save), "Save")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|jig, _, window, cx| jig.save(false, window, cx)),
                            ),
                    ),
            )
            .when(self.pending.is_some(), |footer| {
                footer.child(self.confirmation(cx))
            })
            .child(div().text_sm().child(self.status.clone()))
    }

    fn confirmation(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                dialog_button(
                    "jig-confirm",
                    Intent::Neutral,
                    Some(IconName::Save),
                    "Confirm overwrite",
                )
                .disabled(self.busy)
                .on_click(cx.listener(|jig, _, window, cx| jig.save(true, window, cx))),
            )
            .child(
                dialog_button(
                    "jig-keep-file",
                    Intent::Neutral,
                    Some(IconName::X),
                    "Keep existing file",
                )
                .disabled(self.busy)
                .on_click(cx.listener(|jig, _, _, cx| {
                    jig.pending = None;
                    jig.status = "Overwrite cancelled. No file changed.".into();
                    cx.notify();
                })),
            )
    }

    fn body(&self, cx: &mut Context<Self>) -> Div {
        let open = self.open.clone();
        let sections = Section::ALL.into_iter().enumerate().fold(
            Accordion::new("theme-designer-sections")
                .multiple(true)
                .bordered(false)
                .disabled(self.busy),
            |accordion, (index, section)| {
                let shown = open.contains(&index);
                let content = shown.then(|| self.section(section, cx));
                accordion.item(|item| item.title(section.title()).open(shown).children(content))
            },
        );
        v_flex().p_3().child(
            sections
                .on_toggle_click(cx.listener(|jig, open: &[usize], _, cx| jig.set_open(open, cx))),
        )
    }
}

impl Render for Jig {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_sliders(window, cx);
        let tokens = Tokens::current(cx);
        let width = f32::from(window.viewport_size().width).min(PANEL_WIDTH);
        v_flex()
            .id("theme-designer")
            .key_context(JIG_CONTEXT)
            .track_focus(&self.focus)
            .absolute()
            .right_0()
            .top_0()
            .bottom_0()
            .w(px(width))
            .bg(tokens.surface)
            .text_color(tokens.fg)
            .text_size(px(LABEL_SIZE))
            .font_family(Theme::global(cx).font_family.clone())
            .border_l_1()
            .border_color(tokens.line)
            .overflow_hidden()
            .child(self.header(cx))
            .child(
                div()
                    .id("theme-designer-scroll")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(self.body(cx)),
            )
            .child(self.actions(cx))
    }
}
