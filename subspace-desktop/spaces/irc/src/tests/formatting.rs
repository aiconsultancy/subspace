//! Slack-style formatting in the transcript: rows read the text as shown,
//! the shared rich text shows it without its markers, and literal markers,
//! mIRC codes and mentions keep working.

use gpui_kit::base::{TextSelection, TextSelectionLayer};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, IntoElement, Modifiers, MouseButton, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, point, px,
};
use subspace_client_irc::Event;
use subspace_framework::dashboard::rich;

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{channel, message};
use crate::markup::{self, Mentions};
use crate::view;

/// The rows' accessible names after alice sends `texts`, one a minute
/// apart, after the fixture channel's first two lines.
fn spoken(cx: &mut TestAppContext, texts: &[&str]) -> Vec<String> {
    let room = channel("#subspace");
    let mut events = channel_events();
    let first = events.len() as i64;
    for (minute, text) in texts.iter().enumerate() {
        let minute = first + i64::try_from(minute).unwrap_or_default();
        events.push(Event::Line(message(&room, "alice", text, minute)));
    }
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    let first_seq = 2;
    cx.update(|window, _| {
        (0..texts.len())
            .map(|index| {
                let row = window.find(format!("irc-line:{}:row", first_seq + index));
                row.label()
                    .map(|label| label.to_string())
                    .unwrap_or_default()
            })
            .collect()
    })
}

#[gpui_kit::test]
fn formatted_messages_read_as_shown(cx: &mut TestAppContext) {
    let spoken = spoken(
        cx,
        &[
            "*bold* _italic_ ~gone~ `code`",
            "> quoted *line*",
            "[docs](https://example.org) and https://example.org/a_b",
            "```npm install\nnpm test```",
        ],
    );
    assert_eq!(
        spoken,
        [
            "alice: bold italic gone code",
            "alice: quoted line",
            "alice: docs and https://example.org/a_b",
            "alice: npm install\nnpm test",
        ]
    );
}

#[gpui_kit::test]
fn literal_markers_and_mirc_codes_read_as_typed(cx: &mut TestAppContext) {
    let spoken = spoken(
        cx,
        &[
            "snake_case_names and 2*3*4 and src/*.rs",
            "\u{2}loud\u{2} and *star*",
        ],
    );
    assert_eq!(
        spoken,
        [
            "alice: snake_case_names and 2*3*4 and src/*.rs",
            "alice: loud and *star*",
        ]
    );
}

#[gpui_kit::test]
fn formatted_highlight_words_still_highlight(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(Event::Line(message(&room, "alice", "*rocket* at noon", 3)));
    events.push(Event::Line(message(&room, "alice", "~rocket~ scrubbed", 4)));
    let (_, hub, cx) = mount(cx, settings());
    inject(&hub, events, cx);
    hub.read_with(cx, |hub, _| {
        let buffer = hub.state.buffer(&room).expect("the fixture channel");
        assert_eq!(
            buffer.highlights, 3,
            "bob's line and both formatted mentions of the highlight word"
        );
    });
}

/// The shared rich text showing one message rewritten by the board, with
/// the selection layer that copy reads.
struct Shown {
    markdown: String,
}

impl Render for Shown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(TextSelectionLayer)
            .child(
                div()
                    .w(px(640.))
                    .h(px(100.))
                    .child(rich::rich_text_prepared(
                        "irc-shown",
                        self.markdown.clone(),
                        |_, _, _| false,
                    )),
            )
    }
}

/// The text a drag across the whole message selects.
fn selected(cx: &mut VisualTestContext) -> String {
    cx.simulate_mouse_down(
        point(px(1.), px(4.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(630.), px(90.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(630.), px(90.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        TextSelection::selected_text(window, cx)
    })
}

#[gpui_kit::test]
fn rich_text_shows_formatting_without_markers_and_code_keeps_its_tags(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let rendered = markup::rich(
        "*bold* _italic_ ~gone~ and `<script>` stays",
        &Mentions::default(),
    )
    .expect("formatted text");
    let (_, cx) = cx.add_window_view(|_, _| Shown {
        markdown: rendered.markdown,
    });
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    assert_eq!(
        selected(cx).trim_end(),
        "bold italic gone and <script> stays"
    );
}
