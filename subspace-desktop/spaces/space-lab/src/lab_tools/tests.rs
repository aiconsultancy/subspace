use serde_json::json;

use super::edit;
use super::inspect;
use crate::environments::EnvironmentManager;
use crate::studio::Studio;

#[test]
fn inspect_and_edit_use_the_injected_root_without_creating_it() {
    let directory = tempfile::tempdir().expect("temporary test directory");
    let root = directory.path().join("space-lab");
    let studio = Studio::new(root.clone());
    let manager = EnvironmentManager::new(root.join("environments"));
    let draft = inspect::run(
        &studio,
        &manager,
        &root,
        serde_json::from_value(json!({"action":"draft","profile":"market"})).expect("draft"),
    )
    .expect("draft");
    assert!(draft["preset"]["id"].is_string());
    let controlled = edit::run(
        &studio,
        &manager,
        &root,
        serde_json::from_value(json!({
            "action":"add_control",
            "preset": draft["preset"],
            "parameter":"currency"
        }))
        .expect("add_control"),
    )
    .expect("add_control");
    assert_eq!(controlled["saved"], false);
    assert!(!root.exists());
}

#[test]
fn inspect_network_policies_are_defaults_without_writing() {
    let directory = tempfile::tempdir().expect("temporary test directory");
    let root = directory.path().join("space-lab");
    let studio = Studio::new(root.clone());
    let manager = EnvironmentManager::new(root.join("environments"));
    let output = inspect::run(
        &studio,
        &manager,
        &root,
        serde_json::from_value(json!({"action":"network_policies"})).expect("policies"),
    )
    .expect("policies");
    assert_eq!(
        output["network_policies"]["python-data"][0],
        "api.github.com:443"
    );
    assert!(!root.exists());
}
