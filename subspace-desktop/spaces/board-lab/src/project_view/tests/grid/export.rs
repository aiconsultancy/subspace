use super::{fixture, mount, patch};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

#[gpui_kit::test]
fn export_uses_the_action_snapshot_even_if_worker_updates_during_the_picker(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("view.json");
    let (harness, events, cx) = mount(cx);
    cx.update(|window, cx| window.click("project-grid-example-result-export", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.within("popup-menu").click(3usize, cx));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    assert!(!destination.exists());
    cx.update(|window, cx| {
        let mut state = fixture();
        let mut update = patch(2);
        let crate::project::ResultValue::Table { table } =
            &mut update.results.get_mut("result").unwrap().value
        else {
            panic!("table fixture");
        };
        table.rows[0][0] = crate::project::InputValue::String("New evaluation".into());
        state.apply("session-one", 0, update).unwrap();
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        });
    });
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    let exported: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination).unwrap()).unwrap();
    assert_eq!(exported["rows"][0][0], "Pear");
    assert_eq!(exported["rows"][0][1], 10.);
    assert!(events.borrow().is_empty());
}
