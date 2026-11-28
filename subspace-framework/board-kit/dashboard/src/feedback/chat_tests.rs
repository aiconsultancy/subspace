//! Chat-free state and banner words offer Copy and block selection.

use gpui_kit::base::TextSelectionLayer;
use gpui_kit::test::{ClickOptions, TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Render, Styled as _, TestAppContext, Window, div, point, px,
};

use super::{Words, error_banner_words, error_box_words};
use crate::Tokens;
use crate::row_menu::MenuChat;

const WIDTH: f32 = 400.;
/// Over the state box's title, below its top padding.
const STATE_TITLE: (f32, f32) = (200., 132.);
/// Over the banner's message, which fills the row after its title.
const BANNER_MESSAGE: (f32, f32) = (200., 12.);

struct Surfaces(MenuChat);

impl Render for Surfaces {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::one_dark();
        let chat = self.0;
        let words = |title: &str| Words {
            chat,
            ..Words::new(title.to_owned(), "A message wide enough to click")
        };
        let none: [AnyElement; 0] = [];
        div()
            .size_full()
            .child(TextSelectionLayer)
            .child(
                div()
                    .id("banner-host")
                    .test_support()
                    .w(px(WIDTH))
                    .child(error_banner_words(
                        &tokens,
                        "chat-banner",
                        words("Banner"),
                        none,
                    )),
            )
            .child(
                div()
                    .id("state-host")
                    .test_support()
                    .w(px(WIDTH))
                    .child(error_box_words(
                        &tokens,
                        "chat-state",
                        words("State"),
                        [div().into_any_element()],
                    )),
            )
    }
}

/// The entries of the menu opened at `at` inside the element `host`.
fn menu_labels(
    chat: MenuChat,
    host: &'static str,
    at: (f32, f32),
    cx: &mut TestAppContext,
) -> Vec<String> {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::rich::set_local_chat_enabled(cx, true);
    });
    let (_, cx) = cx.add_window_view(|_, _| Surfaces(chat));
    let options = ClickOptions::new()
        .with_offset(point(px(at.0), px(at.1)))
        .with_button(MouseButton::Right);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click_with_options(host, options, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        (0usize..4)
            .map_while(|index| window.try_find(index))
            .filter_map(|item| item.label().map(str::to_owned))
            .collect()
    })
}

#[gpui_kit::test]
fn a_state_offers_the_chat_by_default(cx: &mut TestAppContext) {
    let labels = menu_labels(MenuChat::Offered, "state-host", STATE_TITLE, cx);
    assert_eq!(labels, ["Copy", "Select all", "Add to local model chat"]);
}

#[gpui_kit::test]
fn a_chat_free_state_offers_copy_only(cx: &mut TestAppContext) {
    let labels = menu_labels(MenuChat::Off, "state-host", STATE_TITLE, cx);
    assert_eq!(labels, ["Copy", "Select all"]);
}

#[gpui_kit::test]
fn a_chat_free_banner_offers_copy_only(cx: &mut TestAppContext) {
    let labels = menu_labels(MenuChat::Off, "banner-host", BANNER_MESSAGE, cx);
    assert_eq!(labels, ["Copy", "Select all"]);
}
