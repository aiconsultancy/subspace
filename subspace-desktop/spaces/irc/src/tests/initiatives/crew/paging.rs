//! Task paging in the narrowest Ship inspector.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, ScrollDelta, TestAppContext, Window, point, px, size};

use super::{BACK, ROW, install_fleet_api, live, set_fleet, settle, ship, view};
use crate::tests::remove::choose;

#[gpui_kit::test]
fn a_successful_crew_read_warns_until_stargate_confirms_full_text(cx: &mut TestAppContext) {
    use gpui_kit::base::test_support::texts;
    use serde_json::json;
    use subspace_client_fleet::CrewDetail;

    let (board, hub, cx) = ship(cx);
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    let note = plain_note("irc-crew-unconfirmed-task-text");
    let progress = "x ".repeat(256) + "The supplied ending.";
    let mut reply = json!({"ship":"aurora-lyra", "member": {
        "id":"wren", "name":"Wren", "contexts":[{"project":"nav", "current":{
            "id":"task-1", "summary":"Chart a coast", "progress":progress,
        }}],
    }});
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(ROW, cx);
        let member = serde_json::from_value(reply["member"].clone()).unwrap();
        board.update(cx, |board, cx| {
            view::inspector::crew::seed_roster_for_test(board, member, cx)
        });
        for marker in [None, Some("preview"), Some("unknown"), Some("full")] {
            if let Some(marker) = marker {
                reply["taskText"] = json!(marker);
            }
            let detail: CrewDetail = serde_json::from_value(reply.clone()).unwrap();
            board.update(cx, |board, cx| {
                view::inspector::crew::finish_crew_read_for_test(board, detail, cx)
            });
            window.render_frame(cx);
            assert_eq!(
                window.find_all(note.clone()).len(),
                usize::from(marker != Some("full")),
                "{marker:?}"
            );
            if marker.is_none() {
                window.within(PROGRESS_PAGER).click("next", cx);
                window.render_frame(cx);
            }
            assert!(
                texts(window)
                    .iter()
                    .any(|text| text.text() == "The supplied ending.")
            );
        }
    });
}

