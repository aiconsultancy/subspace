use crate::project::{
    Component, ComponentKind, InputValue, LivePatch, LiveState, ProjectDefinition, ResultUpdate,
    ResultValue, TableData,
};
use crate::project_view::{ProjectView, ProjectViewEvent};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, TestAppContext, Window, div,
};
use std::{cell::RefCell, rc::Rc};

mod export;
mod viewport;

struct Harness {
    view: Entity<ProjectView>,
    _subscription: Subscription,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

fn fixture() -> LiveState {
    let mut definition = ProjectDefinition::new("grid-example", "python", "main.py");
    definition.components = vec![Component::new("result", "Results", ComponentKind::Table)];
    let mut state = LiveState::new(definition, "session-one").unwrap();
    state.apply("session-one", 0, patch(1)).unwrap();
    state
}

fn patch(time: u64) -> LivePatch {
    let mut patch = LivePatch::default();
    patch.results.insert(
        "result".into(),
        ResultUpdate::new(
            "test",
            time,
            ResultValue::Table {
                table: TableData::new(
                    vec!["Name".into(), "Count".into()],
                    vec![
                        vec![InputValue::String("Pear".into()), InputValue::Number(10.)],
                        vec![InputValue::String("Apple".into()), InputValue::Number(2.)],
                        vec![InputValue::String("Plum".into()), InputValue::Number(8.)],
                    ],
                ),
            },
        ),
    );
    patch
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
    let (harness, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| ProjectView::new(&initial, window, cx));
        let subscription = cx.subscribe(&view, move |_, _, event, _| {
            received.borrow_mut().push(event.clone())
        });
        Harness {
            view,
            _subscription: subscription,
        }
    });
    cx.update(|window, cx| window.render_frame(cx));
    (harness, events, cx)
}

fn clipboard(cx: &mut gpui_kit::VisualTestContext) -> String {
    cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap())
}

#[gpui_kit::test]
fn native_pointer_range_keyboard_copy_and_sort_do_not_call_the_worker(cx: &mut TestAppContext) {
    let (_, events, cx) = mount(cx);
    cx.update(|window, cx| {
        window.click("project-grid-example-result-row-0-cell-0", cx);
        window.click_with_modifiers(
            "project-grid-example-result-row-1-cell-1",
            gpui_kit::Modifiers::shift(),
            cx,
        );
        assert_eq!(
            window
                .find("project-grid-example-result-row-0-cell-1")
                .selected(),
            Some(true)
        );
        window.press("ctrl-c", cx);
    });
    assert_eq!(clipboard(cx), "Pear\t10\nApple\t2");
    cx.update(|window, cx| {
        window.press("shift-down", cx);
        window.press("secondary-c", cx);
    });
    assert_eq!(clipboard(cx), "Pear\t10\nApple\t2\nPlum\t8");
    cx.update(|window, cx| {
        window.click("project-grid-example-result-sort-1", cx);
        window.click("project-grid-example-result-copy-view", cx);
    });
    assert_eq!(clipboard(cx), "Name\tCount\nApple\t2\nPlum\t8\nPear\t10");
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn filters_preserve_entity_and_selection_but_new_evaluation_clears_range(cx: &mut TestAppContext) {
    let (harness, events, cx) = mount(cx);
    let before = harness.read_with(cx, |harness, cx| {
        harness.view.read(cx).grids["result"].view.entity_id()
    });
    cx.update(|window, cx| {
        window.click("project-grid-example-result-row-0-cell-0", cx);
        window.click("project-grid-example-result-filter-0", cx);
        window.input("APP", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.click("project-grid-example-result-copy-view", cx));
    assert_eq!(clipboard(cx), "Name\tCount\nApple\t2");
    cx.update(|window, cx| {
        window.click("project-grid-example-result-copy-selected", cx);
        window.click("project-grid-example-result-clear-filters", cx);
        let mut state = fixture();
        state.apply("session-one", 0, patch(2)).unwrap();
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        });
        window.render_frame(cx);
        assert_eq!(
            window
                .find("project-grid-example-result-row-0-cell-0")
                .selected(),
            Some(false)
        );
    });
    assert_eq!(clipboard(cx), "Pear");
    let after = harness.read_with(cx, |harness, cx| {
        harness.view.read(cx).grids["result"].view.entity_id()
    });
    assert_eq!(before, after);
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn grid_options_disable_sort_filter_selection_and_copy_individually(cx: &mut TestAppContext) {
    let (_, events, cx) = mount(cx);
    cx.update(|window, cx| window.click("project-grid-example-result-options", cx));
    for option in 0..8 {
        cx.update(|window, cx| {
            window.click(format!("project-grid-example-result-option-{option}"), cx);
            assert_eq!(
                window
                    .find(format!("project-grid-example-result-option-{option}"))
                    .checked(),
                Some(false),
                "option {option}"
            );
        });
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        window.click("project-grid-example-result-options", cx);
        assert!(
            window
                .try_find("project-grid-example-result-filter-0")
                .is_none()
        );
        window.click("project-grid-example-result-sort-0", cx);
        assert!(
            window
                .find("project-grid-example-result-row-0")
                .bounds()
                .top()
                < window
                    .find("project-grid-example-result-row-1")
                    .bounds()
                    .top()
        );
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("unchanged".into()));
        window.click("project-grid-example-result-copy-view", cx);
        window.click("project-grid-example-result-row-0-cell-0", cx);
        assert_eq!(
            window
                .find("project-grid-example-result-row-0-cell-0")
                .selected(),
            Some(false)
        );
    });
    assert_eq!(clipboard(cx), "unchanged");
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
}
