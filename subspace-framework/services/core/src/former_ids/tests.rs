use serde_json::{Value, json};

use super::{Renames, declared, rename_config};
use crate::state::State;

mod files;

pub(super) fn example() -> Renames {
    Renames::from([("example-old".to_owned(), "example-new".to_owned())])
}

fn config(value: Value) -> serde_json::Map<String, Value> {
    match value {
        Value::Object(root) => root,
        _ => unreachable!("a test config is an object"),
    }
}

#[test]
fn former_ids_pair_each_with_its_space() {
    let mut descriptor = subspace_product::fixture::descriptor();
    descriptor.boards[0].former_ids = vec!["example-old".into(), "example-older".into()];
    descriptor.boards[1].aliases = vec!["example-short".into()];
    let product = subspace_product::validate_product(&descriptor).unwrap();
    let id = product.boards[0].id.as_str().to_owned();
    assert_eq!(
        declared(&product),
        Renames::from([
            ("example-old".to_owned(), id.clone()),
            ("example-older".to_owned(), id)
        ])
    );
}

#[test]
fn each_config_family_moves_to_the_space_s_id() {
    let mut root = config(json!({
        "boards": {"example-old": false, "other": true},
        "boardOrder": ["other", "example-old"],
        "features": {"board.example-old.flag": true, "app.flag": false},
        "mcp": {"groups": {"example-old": false}, "port": 7400},
        "commands": {"aliases": {"example-old": ["short"], "x": ["y"]}},
        "unknown": {"example-old": 1},
    }));
    assert!(rename_config(&mut root, &example()));
    assert_eq!(
        Value::Object(root),
        json!({
            "boards": {"example-new": false, "other": true},
            "boardOrder": ["other", "example-new"],
            "features": {"board.example-new.flag": true, "app.flag": false},
            "mcp": {"groups": {"example-new": false}, "port": 7400},
            "commands": {"aliases": {"example-new": ["short"], "x": ["y"]}},
            "unknown": {"example-old": 1},
        })
    );
}

#[test]
fn the_space_s_own_config_entry_wins_and_the_former_one_is_gone() {
    let mut root = config(json!({
        "boards": {"example-old": false, "example-new": true},
        "features": {"board.example-old.flag": true, "board.example-new.flag": false},
        "mcp": {"groups": {"example-old": false, "example-new": true}},
        "commands": {"aliases": {"example-old": ["old"], "example-new": ["new"]}},
    }));
    assert!(rename_config(&mut root, &example()));
    assert_eq!(
        Value::Object(root),
        json!({
            "boards": {"example-new": true},
            "features": {"board.example-new.flag": false},
            "mcp": {"groups": {"example-new": true}},
            "commands": {"aliases": {"example-new": ["new"]}},
        })
    );
}

#[test]
fn the_application_mcp_group_s_choice_never_moves() {
    let group = subspace_product::MCP_APPLICATION_GROUP;
    let mut from = config(json!({"mcp": {"groups": {group: false}}}));
    let renames = Renames::from([(group.to_owned(), "example-new".to_owned())]);
    assert!(!rename_config(&mut from, &renames));
    assert_eq!(from["mcp"]["groups"], json!({group: false}));

    let mut onto = config(json!({"mcp": {"groups": {"example-old": false}}}));
    let renames = Renames::from([("example-old".to_owned(), group.to_owned())]);
    assert!(!rename_config(&mut onto, &renames));
    assert_eq!(onto["mcp"]["groups"], json!({"example-old": false}));
}

#[test]
fn board_keys_match_as_the_parser_trims_them() {
    let mut spaced = config(json!({"boards": {" example-old ": false}}));
    assert!(rename_config(&mut spaced, &example()));
    assert_eq!(spaced["boards"], json!({"example-new": false}));

    let mut own = config(json!({"boards": {"example-old": false, "example-new ": true}}));
    assert!(rename_config(&mut own, &example()));
    assert_eq!(own["boards"], json!({"example-new ": true}));
}

#[test]
fn board_order_never_lists_a_space_twice() {
    let mut renames = example();
    renames.insert("example-older".into(), "example-new".into());
    let mut listed = config(json!({"boardOrder": ["example-old", "x", "example-new"]}));
    assert!(rename_config(&mut listed, &renames));
    assert_eq!(listed["boardOrder"], json!(["x", "example-new"]));

    let mut twice = config(json!({"boardOrder": ["example-older", "x", "example-old", 7]}));
    assert!(rename_config(&mut twice, &renames));
    assert_eq!(twice["boardOrder"], json!(["example-new", "x", 7]));
}

#[test]
fn only_a_whole_former_id_matches() {
    let mut root = config(json!({
        "boards": {"example-old-two": false, "example-older": false},
        "boardOrder": ["example-old-two"],
        "features": {"board.example-old-two.flag": true, "board.example-old": true},
    }));
    let before = root.clone();
    assert!(!rename_config(&mut root, &example()));
    assert_eq!(root, before);
}

#[test]
fn each_state_family_moves_and_the_space_s_own_key_wins() {
    let mut state = State::default();
    state.set("app.dashboard", "example-old");
    state.set("example-old.recent", "[\"a\"]");
    state.set("example-old.size", "4");
    state.set("example-new.size", "9");
    state.set("board.example-old.pref", "x");
    state.set("notifications.example-old.mode", "system");
    state.set("example-old", "no dot");
    state.set("example-old-two.size", "2");
    state.set("app.other", "example-old");
    assert!(state.rename_board_ids(&example()));
    let mut expected = State::default();
    expected.set("app.dashboard", "example-new");
    expected.set("example-new.recent", "[\"a\"]");
    expected.set("example-new.size", "9");
    expected.set("board.example-new.pref", "x");
    expected.set("notifications.example-new.mode", "system");
    expected.set("example-old", "no dot");
    expected.set("example-old-two.size", "2");
    expected.set("app.other", "example-old");
    assert_eq!(state, expected);
    assert!(!state.rename_board_ids(&example()));
}

#[test]
fn the_framework_s_own_keys_in_a_space_s_namespace_never_move() {
    let account = crate::credentials::GITHUB_ACCOUNT_KEY;
    let mut from = State::default();
    from.set(account, "octo-dev");
    from.set("github.collapsed", "[]");
    assert!(from.rename_board_ids(&Renames::from([("github".into(), "example-new".into())])));
    assert_eq!(from.get(account), Some("octo-dev"));
    assert_eq!(from.get("example-new.collapsed"), Some("[]"));
    assert_eq!(from.get("example-new.activeAccount"), None);

    let mut onto = State::default();
    onto.set("example-old.activeAccount", "someone");
    assert!(!onto.rename_board_ids(&Renames::from([("example-old".into(), "github".into())])));
    assert_eq!(onto.get(account), None);
    assert_eq!(onto.get("example-old.activeAccount"), Some("someone"));
}

#[test]
fn the_state_s_own_namespaces_are_names_no_former_id_may_be() {
    let prefixes = super::STATE_KEYS.iter().filter_map(|shape| match shape {
        super::Shape::Prefixed(prefix) => prefix.strip_suffix('.'),
        super::Shape::Whole | super::Shape::Trimmed => None,
    });
    let active = super::ACTIVE_SPACE
        .split_once('.')
        .map(|(namespace, _)| namespace);
    let namespaces: Vec<&str> = prefixes.chain(active).collect();
    assert_eq!(namespaces, ["board", "notifications", "app"]);
    for name in namespaces {
        assert!(subspace_product::SAVED_NAMES.contains(&name), "{name}");
    }
}
