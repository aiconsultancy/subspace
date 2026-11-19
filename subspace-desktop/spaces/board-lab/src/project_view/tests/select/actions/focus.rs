use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::super::updated_options;
use super::{choose_next, fixture, mount_state, open, sync};
use crate::project::{InputValue, LivePatch};

#[gpui_kit::test]
fn retired_sdk_confirmation_does_not_steal_unrelated_focus(cx: &mut TestAppContext) {
    let mut next = fixture();
    let (harness, events, cx) = mount_state(cx, next.clone());
    updated_options(&mut next);
    cx.update(|window, cx| {
        window.click("project-example-amount", cx);
        assert_eq!(window.find("project-example-amount").focused(), Some(true));
        let slider_focus = window.focused(cx).unwrap();
        open(&harness, window, cx);
        choose_next(window, cx);
        slider_focus.focus(window, cx);
        window.render_frame(cx);
        assert_eq!(window.find("project-example-amount").focused(), Some(true));
        sync(&harness, &next, window, cx);
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("project-example-amount").focused(), Some(true));
        assert_eq!(
            window.find("project-example-choice").value(),
            Some("Second")
        );
    });
}

#[gpui_kit::test]
fn changed_select_restores_focus_after_old_sdk_confirmation(cx: &mut TestAppContext) {
    let mut next = fixture();
    let (harness, events, cx) = mount_state(cx, next.clone());
    updated_options(&mut next);
    cx.update(|window, cx| {
        open(&harness, window, cx);
        choose_next(window, cx);
        sync(&harness, &next, window, cx);
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("project-example-choice").focused(), Some(true));
        assert_eq!(
            window.find("project-example-choice").expanded(),
            Some(false)
        );
        assert_eq!(
            window.find("project-example-choice").value(),
            Some("Second")
        );
    });
}

#[gpui_kit::test]
fn unrelated_worker_value_patch_preserves_open_select_and_focus(cx: &mut TestAppContext) {
    let mut next = fixture();
    let (harness, events, cx) = mount_state(cx, next.clone());
    let mut patch = LivePatch::default();
    patch.values.insert("amount".into(), InputValue::Number(6.));
    next.apply("session-one", 0, patch).unwrap();
    let focus = cx.update(|window, cx| {
        open(&harness, window, cx);
        let old = harness.read(cx).view.read(cx).selects["choice"]
            .state
            .entity_id();
        let focus = window.focused(cx).unwrap();
        sync(&harness, &next, window, cx);
        assert_eq!(
            harness.read(cx).view.read(cx).selects["choice"]
                .state
                .entity_id(),
            old
        );
        assert!(focus.is_focused(window));
        focus
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(focus.is_focused(window));
        assert_eq!(window.find("project-example-choice").expanded(), Some(true));
        choose_next(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[super::second_event()]);
}
