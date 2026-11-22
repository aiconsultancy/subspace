//! Link activation beside a text selection: one ordinary click on
//! a link follows it even when text elsewhere is still selected, while a
//! drag that selects across a link does not.

use gpui_kit::base::TextSelection;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext};

use super::{bounds, column, drag, privmsgs, row_ids, shown, text_end, text_start, turns};
use crate::fixtures::channel;

const URL: &str = "https://example.org/x";
const MARKDOWN: &str = "[docs](https://example.org/x) *here*";

/// A plain row, then a row whose text starts with a link.
fn rows(cx: &mut TestAppContext) -> (Vec<Point<Pixels>>, &mut VisualTestContext) {
    rows_with(cx, URL)
}

/// A plain row, then a row showing `linked`, whose text starts with a link.
/// The points are the plain row's text start and end, the link, and two
/// points after the link in its own row.
fn rows_with<'a>(
    cx: &'a mut TestAppContext,
    linked: &'static str,
) -> (Vec<Point<Pixels>>, &'a mut VisualTestContext) {
    let room = channel("#links");
    let lines = turns(&room, 1, &privmsgs(&["alpha bravo charlie", linked]));
    let (hub, cx) = shown(cx, &room, lines, Vec::new());
    let ids = row_ids(&hub, &room, cx);
    let column = column(&hub, &room, cx);
    let plain = bounds(&ids[0], cx);
    let linked = bounds(&ids[1], cx);
    let points = vec![
        text_start(plain, column),
        text_end(plain),
        text_start(linked, column) + gpui_kit::point(gpui_kit::px(4.), gpui_kit::px(0.)),
        text_start(linked, column) + gpui_kit::point(gpui_kit::px(44.), gpui_kit::px(0.)),
        text_start(linked, column) + gpui_kit::point(gpui_kit::px(74.), gpui_kit::px(0.)),
    ];
    (points, cx)
}

fn click(at: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

/// Opens the text menu at `at` and chooses its item `index`, `label`.
fn choose(at: Point<Pixels>, index: usize, label: &str, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert_eq!(menu.find(index).label(), Some(label));
        menu.click(index, cx);
    });
    cx.run_until_parked();
}

fn selected(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        TextSelection::selected_text(window, cx)
    })
}

#[gpui_kit::test]
fn a_click_on_a_link_follows_it_with_a_selection_elsewhere(cx: &mut TestAppContext) {
    let (points, cx) = rows(cx);
    let copied = drag(points[0], points[1], cx);
    assert_eq!(copied.trim_end_matches('\n'), "alpha bravo charlie");
    click(points[2], cx);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui_kit::test]
