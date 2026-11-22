//! The slim row under the composer that names its keys with Kit's keyboard
//! key: `@` to mention, `/` for commands, Up to recall a sent line (IRC
//! cannot edit a sent message) and Shift with Return for a new line. While
//! uploads are on, who can see what is sent ends the row, said shortly.

use gpui_kit::component::h_flex;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, Keystroke, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Tokens;

/// Who can see what is sent, as the hint row says it.
pub(crate) const RETENTION: &str = "Kept 30 days. Don’t paste credentials.";

/// Each key and what it does, as the row shows them.
pub(crate) const HINTS: [(&str, &str); 4] = [
    ("@", "mention"),
    ("/", "commands"),
    ("up", "recall"),
    ("shift-enter", "new line"),
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
        .children(HINTS.iter().filter_map(|(key, word)| hint(key, word)))
        .children(uploads.then(|| retention(&tokens)))
        .into_any_element()
}

/// One key and its word, read as `<word>: <key>`.
fn hint(key: &'static str, word: &'static str) -> Option<AnyElement> {
    let stroke = Keystroke::parse(key).ok()?;
    Some(
        h_flex()
            .id(SharedString::from(format!("irc-hint:{word}")))
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
        .into_any_element()
}
