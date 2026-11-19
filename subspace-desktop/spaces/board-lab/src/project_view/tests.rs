use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, TestAppContext, Window, div,
};

use super::{ProjectView, ProjectViewEvent};
use crate::project::{
    Component, ComponentKind, InputDeclaration, InputValue, LivePatch, LiveState, ProjectDefinition,
};

mod grid;
mod outputs;
mod select;

struct Harness {
    view: Entity<ProjectView>,
    _subscription: Subscription,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w_64().child(self.view.clone())
    }
}

fn fixture() -> LiveState {
    let mut definition = ProjectDefinition::new("example", "python", "main.py");
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::Number {
            default: 5.,
            min: 5.,
            max: 29.,
            step: 10.,
        },
    );
    definition.actions.push("run".into());
    definition.components = vec![
        Component::new(
            "amount",
            "Amount",
            ComponentKind::Slider {
                input: "amount".into(),
            },
        ),
        Component::new(
            "run",
            "Run",
            ComponentKind::Button {
                action: "run".into(),
            },
        ),
    ];
    LiveState::new(definition, "session-one").unwrap()
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    Entity<Harness>,
    Rc<RefCell<Vec<ProjectViewEvent>>>,
    &mut gpui_kit::VisualTestContext,
) {
    mount_state(cx, fixture())
}

fn mount_state(
    cx: &mut TestAppContext,
    initial: LiveState,
) -> (
    Entity<Harness>,
    Rc<RefCell<Vec<ProjectViewEvent>>>,
    &mut gpui_kit::VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let received = events.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| ProjectView::new(&initial, window, cx));
        let subscription = cx.subscribe(&view, move |_, _, event, _| {
            received.borrow_mut().push(event.clone())
        });
        Harness {
            view,
            _subscription: subscription,
        }
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, events, cx)
}

#[gpui_kit::test]
fn action_click_emits_intent_at_the_current_revision(cx: &mut TestAppContext) {
    let (_, events, cx) = mount(cx);
    cx.update(|window, cx| window.click("project-example-run", cx));
    cx.run_until_parked();
    assert_eq!(
        events.borrow().as_slice(),
        &[ProjectViewEvent::Action {
            action: "run".into(),
            session_id: "session-one".into(),
            expected_revision: 0
        }]
    );
}

#[gpui_kit::test]
fn content_is_inset_and_action_remains_compact(cx: &mut TestAppContext) {
    let (_, _, cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let viewport = window.find("project-content").bounds();
        let action = window.find("project-example-run").bounds();
        assert!(action.left() > viewport.left());
        assert!(action.right() < viewport.right());
    });
}

#[gpui_kit::test]
fn action_after_session_rebind_carries_the_new_session(cx: &mut TestAppContext) {
    let (harness, events, cx) = mount(cx);
    let mut state = fixture();
    state.bind_session("session-two").unwrap();
    cx.update(|window, cx| {
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx));
        });
        window.render_frame(cx);
        window.click("project-example-run", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        events.borrow().as_slice(),
        &[ProjectViewEvent::Action {
            action: "run".into(),
            session_id: "session-two".into(),
            expected_revision: 0,
        }]
    );
}

#[gpui_kit::test]
fn sync_disables_action_without_emitting_a_user_event(cx: &mut TestAppContext) {
    let (harness, events, cx) = mount(cx);
    let mut state = fixture();
    let mut patch = LivePatch::default();
    patch.enabled.insert("run".into(), false);
    state.apply("session-one", 0, patch).unwrap();
    cx.update(|window, cx| {
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        })
    });
    cx.update(|window, cx| {
        window.click("project-example-run", cx);
        window.press("enter", cx);
        let button = window.find("project-example-run");
        assert_eq!(button.role(), Some(gpui_kit::Role::Button));
        assert_eq!(button.label(), Some("Run"));
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
    let mut patch = LivePatch::default();
    patch.enabled.insert("run".into(), true);
    state.apply("session-one", 0, patch).unwrap();
    cx.update(|window, cx| {
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        });
        window.render_frame(cx);
        window.click("project-example-run", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        events.borrow().as_slice(),
        &[ProjectViewEvent::Action {
            action: "run".into(),
            session_id: "session-one".into(),
            expected_revision: 0,
        }]
    );
}

#[gpui_kit::test]
fn slider_end_emits_a_value_accepted_by_live_state(cx: &mut TestAppContext) {
    let (_, events, cx) = mount(cx);
    cx.update(|window, cx| {
        window.click("project-example-amount", cx);
        window.press("end", cx);
    });
    cx.run_until_parked();
    let events = events.borrow();
    let Some(ProjectViewEvent::Input {
        key,
        value,
        session_id,
        expected_revision,
    }) = events.last()
    else {
        panic!("slider did not emit a user input")
    };
    assert_eq!(key, "amount");
    assert_eq!(session_id, "session-one");
    assert_eq!(value, &InputValue::Number(25.));
    assert!(
        fixture()
            .set_value(key, value.clone(), *expected_revision)
            .is_ok()
    );
}

#[gpui_kit::test]
fn slider_sync_preserves_focus_without_echo(cx: &mut TestAppContext) {
    let (harness, events, cx) = mount(cx);
    cx.update(|window, cx| window.click("project-example-amount", cx));
    cx.run_until_parked();
    events.borrow_mut().clear();
    let mut state = fixture();
    let mut patch = LivePatch::default();
    patch
        .values
        .insert("amount".into(), InputValue::Number(15.));
    state.apply("session-one", 0, patch).unwrap();
    cx.update(|window, cx| {
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx));
        });
        window.render_frame(cx);
        assert_eq!(window.find("project-example-amount").focused(), Some(true));
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
}

#[test]
fn fixed_and_unrepresentable_ranges_do_not_create_a_slider() {
    use super::controls::slider_spec;
    assert!(slider_spec(5., 5., 1.).is_none());
    assert!(slider_spec(5., 6., 2.).is_none());
    assert!(slider_spec(0., 20_000_000., 1.).is_none());
    assert!(slider_spec(5., 29., 10.).is_some());
}
