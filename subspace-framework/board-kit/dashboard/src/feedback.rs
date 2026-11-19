//! Loading, empty and error states (REQ-13), including the list skeleton.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ClickEvent, Div, ElementId, Hsla, ParentElement, SharedString, Styled, Window, div, px,
};

use crate::action::{Intent, action_button};
use crate::rich;
use crate::row_menu::MenuChat;
use crate::tokens::Tokens;
use subspace_lucide::IconName;

const RETRY_MARGIN_PX: f32 = 8.;
const BANNER_VERTICAL_PADDING_PX: f32 = 6.;
const STATE_TOP_PADDING_PX: f32 = 120.;
const STATE_SIDE_PADDING_PX: f32 = 16.;
const STATE_MESSAGE_MAX_WIDTH_PX: f32 = 520.;
/// Preserve the existing fixed-pixel list skeleton geometry at every zoom level.
const SKELETON_BAR_HEIGHT_PX: f32 = 8.;
const SKELETON_LEADING_INSET_PX: f32 = 26.;
const SKELETON_ID_WIDTH_PX: f32 = 48.;
const SKELETON_TITLE_WIDTHS_PX: [f32; 8] = [220., 300., 180., 260., 240., 150., 280., 200.];
const SKELETON_PARENT_INTERVAL: usize = 4;
const SKELETON_BAR_OPACITY: f32 = 0.6;

/// Stable identity for one selectable part of a shared empty or error surface.
///
/// `instance` is the caller's identity, so two surfaces with the same title in
/// one view do not share an element id.
pub fn text_key(
    instance: &'static str,
    kind: &'static str,
    part: &'static str,
    title: &str,
) -> String {
    format!("feedback-{instance}-{kind}-{part}-{title}")
}

/// Placeholder rows while a list loads for the first time.
pub fn skeleton(tokens: &Tokens) -> Div {
    let bar = |width: f32| {
        div()
            .h(px(SKELETON_BAR_HEIGHT_PX))
            .w(px(width))
            .rounded(tokens.control_radius)
            .bg(tokens.line.opacity(SKELETON_BAR_OPACITY))
    };
    v_flex().w_full().children(
        SKELETON_TITLE_WIDTHS_PX
            .iter()
            .enumerate()
            .map(|(i, width)| {
                let parent = i % SKELETON_PARENT_INTERVAL == 0;
                h_flex()
                    .h(tokens.row_height)
                    .gap_2()
                    .pl(if parent {
                        px(SKELETON_LEADING_INSET_PX)
                    } else {
                        px(SKELETON_LEADING_INSET_PX) + tokens.indent
                    })
                    .border_b_1()
                    .border_color(tokens.hair)
                    .when(parent, |row| row.bg(tokens.pane))
                    .child(bar(SKELETON_ID_WIDTH_PX))
                    .child(bar(*width))
            }),
    )
}

/// The words of a state or banner, and whether their text menu offers the
/// chat. A board whose content stays out of chat sets `chat` to
/// [`MenuChat::Off`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    pub title: SharedString,
    pub message: SharedString,
    pub chat: MenuChat,
}

impl Words {
    pub fn new(title: impl Into<SharedString>, message: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            chat: MenuChat::Offered,
        }
    }
}

/// A centred box with a title and a dimmed message.
fn state_box(tokens: &Tokens, instance: &'static str, words: &Words, title_color: Hsla) -> Div {
    let title_key = text_key(instance, "state", "title", &words.title);
    let message_key = text_key(instance, "state", "message", &words.title);
    v_flex()
        .w_full()
        .items_center()
        .pt(px(STATE_TOP_PADDING_PX))
        .px(px(STATE_SIDE_PADDING_PX))
        .gap_2()
        .text_size(tokens.row_font)
        .child(
            div()
                .text_size(tokens.heading_font)
                .text_color(title_color)
                .child(rich::plain_text_with(title_key, &words.title, words.chat)),
        )
        .child(
            div()
                .max_w(px(STATE_MESSAGE_MAX_WIDTH_PX))
                .text_color(tokens.muted)
                .child(rich::plain_text_with(
                    message_key,
                    &words.message,
                    words.chat,
                )),
        )
}

/// An empty state with caller-supplied actions, its words' chat choice kept.
pub fn empty_words(
    tokens: &Tokens,
    instance: &'static str,
    words: Words,
    actions: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
) -> Div {
    state_box(tokens, instance, &words, tokens.fg_strong).child(
        h_flex()
            .mt(px(RETRY_MARGIN_PX))
            .gap_2()
            .flex_wrap()
            .justify_center()
            .children(actions),
    )
}

/// A load failure with caller-supplied actions, its words' chat choice kept.
pub fn error_box_words(
    tokens: &Tokens,
    instance: &'static str,
    words: Words,
    actions: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
) -> Div {
    state_box(tokens, instance, &words, tokens.red).child(
        h_flex()
            .mt(px(RETRY_MARGIN_PX))
            .gap_2()
            .flex_wrap()
            .children(actions),
    )
}

