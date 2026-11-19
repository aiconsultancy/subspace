use gpui_kit::TestAppContext;

use super::{assert_dirty, open_settings};

#[gpui_kit::test]
fn discovered_account_default_is_clean_without_synthesizing_a_saved_preference(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        crate::theme::initialize(cx);
    });
    let file =
        subspace_core::settings::SettingsFile::open(&root.path().join("config.json")).unwrap();
    let accounts = subspace_core::credentials::GitHubAccounts {
        logins: vec!["test-account-one".into(), "test-account-two".into()],
        active: Some("test-account-two".into()),
        signed_in: Vec::new(),
    };
    let settings =
        cx.add_window(move |window, cx| crate::settings::Settings::new(file, accounts, window, cx));
    settings
        .update(cx, |settings, window, cx| {
            let key = subspace_core::credentials::GITHUB_ACCOUNT_KEY;
            assert_dirty(settings, false, cx);
            super::choose(&settings.account, "test-account-one", window, cx);
            assert_dirty(settings, true, cx);
            super::choose(&settings.account, "test-account-two", window, cx);
            assert_dirty(settings, false, cx);
            assert!(!settings.save(window, cx).unwrap());
            assert_eq!(subspace_dashboard::prefs::get(cx, key), None);
        })
        .unwrap();
}

#[gpui_kit::test]
fn full_save_replaces_the_baseline_but_partial_failure_does_not(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.confirm_quit = true;
            assert_dirty(settings, true, cx);
            assert!(!settings.save(window, cx).unwrap());
            assert_dirty(settings, false, cx);
            settings.confirm_quit = false;
            assert_dirty(settings, true, cx);
            settings.confirm_quit = true;
            assert_dirty(settings, false, cx);
            settings.form.tokens[0].clear = true;
        })
        .unwrap();
    let state = root.path().join("state.json");
    std::fs::remove_file(&state).unwrap();
    std::fs::create_dir(&state).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            // Force a real preference-file write even when the appearance
            // choices are already saved. This external key is not a UI draft.
            subspace_dashboard::prefs::set(cx, "test.force-preference-write", "1");
            assert!(settings.save(window, cx).is_err());
            // Config accepted and cleared the token input, but the full save failed.
            assert!(!settings.form.tokens[0].clear);
            assert_dirty(settings, true, cx);
        })
        .unwrap();
    std::fs::remove_dir(&state).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.save(window, cx).unwrap());
            assert_dirty(settings, false, cx);
        })
        .unwrap();
}

#[gpui_kit::test]
fn config_changed_externally_still_rejects_save_and_keeps_draft_dirty(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    std::fs::write(root.path().join("config.json"), "{\"autoRefresh\":false}").unwrap();
    settings
        .update(cx, |settings, window, cx| {
            settings.move_board("future", 1, cx);
            assert!(
                settings
                    .save(window, cx)
                    .unwrap_err()
                    .contains("changed externally")
            );
            assert_dirty(settings, true, cx);
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        "{\"autoRefresh\":false}"
    );
}
