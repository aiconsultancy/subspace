//! Keyboard access to pull request chips: Tab reaches a
//! chip in a formatted message as it does in plain text, the focused chip
//! shows its card, Enter opens it as a click does and Escape leaves. Pull
//! requests in code are not chips, so Tab never stops on them. Tab walks a
//! message's links and references in the order they are read, chips in
//! formatted text included.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::contracts::github::ResolutionState;

use super::interaction::watch;
use super::{Prs, URL, answer, chips, details, open_board};
use crate::conversation::{self, Conversation};
use crate::fixtures::channel;
use crate::tests::open_with_message;
use crate::{Irc, pull_requests};

const TOOLTIP: &str = "irc-pr-tooltip";
const CANONICAL: &str = "https://github.com/octo-org/api/pull/421";
const LINK: &str = "https://example.org";
/// [`LINK`] as the link policy opens it.
const OPENED: &str = "https://example.org/";
const ISSUE: &str = "https://api.example.test/api/v1/issues/TST-12?profile=dev";

/// The message's conversation, with the keys bound and the message input
/// focused, as after a click in it.
fn keyboard(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Entity<Conversation> {
    cx.update(|window, cx| {
        subspace_framework::dashboard::chat_input::bind_keys(cx);
        conversation::bind_keys(cx);
        let conversation = board.read(cx).dock.open[&channel("#subspace")]
            .entity
            .clone();
        let input = conversation.read(cx).composer().clone();
        input.update(cx, |input, cx| input.focus(window, cx));
        conversation
    })
}

fn press(keys: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

/// The focused message's focused link or reference, by index, and whether
/// a message has focus at all.
fn focus(conversation: &Entity<Conversation>, cx: &mut VisualTestContext) -> Option<Option<usize>> {
    conversation
        .read_with(cx, |conversation, _| conversation::focused(conversation))
        .map(|at| at.reference)
}

fn has_card(conversation: &Entity<Conversation>, cx: &mut VisualTestContext) -> bool {
    conversation.read_with(cx, |conversation, _| conversation::has_card(conversation))
}

/// Moves from the message input to the newest message's first link or
/// reference, which must be a pull request chip showing its card.
fn tab_to_the_chip(conversation: &Entity<Conversation>, cx: &mut VisualTestContext) {
    press("shift-tab tab", cx);
    assert_eq!(focus(conversation, cx), Some(Some(0)), "the chip has focus");
    assert!(has_card(conversation, cx), "a focused chip shows its card");
    assert!(cx.update(|window, _| window.find(TOOLTIP).visible()));
}

/// What Enter on the focused chip opens: the pull request in the running
/// PRs Space, or else the link in the browser.
fn enter_opens(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Option<String> {
    let (asked, _subscription) = watch(board, cx);
    press("enter", cx);
    let navigated = asked.borrow().navigated.clone();
    navigated.or_else(|| cx.opened_url())
}

#[gpui_kit::test]
fn tab_reaches_a_chip_in_formatted_text_and_escape_leaves_it(cx: &mut TestAppContext) {
    let (board, _, prs, cx) = open_board(cx, Prs::Running, "*[PR:octo-org/api#421]*");
    answer(
        &prs.expect("a PRs fixture"),
        ResolutionState::Available(details()),
        cx,
    );
    let conversation = keyboard(&board, cx);
    tab_to_the_chip(&conversation, cx);
    let card = crate::tests::tooltip_text(TOOLTIP, cx);
    assert!(
        card.contains("Keep pull request context beside the conversation"),
        "{card}"
    );
    assert_eq!(enter_opens(&board, cx).as_deref(), Some(CANONICAL));
    press("escape", cx);
    assert_eq!(focus(&conversation, cx), None, "back in the message input");
    assert!(!has_card(&conversation, cx));
    cx.update(|window, _| assert!(window.find_all(TOOLTIP).is_empty()));
}

/// Tabs to the chip of formatted message `text` with no PRs Space, and
/// checks that Enter opens [`URL`] as written in the browser.
fn enter_opens_the_url_as_written(text: &str, cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, Prs::Absent, text);
    let conversation = keyboard(&board, cx);
    tab_to_the_chip(&conversation, cx);
    assert_eq!(enter_opens(&board, cx).as_deref(), Some(URL));
}

#[gpui_kit::test]
fn enter_on_a_formatted_pull_request_url_opens_it_as_written(cx: &mut TestAppContext) {
    enter_opens_the_url_as_written(&format!("**review** *{URL}*"), cx);
}

#[gpui_kit::test]
fn enter_on_a_formatted_marker_labelled_link_opens_its_url(cx: &mut TestAppContext) {
    enter_opens_the_url_as_written(&format!("*[PR:octo-org/api#421]({URL})*"), cx);
}

#[gpui_kit::test]
fn tab_skips_pull_requests_in_code_spans_and_fenced_code(cx: &mut TestAppContext) {
    let text = "*see* `was [PR:octo-org/web#7] then` and [PR:octo-org/api#421]\n\
                ```\n[PR:octo-org/web#8]\n```";
    let (board, _, _, cx) = open_board(cx, Prs::Running, text);
    let conversation = keyboard(&board, cx);
    tab_to_the_chip(&conversation, cx);
    press("tab tab", cx);
    assert_eq!(
        focus(&conversation, cx),
        Some(Some(0)),
        "the chip outside code is the only target"
    );
    assert_eq!(enter_opens(&board, cx).as_deref(), Some(CANONICAL));
}

/// Where `Enter` goes at each stop of the newest message, by `Tab` from the
/// message to its last stop, then by `Shift+Tab` back to its first.
fn stops(
    board: &Entity<Irc>,
    count: usize,
    cx: &mut VisualTestContext,
) -> (Vec<String>, Vec<String>) {
    let conversation = keyboard(board, cx);
    press("shift-tab", cx);
    let at = |index: usize, key: &str, cx: &mut VisualTestContext| {
        press(key, cx);
        assert_eq!(
            focus(&conversation, cx),
            Some(Some(index)),
            "{key} to stop {index}"
        );
        enter_opens(board, cx).unwrap_or_default()
    };
    let forward = (0..count).map(|index| at(index, "tab", cx)).collect();
    let back = (0..count - 1)
        .rev()
        .map(|index| at(index, "shift-tab", cx))
        .collect();
    (forward, back)
}

/// Where `Enter` goes at the stops of the two-stop message `text`, Tab
/// forward then Shift+Tab back to the first: `expected`, then its first.
fn two_stops_open(text: &str, expected: [&str; 2], cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, Prs::Running, text);
    let (forward, back) = stops(&board, 2, cx);
    assert_eq!(forward, expected);
    assert_eq!(back, [expected[0]]);
}

#[gpui_kit::test]
fn tab_reaches_a_link_before_a_later_formatted_chip(cx: &mut TestAppContext) {
    two_stops_open(
        &format!("see {LINK} then *[PR:octo-org/api#421]*"),
        [OPENED, CANONICAL],
        cx,
    );
}

#[gpui_kit::test]
fn tab_reaches_a_formatted_chip_before_a_later_link(cx: &mut TestAppContext) {
    two_stops_open(
        &format!("*[PR:octo-org/api#421]* then {LINK}"),
        [CANONICAL, OPENED],
        cx,
    );
}

#[gpui_kit::test]
fn tab_walks_an_issue_a_link_and_a_formatted_chip_in_reading_order(cx: &mut TestAppContext) {
    let text = format!("TST-12 blocks {LINK}, see *[PR:octo-org/api#421]({URL})*");
    let (board, _, _, cx) = open_with_message(
        cx,
        |cx| {
            let (hub, tracker) = crate::tests::issues::install(true, cx);
            pull_requests::attach(&hub, cx);
            (hub, tracker)
        },
        &text,
    );
    let (forward, back) = stops(&board, 3, cx);
    assert_eq!(forward, [ISSUE, OPENED, URL]);
    assert_eq!(back, [OPENED, ISSUE]);
}

#[gpui_kit::test]
fn tab_reaches_a_formatted_chip_then_the_attachment_of_the_same_message(cx: &mut TestAppContext) {
    use crate::tests::uploads::received::{cache, opened, upload};
    use crate::uploads::cache::Card;
    let file = upload("plan.md");
    let key = file.reference.clone();
    let text = format!("*[PR:octo-org/api#421]* and {key} plan.md (4 bytes)");
    let (board, hub, _, cx) = open_board(cx, Prs::Running, &text);
    let ready = Card::Ready(std::sync::Arc::new(file), None);
    cache(&hub, vec![(key.clone(), ready)], cx);
    let conversation = keyboard(&board, cx);
    assert_eq!(chips(cx).len(), 1, "the chip is drawn");
    let thumbnail = gpui_kit::SharedString::from(format!("chat-upload:{key}"));
    assert!(cx.update(|window, _| window.try_find(thumbnail).is_some()));
    press("shift-tab tab", cx);
    assert_eq!(focus(&conversation, cx), Some(Some(0)), "the chip first");
    press("tab", cx);
    assert_eq!(
        focus(&conversation, cx),
        Some(Some(1)),
        "then the attachment"
    );
    let on = conversation.read_with(cx, conversation::focused_upload);
    assert_eq!(on.as_deref(), Some(key.as_str()));
    press("enter", cx);
    assert_eq!(opened(&conversation, cx).as_deref(), Some(key.as_str()));
    press("escape", cx);
    assert_eq!(opened(&conversation, cx), None);
    press("shift-tab", cx);
    assert_eq!(focus(&conversation, cx), Some(Some(0)), "back on the chip");
    assert_eq!(enter_opens(&board, cx).as_deref(), Some(CANONICAL));
    assert_eq!(
        opened(&conversation, cx),
        None,
        "the chip never opens the viewer"
    );
}