/// A muted, selectable one-line loading message for an in-panel fetch.
pub fn loading_line(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    message: impl Into<SharedString>,
) -> Div {
    let message = message.into();
    div()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(rich::plain_text(id, &message))
}

/// A muted, selectable one-line note that a part of a view loaded with
/// nothing to show, such as a list group whose filter matches none of its
/// records. An alias of [`loading_line`]: the same treatment under a name
/// that says what it shows, so it can take that line's place when the load
/// ends.
pub fn empty_line(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    message: impl Into<SharedString>,
) -> Div {
    loading_line(tokens, id, message)
}

/// Nothing to show: the dashboard's message and hint.
pub fn empty(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
) -> Div {
    state_box(tokens, instance, &Words::new(title, hint), tokens.fg_strong)
}

/// An empty setup surface with an in-app configuration action.
pub fn configuration(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
) -> Div {
    empty(tokens, instance, title, hint).child(
        h_flex()
            .mt(px(RETRY_MARGIN_PX))
            .child(configure_action(format!("{instance}-configure"))),
    )
}

/// An empty setup surface with caller-supplied actions, such as opening
/// Settings and copying setup instructions.
// qual:api
pub fn empty_with_actions(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    actions: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
) -> Div {
    empty_words(tokens, instance, Words::new(title, hint), actions)
}

/// Open the visible board's settings without depending on the app shell.
pub fn configure_action(id: impl Into<ElementId>) -> crate::icon_button::IconButton {
    recovery_action(
        id,
        Intent::Neutral,
        IconName::Settings,
        "Configure",
        |_, window, cx| window.dispatch_action(Box::new(crate::ConfigureBoard), cx),
    )
}

/// A board load failure with both retry and connection settings.
pub fn error_box_with_settings(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    error_box_with_actions(
        tokens,
        instance,
        title,
        message,
        [
            retry_action(format!("{instance}-retry"), on_retry),
            configure_action(format!("{instance}-configure")),
        ],
    )
}

/// A load failed: the title, the message and a Retry button.
pub fn error_box(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let title = title.into();
    let retry = text_key(instance, "state", "retry", &title);
    error_box_with_actions(
        tokens,
        instance,
        title,
        message,
        [retry_action(retry, on_retry)],
    )
}

/// A load failure with caller-supplied recovery controls.
///
/// Use this when recovery is more specific than retrying, for example opening
/// settings or copying safe diagnostics. The caller remains responsible for
/// confirmation before any privileged action.
pub fn error_box_with_actions(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    actions: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
) -> Div {
    error_box_words(tokens, instance, Words::new(title, message), actions)
}

/// A one-line error above a list that keeps its last data.
pub fn error_banner(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let title = title.into();
    let retry = text_key(instance, "banner", "retry", &title);
    error_banner_with_actions(
        tokens,
        instance,
        title,
        message,
        [retry_action(retry, on_retry)],
    )
}

/// A compact load failure with caller-supplied recovery controls.
pub fn error_banner_with_actions(
    tokens: &Tokens,
    instance: &'static str,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    actions: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
) -> Div {
    error_banner_words(tokens, instance, Words::new(title, message), actions)
}

/// A compact load failure, its words' chat choice kept.
pub fn error_banner_words(
    tokens: &Tokens,
    instance: &'static str,
    words: Words,
    actions: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
) -> Div {
    let title_key = text_key(instance, "banner", "title", &words.title);
    let message_key = text_key(instance, "banner", "message", &words.title);
    h_flex()
        .w_full()
        .gap_2()
        .px(tokens.pad_x)
        .py(px(BANNER_VERTICAL_PADDING_PX))
        .border_b_1()
        .border_color(tokens.red)
        .text_size(tokens.row_font)
        .child(div().text_color(tokens.red).child(rich::plain_text_with(
            title_key,
            &words.title,
            words.chat,
        )))
        .child(
            rich::plain_text_with(message_key, &words.message, words.chat)
                .flex_1()
                .min_w_0()
                .text_color(tokens.muted),
        )
        .children(actions)
}

