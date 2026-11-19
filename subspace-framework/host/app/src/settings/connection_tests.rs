//! Characterization of the shared connection editor before board dispatch moves.

mod fixture;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_core::settings::{SettingsFile, TokenEdit};

use fixture::{control_ids, open, rendered, show};

use super::form::GITHUB_TOKEN;

const FORMS: &[(&str, &[&str])] = &[
    (
        "actions",
        &["GitHub account", "GitHub token", "Repositories"],
    ),
    (
        "github-copilot-agents",
        &["GitHub account", "GitHub token", "Repositories"],
    ),
    ("terminal", &[]),
];

/// The favourite repositories list, after the account picker and the token.
const REPOSITORIES_FIELD: usize = 2;

#[gpui_kit::test]
fn connection_forms_keep_control_labels_order_alignment_and_shared_repositories(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), false, cx);
    let identity = settings
        .read_with(cx, |settings, _| settings.form.repos.entity_id())
        .unwrap();
    for (board, labels) in FORMS {
        show(settings, board, cx);
        let ids = control_ids(settings, board, cx);
        rendered(settings, cx, |window, _| {
            let bounds: Vec<_> = ids
                .iter()
                .zip(*labels)
                .map(|(id, label)| {
                    let control = window.find(id.clone());
                    assert_eq!(control.label(), Some(*label));
                    control.bounds()
                })
                .collect();
            for pair in bounds.windows(2) {
                assert!(pair[0].bottom() <= pair[1].top(), "{board}: ordered rows");
                assert_eq!(pair[0].left(), pair[1].left(), "{board}: aligned controls");
                assert_eq!(pair[0].right(), pair[1].right(), "{board}: equal widths");
            }
            assert_eq!(
                window.try_find("settings-repositories").is_some(),
                matches!(*board, "actions" | "github-copilot-agents")
            );
            assert_eq!(
                window.try_find("secret-input").is_some(),
                *board != "terminal"
            );
        });
        assert_eq!(
            settings
                .read_with(cx, |settings, _| settings.form.repos.entity_id())
                .unwrap(),
            identity
        );
    }
}

#[gpui_kit::test]
fn the_service_token_starts_empty_blank_save_retains_and_clear_is_private(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), true, cx);
    settings
        .update(cx, |settings, window, cx| {
            for token in &settings.form.tokens {
                assert!(token.input.read(cx).value().is_empty());
                assert!(matches!(token.edit(cx), TokenEdit::Keep));
            }
            assert!(!settings.save(window, cx).unwrap());
        })
        .unwrap();
    let path = root.path().join("config.json");
    assert!(SettingsFile::open(&path).unwrap().token_present("github"));
    settings
        .update(cx, |settings, _, cx| {
            settings
                .form
                .set_board_enabled("github-copilot-agents", true);
            cx.notify();
        })
        .unwrap();
    show(settings, "github-copilot-agents", cx);
    rendered(settings, cx, |window, cx| {
        window
            .within(("saved-token", GITHUB_TOKEN))
            .click("clear-saved-token", cx);
    });
    settings
        .update(cx, |settings, window, cx| {
            assert!(
                settings.save(window, cx).unwrap(),
                "service edits need restart"
            );
            let token = &settings.form.tokens[GITHUB_TOKEN];
            assert!(token.input.read(cx).value().is_empty());
            assert!(!token.clear);
        })
        .unwrap();
    assert!(!SettingsFile::open(&path).unwrap().token_present("github"));
    let state = std::fs::read_to_string(root.path().join("state.json")).unwrap();
    assert!(!state.contains("test-only-nonworking"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[gpui_kit::test]
fn disabled_connection_pages_block_text_secret_and_clear_without_losing_values(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), true, cx);
    for board in ["actions", "github-copilot-agents"] {
        settings
            .update(cx, |settings, _, cx| {
                settings.form.set_board_enabled(board, false);
                cx.notify();
            })
            .unwrap();
        show(settings, board, cx);
        let target = control_ids(settings, board, cx)[REPOSITORIES_FIELD].clone();
        rendered(settings, cx, |window, cx| {
            assert!(window.find("settings-board-off").visible());
            window.click(target, cx);
            window.input("test-only-blocked", cx);
            window
                .within(("saved-token", GITHUB_TOKEN))
                .click("clear-saved-token", cx);
            window
                .within(("saved-token", GITHUB_TOKEN))
                .click("secret-input", cx);
            window.input("test-only-blocked", cx);
        });
        settings
            .read_with(cx, |settings, cx| {
                let token = &settings.form.tokens[GITHUB_TOKEN];
                assert!(!token.clear);
                assert!(token.input.read(cx).value().is_empty());
                assert_eq!(settings.form.repos.read(cx).favourites(), ["owner/repo"]);
            })
            .unwrap();
    }
}

#[gpui_kit::test]
fn cancel_discards_connection_edits_and_replacement_save_requires_restart(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), true, cx);
    let path = root.path().join("config.json");
    let original = std::fs::read(&path).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            settings
                .form
                .repos
                .update(cx, |list, cx| list.toggle("owner/changed", cx));
            settings.toggle_clear(GITHUB_TOKEN, window, cx);
            settings.form.tokens[GITHUB_TOKEN]
                .input
                .update(cx, |input, cx| {
                    input.set_value("test-only-nonworking-replacement", window, cx)
                });
            assert!(settings.has_unsaved_changes(cx));
            assert!(settings.restart_required(cx));
            settings.restore_preview(window, cx).unwrap();
            window.remove_window();
        })
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let reopened = open(root.path(), true, cx);
    reopened
        .update(cx, |settings, window, cx| {
            settings.toggle_clear(GITHUB_TOKEN, window, cx);
            settings.form.tokens[GITHUB_TOKEN]
                .input
                .update(cx, |input, cx| {
                    input.set_value("test-only-nonworking-replacement", window, cx)
                });
            assert!(settings.save(window, cx).unwrap());
            assert!(
                settings.form.tokens[GITHUB_TOKEN]
                    .input
                    .read(cx)
                    .value()
                    .is_empty()
            );
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
    let config = subspace_core::config::load(&path).0;
    assert_eq!(
        config.github.token.unwrap().expose(),
        "test-only-nonworking-replacement"
    );
    assert!(
        !std::fs::read_to_string(root.path().join("state.json"))
            .unwrap()
            .contains("test-only-nonworking")
    );
}
