use super::{Edit, replace, store};
use std::collections::BTreeMap;
use subspace_framework::core::Secret;

#[test]
fn selected_values_are_explicit_and_budgets_and_reserved_names_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let settings = store(root.path(), "project").unwrap();
    let initial = settings.metadata().unwrap();
    assert!(
        settings
            .update(initial.revision(), replace("PATH", "fake"))
            .is_err()
    );
    assert!(
        settings
            .update(initial.revision(), replace("BAD-NAME", "fake"))
            .is_err()
    );
    assert!(
        settings
            .update(
                initial.revision(),
                replace("TOO_BIG", &"x".repeat(16 * 1024 + 1))
            )
            .is_err()
    );
    let many = (0..65)
        .map(|n| (format!("VAR_{n}"), Edit::Replace(Secret::new("fake"))))
        .collect();
    assert!(settings.update(initial.revision(), many).is_err());
    let total = (0..9)
        .map(|n| {
            (
                format!("VAR_{n}"),
                Edit::Replace(Secret::new("x".repeat(16 * 1024))),
            )
        })
        .collect();
    assert!(settings.update(initial.revision(), total).is_err());
    let edits = BTreeMap::from([
        ("FIRST".into(), Edit::Replace(Secret::new("fake-one"))),
        ("SECOND".into(), Edit::Replace(Secret::new("fake-two"))),
    ]);
    settings.update(initial.revision(), edits).unwrap();
    let values = settings.selected(&["SECOND"]).unwrap().into_values();
    assert_eq!(values.len(), 1);
    assert_eq!(values["SECOND"], "fake-two");
}