#[gpui_kit::test]
fn capped_roster_text_marks_the_cut_side_without_altering_the_value(cx: &mut TestAppContext) {
    use gpui_kit::base::test_support::texts;

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
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(ROW, cx);
        board.update(cx, |board, cx| {
            view::inspector::crew::seed_roster_for_test(board, member, cx)
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
    });
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
            view::inspector::crew::finish_crew_read_for_test(board, fetched, cx);
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
            view::inspector::crew::finish_crew_read_for_test(board, fetched, cx);
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

// The shared selectable text registers its context-menu host with this ID.
fn plain_note(id: &str) -> String {
    format!("plain-menu-Name(\"{id}\")")
}

#[gpui_kit::test]
fn crew_task_progress_pages_fit_and_work_at_the_minimum_width(cx: &mut TestAppContext) {
    use gpui_kit::base::test_support::texts;
    use subspace_framework::dashboard::density::Density;

    const TAIL: &str = "🌊\t\r\u{202e}\x1bfinish";
    const VISIBLE_TAIL: &str = "🌊⟨U+0009⟩⟨U+000D⟩⟨U+202E⟩⟨U+001B⟩finish";
    let (board, hub, cx) = ship(cx);
    let first = "λ".repeat(512);
    let second = "y".repeat(512);
    let mut snapshot = crate::live::tests::snapshot();
    let mut member = snapshot
        .ships
        .remove("aurora-lyra")
        .unwrap()
        .crew_members
        .into_iter()
        .find(|member| member.id == "wren")
        .unwrap();
    let task = member.contexts[0].current.as_mut().unwrap();
    task.progress =
        first.clone() + &second + &"x".repeat(64_000 - 1024 - TAIL.chars().count()) + TAIL;
    let summary = task.summary.clone();
    set_fleet(&hub, live(), cx);
    install_fleet_api(cx);
    cx.simulate_resize(size(px(1280.), px(960.)));
    cx.update(|window, cx| {
        let mut density = Density::default();
        density.set("--inspector-w", 260.);
        cx.set_global(density);
        window.render_frame(cx);
        window.click(ROW, cx);
        board.update(cx, |board, cx| {
            view::inspector::crew::seed_roster_for_test(board, member, cx)
        });
        window.render_frame(cx);
        assert!(window.find(BACK).visible());
        let drawn = texts(window);
        assert!(!drawn.iter().any(|text| text.text().contains("finish")));
        assert_eq!(
            drawn.iter().filter(|text| text.text() == summary).count(),
            1
        );
        assert!(drawn.iter().any(|text| text.text() == first));
        exercise_progress_page_steps(window, cx, &first, &second);
        window.within(PROGRESS_PAGER).click("jump", cx);
    });
    settle(cx);
    choose("Last page (125)", cx);
    cx.update(|window, _| {
        let drawn = texts(window);
        assert!(!drawn.iter().any(|text| text.text() == second));
        assert!(drawn.iter().any(|text| text.text().ends_with(VISIBLE_TAIL)));
    });
}

const PROGRESS_PAGER: &str = "irc-crew-task-pages:named:nav:progress";
const BODY: &str = "workspace-inspector-body";

fn exercise_progress_page_steps(window: &mut Window, cx: &mut App, first: &str, second: &str) {
    use gpui_kit::base::test_support::texts;

    reveal_progress_pager(window, cx);
    assert_pager_fits(window, first, &["next"]);
    window.within(PROGRESS_PAGER).click("next", cx);
    window.render_frame(cx);
    assert!(texts(window).iter().any(|text| text.text() == second));
    reveal_progress_pager(window, cx);
    assert_pager_fits(window, second, &["prev", "next", "jump"]);
    for (control, label) in [
        ("prev", "Previous page"),
        ("next", "Next page"),
        ("jump", "Jump to page, current page 2 of 125"),
    ] {
        assert_eq!(
            window.within(PROGRESS_PAGER).find(control).label(),
            Some(label)
        );
    }
    window.within(PROGRESS_PAGER).click("prev", cx);
    window.render_frame(cx);
    assert!(texts(window).iter().any(|text| text.text() == first));
    reveal_progress_pager(window, cx);
    window.within(PROGRESS_PAGER).click("next", cx);
    window.render_frame(cx);
    assert!(texts(window).iter().any(|text| text.text() == second));
    reveal_progress_pager(window, cx);
}

fn reveal_progress_pager(window: &mut Window, cx: &mut App) {
    // At 260 px a long page can reach below the inspector fold. Reach its
    // controls the same way a reader does, via the existing inspector scroll.
    let body = window.find(BODY).bounds();
    let next = window.within(PROGRESS_PAGER).find("next").bounds();
    let delta = if next.bottom() > body.bottom() {
        body.bottom() - next.bottom() - px(12.)
    } else if next.top() < body.top() {
        body.top() - next.top() + px(12.)
    } else {
        return;
    };
    window.scroll(BODY, ScrollDelta::Pixels(point(px(0.), delta)), cx);
}

fn assert_pager_fits(window: &mut Window, value: &str, controls: &[&'static str]) {
    use gpui_kit::base::test_support::texts;

    let body = window.find(BODY).bounds();
    assert_eq!(
        window.find("workspace-inspector-slot").bounds().size.width,
        px(260.)
    );
    let progress = texts(window)
        .into_iter()
        .find(|text| text.text() == value)
        .unwrap()
        .bounds();
    let pager = window.within(PROGRESS_PAGER);
    for control in controls {
        let node = pager.find(*control);
        let bounds = node.bounds();
        assert!(node.visible(), "{control} must be visible");
        assert!(
            bounds.top() >= progress.bottom(),
            "{control} must follow its field"
        );
        assert!(
            bounds.left() >= body.left()
                && bounds.right() <= body.right()
                && bounds.top() >= body.top()
                && bounds.bottom() <= body.bottom(),
            "{control} at {bounds:?} must fit inside {body:?}"
        );
    }
}
