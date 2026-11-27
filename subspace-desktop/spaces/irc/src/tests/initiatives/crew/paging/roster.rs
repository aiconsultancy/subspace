//! Rendered roster previews: cut marks, route fallback and full-text recovery.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext, Window, px, size};

use super::{
    ROW, install_fleet_api, live, plain_note, reveal_in_inspector, set_fleet, settle, ship, view,
};
use crate::tests::remove::choose;

#[gpui_kit::test]
fn a_missing_crew_route_shows_neutral_capped_roster_notes(cx: &mut TestAppContext) {
    use gpui_kit::base::test_support::texts;
    use subspace_client_fleet::Error;

    let (board, hub, cx) = ship(cx);
    let mut member = crate::live::tests::snapshot().ships["aurora-lyra"]
        .crew_members
        .iter()
        .find(|member| member.id == "wren")
        .unwrap()
        .clone();
    let task = member.contexts[0].current.as_mut().unwrap();
    task.summary = "s".repeat(180);
    task.progress = "p".repeat(390) + " Done now.";
    let summary = task.summary.clone();
    let progress = task.progress.clone();
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.simulate_resize(size(px(1280.), px(960.)));
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(ROW, cx);
        board.update(cx, |board, cx| {
            view::inspector::crew::seed_roster_for_test(board, member, cx);
            view::inspector::crew::finish_crew_read_for_test(board, Err(Error::NotFound), cx);
        });
        window.render_frame(cx);
        let drawn = texts(window);
        let index = |value: &str| drawn.iter().position(|text| text.text() == value).unwrap();
        assert_eq!(
            drawn[index(&summary) + 1].text(),
            "…",
            "summary marker follows the head"
        );
        assert!(
            window
                .try_find(plain_note("irc-crew-preview:named:nav:summary"))
                .is_some()
        );
        assert_tail_preview(window, &progress);
        assert!(window.try_find("irc-crew-retry").is_none());
    });
    assert_eq!(
        copy_note("irc-crew-preview:named:nav:progress", cx),
        "Showing the latest 400 characters from the ship's roster; this may be incomplete."
    );
    assert!(
        copy_note("irc-crew-note", cx)
            .starts_with("Crew detail is unavailable. Showing the ship's roster data.")
    );
}

fn copy_note(id: &str, cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        window.render_frame(cx);
        let note = plain_note(id);
        let bounds = window.find(note.clone()).bounds();
        reveal_in_inspector(window, cx, bounds);
        window.right_click(note, cx);
    });
    settle(cx);
    choose("Copy", cx);
    cx.read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap()
}

#[gpui_kit::test]
fn a_changed_roster_tail_marks_its_cut_until_full_text_returns(cx: &mut TestAppContext) {
    use gpui_kit::base::test_support::texts;
    use serde_json::json;

    let (board, hub, cx) = ship(cx);
    let tail = "🌊".repeat(392) + "Complete";
    let old = "The original full progress";
    let updated = format!("New full text: {tail}");
    let reply = |at: i64, progress: &str| {
        json!({"ship":"aurora-lyra", "taskText":"full", "member": {
            "id":"wren", "name":"Wren", "contexts":[{"project":"nav", "current":{
                "id":"task-1", "summary":"Chart a coast", "progress":progress, "updatedAt":at,
            }}],
        }})
    };
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(ROW, cx);
        let member = |at, text| serde_json::from_value(reply(at, text)["member"].clone()).unwrap();
        board.update(cx, |board, cx| {
            view::inspector::crew::seed_roster_for_test(board, member(1, old), cx);
            let fetched = serde_json::from_value(reply(1, old)).unwrap();
            view::inspector::crew::finish_crew_read_for_test(board, Ok(fetched), cx);
        });
        window.render_frame(cx);
        assert!(texts(window).iter().any(|text| text.text() == old));

        board.update(cx, |board, cx| {
            view::inspector::crew::update_roster_for_test(board, member(2, &tail), cx);
        });
        window.render_frame(cx);
        assert_tail_preview(window, &tail);
        assert!(!texts(window).iter().any(|text| text.text() == old));

        board.update(cx, |board, cx| {
            let fetched = serde_json::from_value(reply(2, &updated)).unwrap();
            view::inspector::crew::finish_crew_read_for_test(board, Ok(fetched), cx);
        });
        window.render_frame(cx);
        let drawn = texts(window);
        assert!(drawn.iter().any(|text| text.text() == updated));
        assert!(
            window
                .try_find(plain_note("irc-crew-preview:named:nav:progress"))
                .is_none()
        );
    });
}

fn assert_tail_preview(window: &Window, progress: &str) {
    use gpui_kit::base::test_support::texts;

    let drawn = texts(window);
    let index = drawn
        .iter()
        .position(|text| text.text() == progress)
        .unwrap();
    let predecessor = index.checked_sub(1).map(|index| drawn[index].text());
    assert_eq!(predecessor, Some("…"), "the tail is cut at the start");
    assert!(
        window
            .try_find(plain_note("irc-crew-preview:named:nav:progress"))
            .is_some()
    );
}
