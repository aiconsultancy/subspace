//! A part read in flight holds a page of room until it arrives, and an
//! item that shrank under a part read starts again from its first part,
//! with **Try again** reading page 0, never the old index.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};

use super::super::super::super::remove::settle;
use super::{
    Irc, PAGE, changed_under_parts, drawn, held_bytes, in_parts, on, served_json, set_budget, tool,
};

fn held(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        held_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

fn click(id: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(SharedString::from(on(id)), cx));
}

/// Captain's ruling on round 4, point 2, and SUB decisions `8357ec55` §2
/// and `33ccecf6` item 1: while a part is read the item counts the part
/// shown and its drawn window plus a whole page, then the new part and its
/// window alone.
#[gpui_kit::test]
fn a_part_read_holds_a_page_of_room_until_it_arrives(cx: &mut TestAppContext) {
    let (board, served, cx) = in_parts(cx, &"x".repeat(200_000));
    let (json, pages) = served_json(&served);
    // Room for the page coming in beside the part shown and its window.
    board.update(cx, |board, _| {
        set_budget(board.inspector_activity.as_mut().unwrap(), 4 * PAGE as u64);
    });
    let shown = held(&board, cx);
    click("page-last", cx);
    assert_eq!(
        held(&board, cx),
        shown + PAGE as u64,
        "the part shown and a page held for the read"
    );
    settle(cx);
    let last = json.len() - (pages - 1) * PAGE;
    assert_eq!(
        held(&board, cx),
        shown - 2 * PAGE as u64 + 2 * last as u64,
        "the new part and its window"
    );
}

/// Captain's ruling on round 4, point 3: a part past the last of an item
/// that shrank reads page 0 once and says a newer version is there; when
/// that read fails, **Try again** reads page 0, not the old index.
#[gpui_kit::test]
fn an_item_that_shrank_starts_again_from_its_first_part(cx: &mut TestAppContext) {
    let (served, cx, (json, pages)) =
        changed_under_parts(cx, (&"x".repeat(300_000), &"y".repeat(100_000)));
    click("page-last", cx);
    settle(cx);
    let asked = served.asked();
    assert!(asked[asked.len() - 2].contains("page=4"), "{asked:?}");
    assert!(asked[asked.len() - 1].contains("page=0"), "{asked:?}");
    assert!(drawn(&on("newer"), cx).starts_with("A newer version is on the gateway"));
    assert_eq!(drawn(&on("page"), cx), format!("Part 1 of {pages}"));
    assert_eq!(drawn(&on("json-part"), cx), &json[..PAGE]);

    // Shrinks again, and page 0 is busy: the failure names page 0.
    served.serve(8, tool(8, "z"));
    *served.busy_first.lock().unwrap() = true;
    click("page-next", cx);
    settle(cx);
    assert!(drawn(&on("page-failed"), cx).starts_with("Stargate is busy"));
    *served.busy_first.lock().unwrap() = false;
    let before = served.asked().len();
    click("page-retry", cx);
    settle(cx);
    let asked = served.asked();
    assert_eq!(asked.len(), before + 1, "{asked:?}");
    assert!(asked[before].contains("page=0"), "{asked:?}");
    assert!(drawn(&on("page-failed"), cx).is_empty());
    assert_eq!(drawn(&on("page"), cx), "Part 1 of 1");
}
