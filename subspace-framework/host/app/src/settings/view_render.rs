use super::{Settings, body_height};
use gpui_kit::component::dialog::DialogTitle;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, App, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    TestSupportExt as _, Window, div, px, relative,
};
use subspace_dashboard::{MODAL_CONTEXT, Tokens, rich};

const SIDEBAR_MAX_FRACTION: f32 = 0.5;

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_dialog(window, cx, None, None)
    }
}

impl Settings {
    pub(crate) fn render_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        updates: Option<AnyView>,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        let tokens = Tokens::current(cx);
        let navigation = self.navigation_tree(cx);
        let body = self.page(&tokens, cx);
        h_flex()
            .id("settings-body")
            .test_support()
            .key_context(MODAL_CONTEXT)
            .track_focus(&self.focus)
            .items_stretch()
            .h(px(body_height(f32::from(window.viewport_size().height))))
            .min_h_0()
            .child(
                v_flex()
                    .id("settings-navigation")
                    .test_support()
                    .w_64()
                    .max_w(relative(SIDEBAR_MAX_FRACTION))
                    .min_w_0()
                    .flex_shrink_0()
                    .bg(tokens.sidebar)
                    .p_2()
                    .child(div().flex_1().min_h_0().child(navigation))
                    .children(updates.map(|updates| div().min_w_0().pt_2().child(updates))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .p_4()
                    .gap_3()
                    .child(breadcrumb(self.trail(cx), &tokens))
                    .child(scrolling_page(body, &self.scroll))
                    .children(self.messages(&tokens))
                    .children(actions),
            )
            .into_any_element()
    }

    fn trail(&self, cx: &App) -> Vec<SharedString> {
        super::navigation::page_trail(
            &self.page,
            &self.boards,
            crate::settings::extensions::registrations(cx),
        )
    }

    fn messages(&self, tokens: &Tokens) -> Option<impl IntoElement> {
        let error = self.error.clone();
        let notice = self.notice.clone();
        (error.is_some() || notice.is_some()).then(|| {
            v_flex()
                .gap_3()
                .when_some(error, |body, error| {
                    body.child(
                        div()
                            .id("settings-error-message")
                            .test_support()
                            .aria_label(error.clone())
                            .text_color(tokens.red)
                            .child(rich::plain_text("settings-error", &error)),
                    )
                })
                .when_some(notice, |body, notice| {
                    body.child(
                        div()
                            .text_color(tokens.yellow)
                            .child(rich::plain_text("settings-notice", &notice)),
                    )
                })
        })
    }
}

/// `Settings / Space / GitHub` as the dialog title: one line, with the current
/// page in the strong colour and the trail above it muted.
fn breadcrumb(trail: Vec<SharedString>, tokens: &Tokens) -> impl IntoElement {
    let last = trail.len().saturating_sub(1);
    let (muted, current) = (tokens.muted, tokens.fg_strong);
    DialogTitle::new().pr_6().child(
        h_flex()
            .id("settings-breadcrumb")
            .test_support()
            .aria_label(SharedString::from(trail.join(" / ")))
            .min_w_0()
            .flex_wrap()
            .gap_x_1p5()
            .children(trail.into_iter().enumerate().flat_map(|(index, segment)| {
                let separator = (index > 0).then(|| div().text_color(muted).child("/"));
                let color = if index == last { current } else { muted };
                separator
                    .into_iter()
                    .chain(std::iter::once(div().text_color(color).child(segment)))
            })),
    )
}

/// The page body's scroll region, between the breadcrumb and the footer.
/// Like GPUI Kit's `Scrollable`, the scrollbar sits in an overlay that
/// covers exactly the region, so it never reaches the header or footer.
/// The Settings scroll handle stays shared so a page change can reset it.
fn scrolling_page(body: AnyElement, scroll: &ScrollHandle) -> impl IntoElement {
    div()
        .id("settings-scroll-region")
        .test_support()
        .relative()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(
            div()
                .id("settings-scroll")
                .test_support()
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(div().pr_4().child(body)),
        )
        .child(
            div()
                .id("settings-scrollbar")
                .test_support()
                .absolute()
                .inset_0()
                .child(
                    Scrollbar::vertical(scroll)
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
        )
}
