mod fixture;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, TestAppContext, Window, WindowHandle};
use subspace_core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_core::settings::{SettingsFile, TokenEdit};
use subspace_dashboard::prefs;

use crate::settings::{Settings, form::GITHUB_TOKEN};
use fixture::{INITIAL, OTHER, REPLACEMENT, open};

fn rendered<T>(
    settings: WindowHandle<Settings>,
    cx: &mut TestAppContext,
    check: impl FnOnce(&mut Window, &mut App) -> T,
) -> T {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        check(window, cx)
    })
    .unwrap()
}

fn navigate(settings: WindowHandle<Settings>, destination: &str, cx: &mut TestAppContext) {
    rendered(settings, cx, |window, cx| {
        window.click(format!("settings-{destination}"), cx);
    });
    cx.run_until_parked();
}

fn identities(
    settings: WindowHandle<Settings>,
    cx: &mut TestAppContext,
) -> (gpui_kit::EntityId, gpui_kit::EntityId) {
    settings
        .read_with(cx, |settings, _| {
            (
                settings.account.entity_id(),
                settings.form.tokens[GITHUB_TOKEN].input.entity_id(),
            )
        })
        .unwrap()
}

fn assert_saved_controls(window: &mut Window, repositories: bool) {
    assert_eq!(
        window.find("settings-github-account").label(),
        Some("GitHub account")
    );
    assert!(
        window
            .within(("saved-token", GITHUB_TOKEN))
            .find("saved-token-state")
            .visible()
    );
    assert_eq!(
        window
            .within(("saved-token", GITHUB_TOKEN))
            .find("secret-input")
            .value(),
        None
    );
    assert_eq!(
        window.try_find("settings-repositories").is_some(),
        repositories
    );
}

#[gpui_kit::test]
fn board_pages_and_generic_extension_render_one_shared_account_and_masked_saved_token(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    let original = identities(settings, cx);
    for destination in [
        "board:github",
        "board:actions",
        "board:github-copilot-agents",
        "extension:fixture-import",
    ] {
        navigate(settings, destination, cx);
        rendered(settings, cx, |window, _| {
            assert_saved_controls(window, destination.starts_with("board:"))
        });
        assert_eq!(identities(settings, cx), original);
        settings
            .read_with(cx, |settings, cx| {
                assert!(
                    settings.form.tokens[GITHUB_TOKEN]
                        .input
                        .read(cx)
                        .value()
                        .is_empty()
                );
            })
            .unwrap();
    }
    rendered(settings, cx, |window, _| {
        assert!(window.find("fixture-extension-view").visible())
    });
    navigate(settings, "extension:fixture-plain", cx);
    rendered(settings, cx, |window, _| {
        assert!(window.find("fixture-extension-view").visible());
        assert!(window.try_find("settings-github-account").is_none());
        assert!(window.try_find("settings-repositories").is_none());
    });
}

