use std::collections::BTreeMap;
use std::sync::Arc;

use super::{InputRevision, shared};
mod launch_races;
#[cfg(unix)]
use crate::private_settings::{Edit, PrivateSettings};
use crate::project::{Component, ComponentKind, InputDeclaration, InputValue, ProjectDefinition};
#[cfg(unix)]
use subspace_framework::core::Secret;

fn definition() -> ProjectDefinition {
    let mut definition = ProjectDefinition::new("service-test", "python-default", "main.py");
    definition.components.push(Component::new(
        "summary",
        "Summary",
        ComponentKind::Text {
            text: "Ready".into(),
        },
    ));
    definition
}

fn definition_with_input() -> ProjectDefinition {
    let mut definition = definition();
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::Number {
            default: 1.,
            min: 0.,
            max: 10.,
            step: 1.,
        },
    );
    definition
}

fn stored_project(
    root: &std::path::Path,
    source: &[u8],
) -> anyhow::Result<(Arc<super::ProjectService>, crate::project::LoadedProject)> {
    let service = shared(root)?;
    let loaded = service.create(
        &definition(),
        BTreeMap::from([("main.py".into(), source.to_vec())]),
    )?;
    Ok((service, loaded))
}

#[test]
fn root_registry_reuses_one_service_and_separates_other_roots() {
    let first = tempfile::tempdir().expect("first root");
    let second = tempfile::tempdir().expect("second root");
    let a = shared(first.path()).expect("first service");
    let b = shared(first.path()).expect("same service");
    let c = shared(second.path()).expect("second service");
    assert!(Arc::ptr_eq(&a, &b));
    assert!(!Arc::ptr_eq(&a, &c));
    assert!(shared(std::path::Path::new("relative")).is_err());
}

#[test]
fn file_edits_use_expected_revision_through_the_shared_service() {
    let root = tempfile::tempdir().expect("service root");
    let (service, loaded) =
        stored_project(root.path(), b"print('original')").expect("stored project");
    let updated = service
        .edit_file(
            "service-test",
            "main.py",
            b"print('updated')",
            &loaded.revision,
        )
        .expect("edit");
    assert_ne!(updated.revision, loaded.revision);
    assert!(
        service
            .edit_file(
                "service-test",
                "main.py",
                b"stale overwrite",
                &loaded.revision
            )
            .is_err()
    );
    assert_eq!(
        service.load("service-test").expect("readback").files["main.py"],
        b"print('updated')"
    );
}

#[cfg(unix)]
#[test]
fn export_checks_all_saved_values_not_only_delegated_names() {
    let root = tempfile::tempdir().expect("service root");
    let (service, loaded) = stored_project(root.path(), b"print('test-only-undelgated-value')")
        .expect("stored project");
    let settings = PrivateSettings::new(root.path(), "service-test").expect("private settings");
    settings
        .update(
            0,
            BTreeMap::from([(
                "UNSELECTED".into(),
                Edit::Replace(Secret::new("test-only-undelgated-value")),
            )]),
        )
        .expect("save value");
    let destination = root.path().join("shared.zip");
    assert!(
        service
            .export("service-test", &loaded.revision, &destination)
            .is_err()
    );
    assert!(!destination.exists());
}

#[test]
fn input_requests_require_the_same_session_even_when_the_input_revision_matches() {
    let root = tempfile::tempdir().expect("service root");
    let service = shared(root.path()).expect("service");
    service
        .create(
            &definition_with_input(),
            BTreeMap::from([("main.py".into(), b"pass".to_vec())]),
        )
        .expect("create");
    let presentation = service
        .open("service-test")
        .expect("open without execution");
    let wrong = InputRevision::new("previous-worker", presentation.live.input_revision());
    assert!(
        service
            .set_value("service-test", "amount", InputValue::Number(2.), &wrong)
            .is_err()
    );
    let current = InputRevision::new(
        presentation.live.session_id(),
        presentation.live.input_revision(),
    );
    let mut changes = service.subscribe();
    assert_eq!(
        service
            .set_value("service-test", "amount", InputValue::Number(2.), &current)
            .expect("accepted input"),
        1
    );
    assert!(changes.has_changed().unwrap());
    changes.borrow_and_update();
    assert!(
        service
            .set_value("service-test", "amount", InputValue::Number(3.), &current)
            .is_err()
    );
    assert!(!changes.has_changed().unwrap());
}
