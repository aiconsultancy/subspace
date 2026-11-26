//! Task paging in the narrowest Ship inspector.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, TestAppContext, Window, px, size};

use super::{BACK, ROW, install_fleet_api, live, set_fleet, settle, ship, view};
use crate::tests::remove::choose;

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
        assert_eq!(
            drawn[index(&progress) - 1].text(),
            "…",
            "progress marker precedes the tail"
        );
        assert!(
            drawn
                .iter()
                .any(|text| text.text().contains("the first 180 characters"))
        );
        assert!(
            drawn
                .iter()
                .any(|text| text.text().contains("the latest 400 characters"))
        );
    });
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

fn exercise_progress_page_steps(window: &mut Window, cx: &mut App, first: &str, second: &str) {
    use gpui_kit::base::test_support::texts;

    assert_pager_fits(window, &["next"]);
    window.within(PROGRESS_PAGER).click("next", cx);
    window.render_frame(cx);
    assert!(texts(window).iter().any(|text| text.text() == second));
    assert_pager_fits(window, &["prev", "next", "jump"]);
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
    window.within(PROGRESS_PAGER).click("next", cx);
    window.render_frame(cx);
    assert!(texts(window).iter().any(|text| text.text() == second));
}

fn assert_pager_fits(window: &mut Window, controls: &[&'static str]) {
    let body = window.find("workspace-inspector-body").bounds();
    assert_eq!(
        window.find("workspace-inspector-slot").bounds().size.width,
        px(260.)
    );
    let pager = window.within(PROGRESS_PAGER);
    let progress = window.find("inspector-fact:Progress").bounds();
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
