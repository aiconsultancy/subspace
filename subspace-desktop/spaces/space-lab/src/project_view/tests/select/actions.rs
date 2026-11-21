use gpui_kit::base::actions::{Confirm, SelectDown};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Action, App, Entity, Focusable as _, TestAppContext, Window};

use super::super::{Harness, ProjectViewEvent, mount_state};
use super::select_fixture;
use crate::project::{Component, ComponentKind, InputDeclaration, InputValue, LiveState};

mod focus;
mod stale;

fn fixture() -> LiveState {
    let mut definition = select_fixture().definition().clone();
    if let Some(InputDeclaration::String { options, .. }) = definition.inputs.get_mut("choice") {
        options.push("Third".into());
    }
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::Number {
            default: 5.,
            min: 0.,
            max: 10.,
            step: 1.,
        },
    );
    definition.components.push(Component::new(
        "amount",
        "Amount",
        ComponentKind::Slider {
            input: "amount".into(),
        },
    ));
    definition.components.push(Component::new(
        "summary",
        "Summary",
        ComponentKind::Text {
            text: "Waiting".into(),
        },
    ));
    LiveState::new(definition, "session-one").unwrap()
}

fn sync(harness: &Entity<Harness>, next: &LiveState, window: &mut Window, cx: &mut App) {
    harness.update(cx, |harness, cx| {
        harness
            .view
            .update(cx, |view, cx| view.sync(next, window, cx));
    });
}

fn dispatch(action: &dyn Action, window: &mut Window, cx: &mut App) {
    let focus = window.focused(cx).expect("select must have focus");
    focus.dispatch_action(action, window, cx);
    window.render_frame(cx);
}

fn open(harness: &Entity<Harness>, window: &mut Window, cx: &mut App) {
    let select = harness.read(cx).view.read(cx).selects["choice"]
        .state
        .clone();
    let focus = select.focus_handle(cx);
    focus.focus(window, cx);
    window.render_frame(cx);
    dispatch(&Confirm { secondary: false }, window, cx);
    assert_eq!(window.find("project-example-choice").expanded(), Some(true));
}

fn choose_next(window: &mut Window, cx: &mut App) {
    dispatch(&SelectDown, window, cx);
    dispatch(&Confirm { secondary: false }, window, cx);
}

fn second_event() -> ProjectViewEvent {
    ProjectViewEvent::Input {
        key: "choice".into(),
        value: InputValue::String("Second".into()),
        session_id: "session-one".into(),
        expected_revision: 0,
    }
}

#[gpui_kit::test]
fn sdk_actions_without_pointer_or_key_capture_emit_selection(cx: &mut TestAppContext) {
    let (harness, events, cx) = mount_state(cx, select_fixture());
    cx.update(|window, cx| {
        open(&harness, window, cx);
        choose_next(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(events.borrow().as_slice(), &[second_event()]);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("project-example-choice").value(),
            Some("Second")
        );
        assert_eq!(window.find("project-example-choice").focused(), Some(true));
    });
}
