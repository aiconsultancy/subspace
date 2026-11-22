//! The slim row under the composer that names its keys with Kit's keyboard
//! key: `@` to mention, `/` for commands, Up to recall a sent line (IRC
//! cannot edit a sent message) and Shift with Return for a new line, each
//! named `<word>: <key>`. While uploads are on, who can see what is sent and
//! for how long ends the row, said shortly; hovering it says it in full.

use gpui_kit::component::h_flex;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, Keystroke, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;

/// Who can see what is sent and for how long, as the hint row says it:
/// short enough for the slim row, which truncates it in a narrow pane; the
/// full sentence is its accessible name and its tooltip.
pub(crate) const RETENTION: &str = "Subspace users see it for 30 days. No credentials.";

/// Each key, what it does and the key in words, as the row shows them.
pub(crate) const HINTS: [(&str, &str, &str); 4] = [
    ("@", "mention", "at sign"),
    ("/", "commands", "slash"),
    ("up", "recall", "Up arrow"),
    ("shift-enter", "new line", "Shift+Return"),
];

/// The hint row: each key and its word, then the retention notice when
/// `uploads` are on.
pub(super) fn row(uploads: bool, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    h_flex()
        .id("irc-composer-hints")
        .test_support()
        .w_full()
        .min_w_0()
        .gap_3()
        .px_2()
        .pb_1()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .children(HINTS.iter().filter_map(|hint_keys| hint(*hint_keys)))
        .children(uploads.then(|| retention(&tokens)))
        .into_any_element()
}

/// One key and its word, read as `<word>: <key>`.
fn hint((key, word, spoken): (&'static str, &'static str, &'static str)) -> Option<AnyElement> {
    let stroke = Keystroke::parse(key).ok()?;
    Some(
        h_flex()
            .id(SharedString::from(format!("irc-hint:{word}")))
            .aria_label(format!("{word}: {spoken}"))
            .test_support()
            .flex_shrink_0()
            .gap_1()
            .child(Kbd::new(stroke))
            .child(word)
            .into_any_element(),
    )
}

/// The retention notice at the row's trailing end, named in full.
fn retention(tokens: &Tokens) -> AnyElement {
    div()
        .id("irc-retention")
        .aria_label(crate::uploads::outgoing::VISIBILITY)
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .justify_end()
        .text_color(tokens.muted)
        .child(div().truncate().child(RETENTION))
        .tooltip(|window, cx| Tooltip::new(crate::uploads::outgoing::VISIBILITY).build(window, cx))
        .into_any_element()
}
