use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::mount_state;
use crate::project::{
    Component, ComponentKind, InputValue, LivePatch, LiveState, ProjectDefinition, ResultUpdate,
    ResultValue, TableData,
};

fn result_fixture() -> (LiveState, LivePatch) {
    let mut definition = ProjectDefinition::new("example", "python", "main.py");
    definition.components = vec![
        Component::new(
            "heading",
            "Heading",
            ComponentKind::Text {
                text: "Waiting".into(),
            },
        ),
        Component::new("plot", "Revenue chart", ComponentKind::Chart),
        Component::new("table", "Revenue table", ComponentKind::Table),
    ];
    let mut patch = LivePatch::default();
    patch.results.insert(
        "heading".into(),
        ResultUpdate::new(
            "test",
            1,
            ResultValue::Text {
                text: "Ready".into(),
            },
        ),
    );
    patch.results.insert(
        "plot".into(),
        ResultUpdate::new(
            "test",
            1,
            ResultValue::Chart {
                chart: subspace_framework::charts::ChartModel::default(),
            },
        ),
    );
    patch.results.insert(
        "table".into(),
        ResultUpdate::new(
            "test",
            1,
            ResultValue::Table {
                table: TableData::new(
                    vec!["Name".into(), "Count".into()],
                    vec![vec![
                        InputValue::String("Apples".into()),
                        InputValue::Number(7.),
                    ]],
                ),
            },
        ),
    );
    (LiveState::new(definition, "session-one").unwrap(), patch)
}

#[gpui_kit::test]
fn result_sync_updates_rendered_text_chart_and_table_without_input(cx: &mut TestAppContext) {
    let (mut state, patch) = result_fixture();
    let (harness, events, cx) = mount_state(cx, state.clone());
    state.apply("session-one", 0, patch).unwrap();
    cx.update(|window, cx| {
        harness.update(cx, |harness, cx| {
            harness
                .view
                .update(cx, |view, cx| view.sync(&state, window, cx))
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("project-example-heading").label(),
            Some("Ready")
        );
        assert_eq!(
            window.find("project-example-plot").label(),
            Some("Revenue chart")
        );
        assert_eq!(
            window.find("project-example-table-row-0").label(),
            Some("Apples, 7")
        );
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn table_rows_fill_the_same_width_as_the_result_content(cx: &mut TestAppContext) {
    let (mut state, patch) = result_fixture();
    state.apply("session-one", 0, patch).unwrap();
    let (_, _, cx) = mount_state(cx, state);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let content = window.find("project-example-heading").bounds();
        let row = window.find("project-example-table-row-0").bounds();
        assert_eq!(row.size.width, content.size.width);
        for column in 0..2 {
            let header = window
                .find(format!("project-example-table-header-{column}"))
                .bounds();
            let cell = window
                .find(format!("project-example-table-row-0-cell-{column}"))
                .bounds();
            assert_eq!(header.origin.x, cell.origin.x);
            assert_eq!(header.size.width, cell.size.width);
        }
    });
}

#[gpui_kit::test]
fn rejected_result_keeps_the_reported_metadata(cx: &mut TestAppContext) {
    let (mut state, patch) = result_fixture();
    state.apply("session-one", 0, patch).unwrap();
    let mut invalid = LivePatch::default();
    invalid.results.insert(
        "table".into(),
        ResultUpdate::new(
            "wrong",
            55,
            ResultValue::Table {
                table: TableData::new(vec!["Name".into()], vec![vec![]]),
            },
        ),
    );
    assert!(state.apply("session-one", 0, invalid).is_err());
    let (_, _, cx) = mount_state(cx, state);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("project-example-table-metadata").label(),
            Some("Scope: test · Reported evaluation: 1970-01-01 00:00:00.001 UTC")
        );
    });
}

#[gpui_kit::test]
fn result_metadata_shows_the_reported_scope_and_time(cx: &mut TestAppContext) {
    let (mut state, patch) = result_fixture();
    state.apply("session-one", 0, patch).unwrap();
    let (_, _, cx) = mount_state(cx, state);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let metadata = window.find("project-example-table-metadata");
        assert_eq!(
            metadata.label(),
            Some("Scope: test · Reported evaluation: 1970-01-01 00:00:00.001 UTC")
        );
    });
}