fn choose_other(settings: WindowHandle<Settings>, cx: &mut TestAppContext) {
    rendered(settings, cx, |window, cx| {
        window.click("settings-github-account", cx)
    });
    cx.run_until_parked();
    rendered(settings, cx, |window, cx| {
        window.press("down", cx);
        window.press("enter", cx);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn account_selection_restores_focus_and_stays_a_shared_draft_until_save(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    navigate(settings, "extension:fixture-import", cx);
    choose_other(settings, cx);
    settings
        .read_with(cx, |settings, cx| {
            assert_eq!(
                settings
                    .account
                    .read(cx)
                    .selected_value()
                    .map(String::as_str),
                Some(OTHER)
            );
            assert_eq!(prefs::get(cx, GITHUB_ACCOUNT_KEY).as_deref(), Some(INITIAL));
            assert!(settings.has_unsaved_changes(cx));
        })
        .unwrap();
    rendered(settings, cx, |window, _| {
        assert_eq!(window.find("settings-github-account").focused(), Some(true))
    });
    navigate(settings, "board:github", cx);
    rendered(settings, cx, |window, cx| {
        window.click("settings-github-account", cx)
    });
    cx.run_until_parked();
    rendered(settings, cx, |window, cx| window.press("escape", cx));
    cx.run_until_parked();
    rendered(settings, cx, |window, _| {
        assert_eq!(window.find("settings-github-account").focused(), Some(true))
    });
    navigate(settings, "extension:fixture-import", cx);
    rendered(settings, cx, |window, cx| window.click("fixture-save", cx));
    cx.run_until_parked();
    settings
        .read_with(cx, |settings, cx| {
            assert!(!settings.has_unsaved_changes(cx));
            assert_eq!(prefs::get(cx, GITHUB_ACCOUNT_KEY).as_deref(), Some(OTHER));
        })
        .unwrap();
}

fn replace_token(settings: WindowHandle<Settings>, cx: &mut TestAppContext) {
    rendered(settings, cx, |window, cx| {
        window
            .within(("saved-token", GITHUB_TOKEN))
            .click("clear-saved-token", cx);
        window
            .within(("saved-token", GITHUB_TOKEN))
            .click("secret-input", cx);
        window.input(REPLACEMENT, cx);
        assert_eq!(
            window
                .within(("saved-token", GITHUB_TOKEN))
                .find("secret-input")
                .value(),
            None
        );
    });
}

fn undo_clear_and_reject_input(settings: WindowHandle<Settings>, cx: &mut TestAppContext) {
    rendered(settings, cx, |window, cx| {
        window
            .within(("saved-token", GITHUB_TOKEN))
            .click("clear-saved-token", cx);
        assert!(
            window
                .within(("saved-token", GITHUB_TOKEN))
                .find("saved-token-state")
                .visible()
        );
        window
            .within(("saved-token", GITHUB_TOKEN))
            .click("secret-input", cx);
        window.input("test-only-blocked-input", cx);
    });
    settings
        .read_with(cx, |settings, cx| {
            assert!(
                settings.form.tokens[GITHUB_TOKEN]
                    .input
                    .read(cx)
                    .value()
                    .is_empty()
            );
            assert!(matches!(
                settings.form.tokens[GITHUB_TOKEN].edit(cx),
                TokenEdit::Keep
            ));
        })
        .unwrap();
}

#[gpui_kit::test]
fn clear_and_replacement_share_the_board_draft_and_cancel_keeps_disk_and_account(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    let original_config = std::fs::read(root.path().join("config.json")).unwrap();
    let original_state = std::fs::read(root.path().join("state.json")).unwrap();
    navigate(settings, "extension:fixture-import", cx);
    choose_other(settings, cx);
    replace_token(settings, cx);
    navigate(settings, "board:github", cx);
    settings
        .read_with(cx, |settings, cx| {
            let token = &settings.form.tokens[GITHUB_TOKEN];
            assert!(token.clear);
            assert_eq!(token.input.read(cx).value().as_ref(), REPLACEMENT);
            assert!(matches!(token.edit(cx), TokenEdit::Replace(_)));
            assert!(settings.has_unsaved_changes(cx));
        })
        .unwrap();
    undo_clear_and_reject_input(settings, cx);
    replace_token(settings, cx);
    navigate(settings, "extension:fixture-import", cx);
    rendered(settings, cx, |window, cx| {
        window.click("fixture-cancel", cx)
    });
    cx.run_until_parked();
    assert!(settings.read_with(cx, |_, _| ()).is_err());
    assert_eq!(
        std::fs::read(root.path().join("config.json")).unwrap(),
        original_config
    );
    assert_eq!(
        std::fs::read(root.path().join("state.json")).unwrap(),
        original_state
    );
    cx.update(|cx| assert_eq!(prefs::get(cx, GITHUB_ACCOUNT_KEY).as_deref(), Some(INITIAL)));
}

#[gpui_kit::test]
fn token_replacement_on_extension_saves_once_and_empties_the_shared_input(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    navigate(settings, "extension:fixture-import", cx);
    replace_token(settings, cx);
    rendered(settings, cx, |window, cx| window.click("fixture-save", cx));
    cx.run_until_parked();
    settings
        .read_with(cx, |settings, cx| {
            assert!(!settings.form.tokens[GITHUB_TOKEN].clear);
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
    let saved = subspace_core::config::load(&root.path().join("config.json")).0;
    assert_eq!(saved.github.token.as_ref().unwrap().expose(), REPLACEMENT);
    assert!(
        SettingsFile::open(&root.path().join("config.json"))
            .unwrap()
            .token_present("github")
    );
}
