//! One budget for everything the view holds: an item larger than it
//! shows one page at a time, each part pinned to its version, a late
//! reply is dropped, and the budget holds during a read as after it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};

use super::super::super::super::remove::settle;
use super::{
    Error, Irc, PAGE, answer, drawn, held_bytes, label, late_reply, load, loaded_keys, on, paged,
    read_of, refresh, running, served, set_budget, tool,
};

/// The view's budget set to `budget`.
fn budgeted(board: &Entity<Irc>, budget: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), budget);
    });
}

/// The bytes the view holds of the captain's activity.
fn held(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        held_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

/// The paged card shows part `part` of `pages`, holding `json`.
fn shows_part(cx: &mut VisualTestContext, (part, pages): (usize, usize), json: &str) {
    assert_eq!(drawn(&on("page"), cx), format!("Part {part} of {pages}"));
    assert_eq!(drawn(&on("json-part"), cx), json);
}

/// SUB decision `a17870f4`: an item larger than the view can hold shows
/// one page of its JSON at a time, the first and last in reach, and the
/// view never holds more than its budget.
#[gpui_kit::test]
fn an_item_over_the_budget_shows_page_by_page_within_it(cx: &mut TestAppContext) {
    let output = "x".repeat(200_000);
    let (board, served, cx) = served(cx, 6, &output);
    let budget = 3 * PAGE as u64;
    budgeted(&board, budget, cx);
    load(cx);
    let json = served.item.lock().unwrap().1.clone();
    let pages = json.len().div_ceil(PAGE);
    assert_eq!(
        served.asked().len(),
        1,
        "page 0 alone: {:?}",
        served.asked()
    );
    assert!(drawn(&on("parts"), cx).starts_with("Too large to hold whole"));
    shows_part(cx, (1, pages), &json[..PAGE]);
    assert!(held(&board, cx) <= budget);

    cx.update(|window, cx| window.click(SharedString::from(on("page-last")), cx));
    settle(cx);
    // The last bytes in reach.
    shows_part(cx, (pages, pages), &json[(pages - 1) * PAGE..]);
    assert!(
        served
            .asked()
            .last()
            .unwrap()
            .contains(&format!("page={}", pages - 1)),
        "{:?}",
        served.asked()
    );
    assert!(held(&board, cx) <= budget);

    cx.update(|window, cx| window.click(SharedString::from(on("page-first")), cx));
    settle(cx);
    shows_part(cx, (1, pages), &json[..PAGE]);
    assert!(drawn(&on("newer"), cx).is_empty());
    assert!(held(&board, cx) <= budget);
}

/// SUB decision `a17870f4`: each part is pinned to the version it began
/// with; once the item changed, it starts again from the first part of
/// the newest version and says so.
#[gpui_kit::test]
fn a_part_of_a_changed_item_starts_again_and_says_a_newer_version_is_there(
    cx: &mut TestAppContext,
) {
    let (board, served, cx) = served(cx, 6, &"x".repeat(200_000));
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), 3 * PAGE as u64);
    });
    load(cx);
    served.serve(7, tool(7, &"y".repeat(300_000)));
    let json = served.item.lock().unwrap().1.clone();
    let pages = json.len().div_ceil(PAGE);
    cx.update(|window, cx| window.click(SharedString::from(on("page-next")), cx));
    settle(cx);
    assert!(drawn(&on("newer"), cx).starts_with("A newer version is on the gateway"),);
    shows_part(cx, (1, pages), &json[..PAGE]);
    let asked = served.asked();
    assert!(asked[asked.len() - 2].contains("page=1") && asked[asked.len() - 1].contains("page=0"));
}

/// SUB decision `a17870f4`: a reply that arrives after its item was let
/// go, or after the item was read again, is dropped.
#[gpui_kit::test]
fn a_late_reply_is_dropped(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "FIRST-READ");
    load(cx);
    let first = board.read_with(cx, |board, _| {
        read_of(board.inspector_activity.as_ref().unwrap(), "t-now", "i3").unwrap()
    });
    // No room for any item: the next snapshot lets it go.
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), 0);
    });
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(6, bytes))), cx);
    let keys = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            loaded_keys(board.inspector_activity.as_ref().unwrap())
        })
    };
    assert!(keys(&board, cx).is_empty(), "let go to make room");
    board.update(cx, |board, cx| {
        late_reply(board, ("t-now", "i3", first), Err(Error::NotFound), cx);
    });
    settle(cx);
    assert!(keys(&board, cx).is_empty(), "a reply after its item went");
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load in full"));

    // Read again: the first read's reply is no answer to the second.
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), u64::MAX);
    });
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("FIRST-READ"));
    board.update(cx, |board, cx| {
        late_reply(board, ("t-now", "i3", first), Err(Error::NotFound), cx);
    });
    settle(cx);
    assert!(drawn(&on("failed"), cx).is_empty());
    assert!(drawn(&on("out:0"), cx).ends_with("FIRST-READ"));
}

/// Captain's ruling on round 3, point 5: the budget holds during a read
/// too. Room is made before the read, and an item whose page 0 claims
/// more than that room comes back alone; once room is made for the size
/// it claims, it is read whole.
#[gpui_kit::test]
fn an_item_that_grew_past_its_room_is_read_whole_once_room_is_made(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "short");
    let budget = 8 * PAGE as u64;
    budgeted(&board, budget, cx);
    // The snapshot still names the short version.
    served.serve(7, tool(7, &format!("{}GROWN-END", "g".repeat(200_000))));
    let json = served.item.lock().unwrap().1.clone();
    let pages = json.len().div_ceil(PAGE);
    let inline = held(&board, cx);
    cx.update(|window, cx| window.click(SharedString::from(on("load")), cx));
    assert_eq!(
        held(&board, cx) - inline,
        PAGE as u64,
        "room for page 0 while read"
    );
    settle(cx);
    let asked = served.asked();
    assert_eq!(
        asked.len(),
        1 + pages,
        "page 0 again, then the rest: {asked:?}"
    );
    assert!(asked[0].contains("page=0") && asked[1].contains("page=0"));
    assert!(drawn(&on("out:0"), cx).ends_with("GROWN-END"));
    assert_eq!(held(&board, cx) - inline, json.len() as u64);
    assert!(held(&board, cx) <= budget);
}
