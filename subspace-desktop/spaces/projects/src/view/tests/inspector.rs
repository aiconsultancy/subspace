use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext, WindowOptions, px};
use subspace_framework::dashboard::icon_button::IconButtonSize;
use subspace_framework::dashboard::{Dashboard as _, prefs, workspace::SpaceDock};

use super::{Page, narrow, settle, writable_record};
use crate::{INSPECTOR_KEY, Projects, Record};

#[gpui_kit::test]
fn the_toolbar_lines_up_with_both_dock_headers(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, _board) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Projects::new(window, cx))
        })
        .unwrap()
    });
    let view = VisualTestContext::from_window(window, cx).into_mut();
    view.update(|window, cx| {
        window.render_frame(cx);
        let header = window.find("workspace-navigator-header").bounds();
        let toolbar = window.find("projects-toolbar").bounds();
        let inspector = window.find("workspace-inspector-header").bounds();
        assert!((toolbar.bottom() - inspector.bottom()).abs() < px(0.5));
        assert!(
            (toolbar.bottom() - header.bottom()).abs() < px(0.5),
            "the hairlines under {toolbar:?} and {header:?} are one line"
        );
    });
}

#[gpui_kit::test]
fn the_inspector_follows_the_open_milestone_and_back(cx: &mut TestAppContext) {
    let (board, view) = narrow(IconButtonSize::Small, Page::Milestones, cx);
    let milestone = serde_json::from_value(serde_json::json!({
        "id": "milestone-first", "name": "Prepare the fixture", "status": "next",
        "targetDate": "2026-12-04", "issueProgress": {"done": 2, "total": 4},
        "version": 1
    }))
    .unwrap();
    board.update(view, |board, cx| {
        board.record = Some(Ok(Record {
            milestones: Ok(vec![milestone]),
            ..writable_record()
        }));
        cx.notify();
    });
    settle(view);
    view.update(|window, cx| {
        assert!(window.try_find("inspector-fact:Health").is_some());
        window.click("milestone-open:milestone-first", cx);
    });
    settle(view);
    view.update(|window, cx| {
        assert!(window.try_find("inspector-fact:Project").is_some());
        assert!(window.try_find("inspector-fact:Health").is_none());
        window.click("milestone-back", cx);
    });
    settle(view);
    view.update(|window, _| assert!(window.try_find("inspector-fact:Health").is_some()));
}

#[gpui_kit::test]
fn closing_and_reopening_the_inspector_saves_its_visibility(cx: &mut TestAppContext) {
    let (board, view) = narrow(IconButtonSize::Small, Page::Overview, cx);
    view.update(|window, cx| {
        prefs::init_file(None, cx);
        window.click("workspace-inspector-close", cx);
    });
    settle(view);
    view.update(|window, cx| {
        assert!(window.try_find("projects-inspector").is_none());
        assert_eq!(prefs::get(cx, INSPECTOR_KEY).as_deref(), Some("hidden"));
        board.update(cx, |board, cx| {
            assert_eq!(board.docks(cx).get(SpaceDock::Inspector), Some(false));
            board.toggle_dock(SpaceDock::Inspector, window, cx);
        });
    });
    settle(view);
    view.update(|window, cx| {
        assert!(window.try_find("inspector-fact:Health").is_some());
        assert_eq!(prefs::get(cx, INSPECTOR_KEY).as_deref(), Some("shown"));
    });
}
