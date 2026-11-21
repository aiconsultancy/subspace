use std::collections::BTreeSet;

use super::{Choice, names, replacement_edits};
use crate::private_settings::Edit;

#[test]
fn names_are_the_sorted_union_of_declared_and_saved_names() {
    let declared = vec!["SELECTED".to_owned(), "BOTH".to_owned()];
    let saved = vec!["SAVED".to_owned(), "BOTH".to_owned()];
    assert_eq!(names(&declared, &saved), ["BOTH", "SAVED", "SELECTED"]);
}

#[test]
fn edits_preserve_keep_replace_and_clear_including_empty_replacements() {
    let edits = replacement_edits([
        ("ONE".to_owned(), Choice::Keep, String::new()),
        ("TWO".to_owned(), Choice::Replace, String::new()),
        ("THREE".to_owned(), Choice::Clear, String::new()),
    ]);
    assert!(matches!(edits.get("ONE"), Some(Edit::Keep)));
    assert!(matches!(edits.get("THREE"), Some(Edit::Clear)));
    match edits.get("TWO") {
        Some(Edit::Replace(value)) => assert_eq!(value.expose(), ""),
        _ => panic!("expected replacement"),
    }
}

#[test]
fn conflict_approval_requires_all_names() {
    let conflicts = vec!["ALPHA".to_owned(), "BETA".to_owned()];
    let mut approved = BTreeSet::from(["ALPHA".to_owned()]);
    assert!(!super::all_approved(&conflicts, &approved));
    approved.insert("BETA".to_owned());
    assert!(super::all_approved(&conflicts, &approved));
}

#[cfg(unix)]
mod native {
    use std::collections::BTreeMap;

    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt as _;
    use subspace_framework::core::Secret;

    use super::super::{Choice, ProjectSettings};
    use crate::private_settings::{Edit, PrivateSettings};
    use crate::project::{LoadedProject, ProjectDefinition, ProjectRevision};

    fn fixture(root: &std::path::Path) -> LoadedProject {
        let mut definition = ProjectDefinition::new("private-test", "python", "main.py");
        definition.environment_keys = vec!["SELECTED".into(), "BOTH".into()];
        LoadedProject::new(
            definition,
            ProjectRevision::new("d", "s"),
            BTreeMap::new(),
            root.to_owned(),
        )
    }

    #[gpui_kit::test]
    fn choices_expose_pressed_state_and_ignore_input_when_busy(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let root = tempfile::tempdir().unwrap();
        let project = fixture(root.path());
        let path = root.path().to_owned();
        let (view, cx) =
            cx.add_window_view(move |window, cx| ProjectSettings::new(path, project, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("private-BOTH-Keep").checked(), Some(true));
            assert_eq!(window.find("private-BOTH-Replace").checked(), Some(false));
            window.click("private-BOTH-Replace", cx);
            assert_eq!(window.find("private-BOTH-Keep").checked(), Some(false));
            assert_eq!(window.find("private-BOTH-Replace").checked(), Some(true));
            assert_eq!(
                view.read(cx).rows["BOTH"].input.read(cx).value().as_ref(),
                ""
            );
            view.update(cx, |view, cx| {
                view.busy = true;
                cx.notify();
            });
            window.render_frame(cx);
            window.click("private-BOTH-Clear", cx);
            assert!(view.read(cx).rows["BOTH"].choice == Choice::Replace);
        });
    }

    #[gpui_kit::test]
    fn saved_values_never_prefill_and_stale_save_keeps_new_input(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let root = tempfile::tempdir().unwrap();
        let store = PrivateSettings::new(root.path(), "private-test").unwrap();
        store
            .update(
                0,
                BTreeMap::from([
                    (
                        "BOTH".into(),
                        Edit::Replace(Secret::new("dummy saved value")),
                    ),
                    (
                        "SAVED".into(),
                        Edit::Replace(Secret::new("dummy other value")),
                    ),
                ]),
            )
            .unwrap();
        let project = fixture(root.path());
        let path = root.path().to_owned();
        let (view, cx) =
            cx.add_window_view(move |window, cx| ProjectSettings::new(path, project, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.rows.len(), 3);
                assert_eq!(view.rows["BOTH"].input.read(cx).value().as_ref(), "");
                let row = view.rows.get_mut("BOTH").unwrap();
                row.choice = Choice::Replace;
                row.input.update(cx, |input, cx| {
                    input.set_value("dummy new value", window, cx)
                });
            });
        });
        store.update(1, BTreeMap::new()).unwrap();
        cx.update(|window, cx| view.update(cx, |view, cx| view.save(window, cx)));
        cx.run_until_parked();
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                assert!(view.error.is_some());
                assert_eq!(
                    view.rows["BOTH"].input.read(cx).value().as_ref(),
                    "dummy new value"
                );
            });
        });
    }

    #[gpui_kit::test]
    fn import_conflicts_need_approval_and_success_clears_replacement_input(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let root = tempfile::tempdir().unwrap();
        let store = PrivateSettings::new(root.path(), "private-test").unwrap();
        store
            .update(
                0,
                BTreeMap::from([("BOTH".into(), Edit::Replace(Secret::new("dummy old")))]),
            )
            .unwrap();
        let import_path = root.path().join("synthetic.env");
        std::fs::write(&import_path, "BOTH=dummy-import\nSAVED=dummy-new\n").unwrap();
        let project = fixture(root.path());
        let path = root.path().to_owned();
        let (view, cx) =
            cx.add_window_view(move |window, cx| ProjectSettings::new(path, project, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.path.update(cx, |input, cx| {
                    input.set_value(import_path.to_string_lossy().to_string(), window, cx)
                });
                view.preview_import(window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.preview.as_ref().unwrap().conflicts(), ["BOTH"]);
                view.apply_import(window, cx);
                assert!(view.preview.is_some());
                view.approved.insert("BOTH".into());
                view.apply_import(window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.metadata.as_ref().unwrap().revision(), 2);
                let row = view.rows.get_mut("BOTH").unwrap();
                row.choice = Choice::Replace;
                row.input.update(cx, |input, cx| {
                    input.set_value("dummy replacement", window, cx)
                });
                view.save(window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.rows["BOTH"].input.read(cx).value().as_ref(), "");
                assert!(view.rows["BOTH"].choice == Choice::Keep);
                assert_eq!(view.metadata.as_ref().unwrap().revision(), 3);
            })
        });
    }
}