/// The standard Retry action used by error boxes, banners and inline errors.
pub fn retry_action(
    id: impl Into<ElementId>,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::icon_button::IconButton {
    recovery_action(id, Intent::Primary, IconName::RotateCw, "Retry", on_retry)
}

/// The standard glyph-bearing action for empty and error states, such as
/// Retry, Restart or New tab.
pub fn recovery_action(
    id: impl Into<ElementId>,
    intent: Intent,
    icon: subspace_lucide::SvgIcon,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::icon_button::IconButton {
    action_button(id, intent, Some(icon), label).on_click(on_click)
}

#[cfg(test)]
#[path = "feedback/chat_tests.rs"]
mod chat_tests;

#[cfg(test)]
mod tests {
    use super::{error_banner_with_actions, text_key};
    use crate::Tokens;
    use crate::links;
    use gpui_kit::base::{TextSelection, TextSelectionLayer};
    use gpui_kit::component::button::Button;
    use gpui_kit::component::v_flex;
    use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
    use gpui_kit::{
        App, Bounds, Context, FocusHandle, InteractiveElement as _, IntoElement,
        ParentElement as _, Pixels, Render, Styled as _, TestAppContext, Window, point, px,
    };

    struct Configuration {
        opened: bool,
        focus: FocusHandle,
    }

    impl Render for Configuration {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .track_focus(&self.focus)
                .child(super::configuration(
                    &Tokens::one_dark(),
                    "setup-test",
                    "No repositories configured",
                    "Add repositories in Settings.",
                ))
                .on_action(cx.listener(|this, _: &crate::ConfigureBoard, _, _| {
                    this.opened = true;
                }))
        }
    }

    #[gpui_kit::test]
    fn configure_dispatches_an_in_app_settings_action(cx: &mut TestAppContext) {
        cx.update(gpui_kit::component::init);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            Configuration {
                opened: false,
                focus,
            }
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            window.click("setup-test-configure", cx);
        });
        assert!(view.read_with(cx, |view, _| view.opened));
    }

    #[test]
    fn feedback_identities_are_stable_and_distinct() {
        let load = text_key("list-error", "state", "message", "Could not load");
        let empty = text_key("list-empty", "state", "message", "Nothing here");
        let banner = text_key("list-banner", "banner", "message", "Could not load");
        let title = text_key("list-error", "state", "title", "Could not load");
        let retry = text_key("list-error", "state", "retry", "Could not load");
        assert_eq!(load, "feedback-list-error-state-message-Could not load");
        assert_eq!(
            load,
            text_key("list-error", "state", "message", "Could not load")
        );
        assert_ne!(load, empty);
        assert_ne!(load, banner);
        assert_ne!(load, title);
        assert_ne!(load, retry);
        let keys = [load, empty, banner, title, retry];
        let unique = keys.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), keys.len());
    }

    #[test]
    fn same_title_instances_keep_distinct_identities() {
        let first = text_key("list-empty", "state", "title", "Could not load");
        let second = text_key("list-error", "state", "title", "Could not load");
        assert_ne!(first, second);
        assert_eq!(first, "feedback-list-empty-state-title-Could not load");
        assert_eq!(second, "feedback-list-error-state-title-Could not load");
    }

    struct TwoBanners;

    impl Render for TwoBanners {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let tokens = Tokens::one_dark();
            let none = std::iter::empty::<Button>();
            v_flex()
                .child(TextSelectionLayer)
                .child(
                    v_flex()
                        .id("banner-a")
                        .test_support()
                        .flex_none()
                        .w(px(640.))
                        .child(error_banner_with_actions(
                            &tokens,
                            "feedback-a",
                            "Could not load",
                            "ALPHA unique selectable message",
                            none.clone(),
                        )),
                )
                .child(
                    v_flex()
                        .id("banner-b")
                        .test_support()
                        .flex_none()
                        .w(px(640.))
                        .child(error_banner_with_actions(
                            &tokens,
                            "feedback-b",
                            "Could not load",
                            "BRAVO unique selectable message",
                            none,
                        )),
                )
        }
    }

    fn drag_across(window: &mut Window, cx: &mut App, bounds: Bounds<Pixels>) {
        let y = bounds.origin.y + bounds.size.height / 2.;
        let start = point(bounds.origin.x + bounds.size.width * 0.20, y);
        let end = point(bounds.origin.x + bounds.size.width * 0.70, y);
        window.drag(start, end, cx);
    }

    fn copied_selection(window: &mut Window, cx: &mut App) -> String {
        let selected = TextSelection::selected_text(window, cx);
        links::copy(selected.clone(), cx);
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text().map(|text| text.to_string()));
        assert_eq!(copied.as_deref(), Some(selected.as_str()));
        selected
    }

    #[gpui_kit::test]
    fn same_title_banners_select_and_copy_distinct_text(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| TwoBanners);
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            drag_across(window, cx, window.find("banner-a").bounds());
            let first_text = copied_selection(window, cx);
            assert!(first_text.contains("ALPHA"), "first {first_text:?}");
            assert!(!first_text.contains("BRAVO"), "first {first_text:?}");
            window.render_frame(cx);
            let second_bounds = window.find("banner-b").bounds();
            drag_across(window, cx, second_bounds);
            let second_text = copied_selection(window, cx);
            assert!(
                second_text.contains("BRAVO"),
                "second {second_text:?} bounds={second_bounds:?}"
            );
            assert!(!second_text.contains("ALPHA"), "second {second_text:?}");
        });
    }
}
