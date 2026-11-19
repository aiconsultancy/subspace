use super::{GridCell, GridFormat, GridScope, GridSnapshot, model::Model, selection::Cell};

fn fixture() -> Model {
    Model::new(
        GridSnapshot::new(
            vec!["Name".into(), "Value".into()],
            vec![
                vec![GridCell::text("pear"), GridCell::number(10.)],
                vec![GridCell::text("Apple"), GridCell::number(2.)],
                vec![GridCell::text("apple"), GridCell::number(2.)],
                vec![GridCell::text("plum"), GridCell::text("1")],
            ],
        )
        .unwrap(),
    )
}

#[test]
fn numeric_then_unicode_sort_keeps_ties_in_source_order() {
    let mut model = fixture();
    model.cycle_sort(1);
    assert_eq!(model.shown, [1, 2, 0, 3]);
    model.cycle_sort(1);
    assert_eq!(model.shown, [3, 0, 1, 2]);
    model.cycle_sort(1);
    assert_eq!(model.shown, [0, 1, 2, 3]);
}

#[test]
fn filters_are_case_insensitive_contains_and_combined_with_and() {
    let mut model = fixture();
    model.filters[0] = "APP".into();
    model.filters[1] = "2".into();
    model.rebuild();
    assert_eq!(model.shown, [1, 2]);
    model.filters[1] = "absent".into();
    model.rebuild();
    assert!(model.shown.is_empty());
    model.filters.fill(String::new());
    model.rebuild();
    assert_eq!(model.shown, [0, 1, 2, 3]);
}

#[test]
fn view_and_all_scopes_are_distinct_and_do_not_modify_the_source() {
    let mut model = fixture();
    let original = model.source.clone();
    model.cycle_sort(1);
    model.filters[0] = "apple".into();
    model.rebuild();
    assert_eq!(
        model.capture(GridScope::View).tsv(true),
        "Name\tValue\nApple\t2\napple\t2"
    );
    assert_eq!(
        model.capture(GridScope::All).tsv(true),
        "Name\tValue\npear\t10\nApple\t2\napple\t2\nplum\t1"
    );
    assert_eq!(model.source, original);
}

#[test]
fn unicode_strings_sort_by_scalar_order_and_snapshot_rejects_invalid_shapes() {
    let source = GridSnapshot::new(
        vec!["Name".into()],
        ["é", "Ω", "e", "é"]
            .into_iter()
            .map(|text| vec![GridCell::text(text)])
            .collect(),
    )
    .unwrap();
    let mut model = Model::new(source);
    model.cycle_sort(0);
    assert_eq!(model.shown, [2, 0, 3, 1]);
    assert!(GridSnapshot::new(vec![], vec![]).is_err());
    assert!(GridSnapshot::new(vec!["One".into()], vec![vec![]]).is_err());
    assert!(GridSnapshot::new(vec!["One".into()], vec![vec![GridCell::number(f64::NAN)]]).is_err());
    assert!(GridSnapshot::new(vec!["One".into()], vec![vec![GridCell::number(0.)]; 501]).is_err());
    assert!(
        GridSnapshot::new(
            vec!["One".into(); 17],
            vec![vec![GridCell::number(0.); 17]; 500]
        )
        .is_err()
    );
    assert!(GridSnapshot::new(vec!["One".into()], vec![]).is_ok());
}

#[test]
fn schema_change_clears_positional_filters_and_sort() {
    let mut model = fixture();
    model.filters[1] = "pear".into();
    model.cycle_sort(1);
    model.replace(
        GridSnapshot::new(vec!["Other".into()], vec![vec![GridCell::text("fresh")]]).unwrap(),
    );
    assert_eq!(model.shown, [0]);
    assert!(model.sort.is_none());
    assert_eq!(model.filters, [""]);
}

#[test]
fn selected_records_survive_sort_filter_but_not_replacement() {
    let mut model = fixture();
    model
        .selection
        .select(Cell { row: 0, column: 0 }, false, &model.shown);
    model
        .selection
        .select(Cell { row: 1, column: 1 }, true, &model.shown);
    model.cycle_sort(1);
    model.filters[0] = "Apple".into();
    model.rebuild();
    let captured = model.capture(GridScope::Selected);
    assert_eq!(captured.tsv(false), "Apple\t2\npear\t10");
    model.replace(fixture().source);
    assert!(model.selection.cells.is_empty());
    assert!(model.selection.anchor.is_none());
    assert_eq!(captured.tsv(false), "Apple\t2\npear\t10");
}

#[test]
fn spreadsheet_safety_preserves_number_types_and_quotes_delimiters() {
    let source = GridSnapshot::new(
        vec![" \u{1}=header".into(), "Value".into()],
        vec![vec![
            GridCell::text("\t-evil,\"x\"\n"),
            GridCell::number(-2.),
        ]],
    )
    .unwrap();
    assert_eq!(
        source.encode(GridFormat::Csv).unwrap(),
        "' \u{1}=header,Value\n\"'\t-evil,\"\"x\"\"\n\",-2"
    );
    assert_eq!(
        source.tsv(true),
        "' \u{1}=header\tValue\n\"'\t-evil,\"\"x\"\"\n\"\t-2"
    );
    let json: serde_json::Value =
        serde_json::from_str(&source.encode(GridFormat::Json).unwrap()).unwrap();
    assert_eq!(json["rows"][0][1], -2.);
    assert_eq!(json["rows"][0][0], "\t-evil,\"x\"\n");
    for prefix in ["=", "+", "-", "@", " \t\u{1}=", "\r\n+"] {
        let text = format!("{prefix}not-a-formula");
        assert_eq!(
            GridCell::text(text.clone()).spreadsheet(),
            format!("'{text}")
        );
    }
    let bom_source = GridSnapshot::new(
        vec!["\u{feff}=1+1".into(), "Number".into()],
        vec![vec![
            GridCell::text(" \u{feff}@cell"),
            GridCell::number(-2.),
        ]],
    )
    .unwrap();
    assert_eq!(
        bom_source.encode(GridFormat::Csv).unwrap(),
        "'\u{feff}=1+1,Number\n' \u{feff}@cell,-2"
    );
    assert_eq!(
        bom_source.tsv(true),
        "'\u{feff}=1+1\tNumber\n' \u{feff}@cell\t-2"
    );
    let json: serde_json::Value =
        serde_json::from_str(&bom_source.encode(GridFormat::Json).unwrap()).unwrap();
    assert_eq!(json["columns"][0], "\u{feff}=1+1");
    assert_eq!(json["rows"][0][0], " \u{feff}@cell");
    assert_eq!(json["rows"][0][1], -2.);
}