fn a_click_on_a_link_follows_it_after_a_double_click_selection(cx: &mut TestAppContext) {
    let (points, cx) = rows(cx);
    // A double click selects a word, which a later click on a link clears.
    cx.simulate_mouse_down(points[0], MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(points[0], MouseButton::Left, Modifiers::default());
    cx.simulate_event(gpui_kit::MouseDownEvent {
        button: MouseButton::Left,
        position: points[0],
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        button: MouseButton::Left,
        position: points[0],
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    cx.run_until_parked();
    assert!(!selected(cx).is_empty(), "the double click selects a word");
    click(points[2], cx);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui_kit::test]
fn a_drag_across_a_link_selects_without_following_it(cx: &mut TestAppContext) {
    let (points, cx) = rows(cx);
    let copied = drag(points[0], points[2], cx);
    assert!(!copied.is_empty(), "the drag selects text");
    assert_eq!(cx.opened_url(), None);
}

#[gpui_kit::test]
fn a_click_on_a_markdown_link_follows_it_with_a_selection_elsewhere(cx: &mut TestAppContext) {
    let (points, cx) = rows_with(cx, MARKDOWN);
    let copied = drag(points[0], points[1], cx);
    assert_eq!(copied.trim_end_matches('\n'), "alpha bravo charlie");
    click(points[2], cx);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui_kit::test]
fn a_drag_across_a_markdown_link_selects_without_following_it(cx: &mut TestAppContext) {
    let (points, cx) = rows_with(cx, MARKDOWN);
    let copied = drag(points[0], points[2], cx);
    assert!(!copied.is_empty(), "the drag selects text");
    assert_eq!(cx.opened_url(), None);
}

/// Where a test leaves its stale selection: the plain row, for a plain link,
/// whose trigger refuses while anything in the window is selected; or after
/// the link in its own row, for a Markdown link, whose Kit handler refuses
/// while its own text is selected.
const ELSEWHERE: (usize, usize) = (0, 1);
const OWN_ROW: (usize, usize) = (3, 4);

/// Chooses Select all where `stale` starts, then clicks the link in a row
/// showing `linked`, and returns the URL that opened.
///
/// The release of the menu item's click never reaches Kit's window
/// selection, so it never saw that press end. The next press then kept the
/// stale selection and the link refused to open.
fn click_after_select_all(
    cx: &mut TestAppContext,
    linked: &'static str,
    stale: (usize, usize),
) -> Option<String> {
    let (points, cx) = rows_with(cx, linked);
    choose(points[stale.0], 1, "Select all", cx);
    assert!(!selected(cx).is_empty(), "Select all selects text");
    click(points[2], cx);
    cx.opened_url()
}

/// Drags over `stale` without a release, as when it ends outside the
/// window, then clicks the link in a row showing `linked`, and returns the
/// URL that opened.
fn click_after_a_release_outside(
    cx: &mut TestAppContext,
    linked: &'static str,
    stale: (usize, usize),
) -> Option<String> {
    let (points, cx) = rows_with(cx, linked);
    cx.simulate_mouse_down(points[stale.0], MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        points[stale.1],
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert!(!selected(cx).is_empty(), "the drag selects text");
    click(points[2], cx);
    cx.opened_url()
}

#[gpui_kit::test]
#[ignore = "needs the selection-latch fix GPUI Kit 0.9.1 lacks; see docs/changes/subspace-api-accounts"]
fn a_click_on_a_link_follows_it_after_select_all_elsewhere(cx: &mut TestAppContext) {
    assert_eq!(
        click_after_select_all(cx, URL, ELSEWHERE).as_deref(),
        Some(URL)
    );
}

#[gpui_kit::test]
#[ignore = "needs the selection-latch fix GPUI Kit 0.9.1 lacks; see docs/changes/subspace-api-accounts"]
fn a_click_on_a_markdown_link_follows_it_after_select_all_in_its_row(cx: &mut TestAppContext) {
    assert_eq!(
        click_after_select_all(cx, MARKDOWN, OWN_ROW).as_deref(),
        Some(URL)
    );
}

#[gpui_kit::test]
fn a_click_on_a_link_follows_it_after_copying_a_selection(cx: &mut TestAppContext) {
    let (points, cx) = rows(cx);
    let copied = drag(points[0], points[1], cx);
    assert_eq!(copied.trim_end_matches('\n'), "alpha bravo charlie");
    choose(points[0], 0, "Copy", cx);
    click(points[2], cx);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui_kit::test]
#[ignore = "needs the selection-latch fix GPUI Kit 0.9.1 lacks; see docs/changes/subspace-api-accounts"]
fn a_click_on_a_link_follows_it_after_a_drag_released_outside_the_window(cx: &mut TestAppContext) {
    assert_eq!(
        click_after_a_release_outside(cx, URL, ELSEWHERE).as_deref(),
        Some(URL)
    );
}

#[gpui_kit::test]
#[ignore = "needs the selection-latch fix GPUI Kit 0.9.1 lacks; see docs/changes/subspace-api-accounts"]
fn a_click_on_a_markdown_link_follows_it_after_a_drag_in_its_row_released_outside(
    cx: &mut TestAppContext,
) {
    assert_eq!(
        click_after_a_release_outside(cx, MARKDOWN, OWN_ROW).as_deref(),
        Some(URL)
    );
}

#[gpui_kit::test]
fn a_drag_over_an_issue_chip_with_its_state_dot_copies_the_reference_as_written(
    cx: &mut TestAppContext,
) {
    let (_, hub, _, cx) = crate::tests::issues::open_board(cx, true, "TST-12 blocks TST-13");
    let dots = cx.update(|window, cx| {
        window.render_frame(cx);
        gpui_kit::base::test_support::snapshots(window)
            .into_iter()
            .filter(|node| {
                node.path()
                    .last()
                    .is_some_and(|id| id.to_string().starts_with("irc-reference-state:"))
            })
            .count()
    });
    assert_eq!(dots, 1, "the answered reference shows its state");
    let copied = super::drag_rows(&hub, &channel("#subspace"), (1, 2), cx);
    assert_eq!(
        copied,
        super::copied_rows(&["rocket ready?", "TST-12 blocks TST-13"])
    );
}
