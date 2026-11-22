use super::tests::fixture;
use super::{
    Component, ComponentKind, InputDeclaration, InputValue, LivePatch, LiveState, ResultUpdate,
    ResultValue, TableData,
};
use subspace_framework::charts::{ChartModel, Point, Series};

fn result_state() -> LiveState {
    let mut definition = fixture();
    definition
        .components
        .push(Component::new("plot", "Plot", ComponentKind::Chart));
    definition
        .components
        .push(Component::new("table", "Table", ComponentKind::Table));
    LiveState::new(definition, "session-one").unwrap()
}

#[test]
fn stale_session_or_input_and_mixed_invalid_patch_leave_all_state_unchanged() {
    let mut state = result_state();
    let mut patch = LivePatch::default();
    patch
        .values
        .insert("amount".into(), InputValue::Number(7.0));
    patch.results.insert(
        "heading".into(),
        ResultUpdate::new(
            "local-fixture",
            1000,
            ResultValue::Text {
                text: "Successful".into(),
            },
        ),
    );
    assert!(state.apply("old-session", 0, patch.clone()).is_err());
    state
        .set_value("amount", InputValue::Number(6.0), 0)
        .unwrap();
    assert!(state.apply("session-one", 0, patch.clone()).is_err());
    patch
        .values
        .insert("undeclared".into(), InputValue::Number(1.0));
    assert!(state.apply("session-one", 1, patch.clone()).is_err());
    assert_eq!(state.values()["amount"], InputValue::Number(6.0));
    assert!(state.results().is_empty());
    patch.values.remove("undeclared");
    state.apply("session-one", 1, patch).unwrap();
    assert_eq!(state.values()["amount"], InputValue::Number(7.0));
    assert_eq!(state.input_revision(), 1);
    assert_eq!(state.results()["heading"].evaluated_at_ms, 1000);
    state.bind_session("session-two").unwrap();
    assert!(state.apply("session-one", 1, LivePatch::default()).is_err());
    assert_eq!(state.results()["heading"].scope, "local-fixture");
}

#[test]
fn options_values_and_availability_apply_atomically_without_an_input_echo() {
    let mut definition = fixture();
    definition.inputs.insert(
        "choice".into(),
        InputDeclaration::String {
            default: "old".into(),
            options: vec!["old".into()],
        },
    );
    definition.components.push(Component::new(
        "choice",
        "Choice",
        ComponentKind::Select {
            input: "choice".into(),
        },
    ));
    let mut state = LiveState::new(definition, "session-one").unwrap();
    let mut patch = LivePatch::default();
    patch.options.insert("choice".into(), vec!["new".into()]);
    assert!(state.apply("session-one", 0, patch.clone()).is_err());
    assert_eq!(state.options("choice").unwrap(), &["old"]);
    patch
        .values
        .insert("choice".into(), InputValue::String("new".into()));
    patch.enabled.insert("choice".into(), false);
    state.apply("session-one", 0, patch).unwrap();
    assert_eq!(state.values()["choice"], InputValue::String("new".into()));
    assert_eq!(state.options("choice").unwrap(), &["new"]);
    assert!(!state.is_enabled("choice"));
    assert_eq!(state.input_revision(), 0);
    assert!(
        state
            .set_value("amount", InputValue::Number(8.0), 1)
            .is_err()
    );
    assert_eq!(
        state
            .set_value("amount", InputValue::Number(5.0), 0)
            .unwrap(),
        0
    );
}

fn table_patch(rows: usize, columns: usize) -> LivePatch {
    let table = TableData::new(
        (0..columns).map(|ix| format!("Column {ix}")).collect(),
        vec![vec![InputValue::Number(1.0); columns]; rows],
    );
    let mut patch = LivePatch::default();
    patch.results.insert(
        "table".into(),
        ResultUpdate::new("fixture", 1234, ResultValue::Table { table }),
    );
    patch
}

#[test]
fn table_budget_and_shape_failures_retain_the_last_successful_result_and_age() {
    let mut state = result_state();
    state.apply("session-one", 0, table_patch(500, 16)).unwrap();
    let accepted = state.results().clone();
    assert!(state.apply("session-one", 0, table_patch(501, 1)).is_err());
    assert!(state.apply("session-one", 0, table_patch(500, 17)).is_err());
    let mut malformed = table_patch(1, 2);
    if let ResultValue::Table { table } = &mut malformed.results.get_mut("table").unwrap().value {
        table.rows[0].pop();
    }
    assert!(state.apply("session-one", 0, malformed).is_err());
    assert_eq!(state.results(), &accepted);
    assert_eq!(state.results()["table"].evaluated_at_ms, 1234);
}

fn chart_patch(chart: ChartModel) -> LivePatch {
    let mut patch = LivePatch::default();
    patch.results.insert(
        "plot".into(),
        ResultUpdate::new("fixture", 1000, ResultValue::Chart { chart }),
    );
    patch
}

#[test]
fn native_chart_budgets_and_finite_shapes_are_checked_before_apply() {
    let mut state = result_state();
    let mut chart = ChartModel {
        labels: vec!["Label".into(); 500],
        series: vec![Series::new("Series", vec![Some(1.0); 500]); 32],
        ..ChartModel::default()
    };
    assert!(
        state
            .apply("session-one", 0, chart_patch(chart.clone()))
            .is_ok()
    );
    chart
        .series
        .push(Series::new("Extra", vec![Some(1.0); 500]));
    assert!(
        state
            .apply("session-one", 0, chart_patch(chart.clone()))
            .is_err()
    );
    chart.series.truncate(1);
    chart.series[0].values.push(Some(1.0));
    assert!(
        state
            .apply("session-one", 0, chart_patch(chart.clone()))
            .is_err()
    );
    chart.series[0].values = vec![Some(f64::NAN); 500];
    assert!(
        state
            .apply("session-one", 0, chart_patch(chart.clone()))
            .is_err()
    );
    chart.series[0].values = vec![Some(1.0)];
    assert!(
        state
            .apply("session-one", 0, chart_patch(chart.clone()))
            .is_err()
    );
    chart.labels.clear();
    chart.series[0].values.clear();
    chart.series[0].points = vec![Point {
        x: 1.0,
        y: f64::INFINITY,
    }];
    assert!(state.apply("session-one", 0, chart_patch(chart)).is_err());
}

#[test]
fn wrong_result_type_and_undeclared_options_or_targets_are_rejected() {
    let mut state = result_state();
    let mut patch = LivePatch::default();
    patch.results.insert(
        "plot".into(),
        ResultUpdate::new(
            "fixture",
            1000,
            ResultValue::Text {
                text: "wrong type".into(),
            },
        ),
    );
    assert!(state.apply("session-one", 0, patch).is_err());
    let mut patch = LivePatch::default();
    patch
        .options
        .insert("amount".into(), vec!["wrong type".into()]);
    assert!(state.apply("session-one", 0, patch).is_err());
    let mut patch = LivePatch::default();
    patch.enabled.insert("heading".into(), false);
    assert!(state.apply("session-one", 0, patch).is_err());
    assert!(state.results().is_empty());
}
