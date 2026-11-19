use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Role, TestAppContext, WindowHandle, px, size};
use subspace_core::settings::SettingsFile;

use super::{CONFIG, Control, fields, input, open};
use crate::settings::Settings;
use crate::settings::board_fields;

#[gpui_kit::test]
fn generic_text_and_secret_drafts_are_net_changes_even_when_invalid(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(!settings.has_unsaved_changes(cx));
            for (field, original, changed) in [
                (0, "", "not a URL"),
                (1, "3", "not a number"),
                (4, "", "test-only replacement secret"),
            ] {
                input(settings, field).update(cx, |input, cx| input.set_value(changed, window, cx));
                assert!(settings.has_unsaved_changes(cx), "field {field}");
                input(settings, field)
                    .update(cx, |input, cx| input.set_value(original, window, cx));
                assert!(!settings.has_unsaved_changes(cx), "reverted field {field}");
            }
            settings.toggle_field_clear(0, 4, window, cx);
            assert!(settings.has_unsaved_changes(cx));
            settings.toggle_field_clear(0, 4, window, cx);
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn generic_choices_and_grants_join_the_complete_save_baseline(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            for on in [true, false] {
                settings.set_field_toggle(0, 2, on);
                assert_eq!(settings.has_unsaved_changes(cx), on);
            }
            for on in [true, false] {
                settings.set_board_grant(0, "identity.settings", on);
                assert_eq!(settings.has_unsaved_changes(cx), on);
            }
            let Control::Choice { choice, .. } = &settings.board_fields[0].fields[3].control else {
                panic!("expected choice");
            };
            for value in ["dark", "light"] {
                choice.update(cx, |choice, cx| {
                    choice.set_selected_value(&value.to_owned(), window, cx)
                });
                assert_eq!(settings.has_unsaved_changes(cx), value == "dark");
            }
            settings.set_field_toggle(0, 2, true);
            settings.set_board_grant(0, "identity.settings", true);
            assert!(!settings.save(window, cx).unwrap());
            assert!(!settings.has_unsaved_changes(cx));
            settings.set_field_toggle(0, 2, false);
            assert!(settings.has_unsaved_changes(cx));
            settings.set_field_toggle(0, 2, true);
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn accepted_generic_secrets_keep_a_partial_save_dirty_until_retry(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    let state = root.path().join("state.json");
    std::fs::create_dir(&state).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            input(settings, 5).update(cx, |input, cx| {
                input.set_value("test-only new secret", window, cx)
            });
            subspace_dashboard::prefs::set(cx, "test.force-preference-write", "1");
            assert!(settings.save(window, cx).is_err());
            assert_eq!(input(settings, 5).read(cx).value(), "");
            assert!(!board_fields::changed(&settings.board_fields, cx));
            assert!(settings.has_unsaved_changes(cx));
        })
        .unwrap();
    let file = SettingsFile::open(&root.path().join("config.json")).unwrap();
    assert!(file.board_secret_present(&fields().section, "key"));
    std::fs::remove_dir(&state).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            assert!(!settings.save(window, cx).unwrap());
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn generic_boolean_and_permission_switches_edit_only_the_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    cx.simulate_window_resize(settings.into(), size(px(1100.), px(1700.)));
    settings
        .update(cx, |settings, _, cx| {
            settings.select_initial_board("notes", cx)
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        for (id, label) in [
            ("board-field-notes-compact", "compact"),
            ("board-grant-notes-identity.settings", "Read settings"),
        ] {
            let switch = window.find(id);
            assert_eq!(switch.role(), Some(Role::Switch));
            assert_eq!(switch.label(), Some(label));
            assert_eq!(switch.checked(), Some(false));
            window.click(id, cx);
            assert_eq!(window.find(id).checked(), Some(true));
            window.press("space", cx);
            assert_eq!(window.find(id).checked(), Some(false));
        }
    })
    .unwrap();
    settings
        .update(cx, |settings, _, cx| {
            assert!(!settings.has_unsaved_changes(cx))
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        CONFIG
    );
}

fn open_notes_page(root: &std::path::Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    let settings = open(root, cx);
    cx.simulate_window_resize(settings.into(), size(px(900.), px(1500.)));
    settings
        .update(cx, |settings, _, cx| {
            settings.page = crate::settings::SettingsPage::Board("notes");
            cx.notify();
        })
        .unwrap();
    // The page's elements are observed from the frame after it opens.
    for _ in 0..2 {
        cx.update_window(settings.into(), |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    settings
}

fn hint_in(settings: WindowHandle<Settings>, row: &'static str, cx: &mut TestAppContext) -> bool {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        window.within(row).try_find("saved-token-hint").is_some()
    })
    .unwrap()
}

fn click_in(
    settings: WindowHandle<Settings>,
    (row, target): (&'static str, &'static str),
    cx: &mut TestAppContext,
) {
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        window.within(row).click(target, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn type_text(settings: WindowHandle<Settings>, text: &str, cx: &mut TestAppContext) -> String {
    cx.update_window(settings.into(), |_, window, cx| window.input(text, cx))
        .unwrap();
    settings
        .update(cx, |settings, _, cx| {
            input(settings, 4).read(cx).value().to_string()
        })
        .unwrap()
}

const SAVED: &str = "board-field-notes-token";
const EMPTY: &str = "board-field-notes-key";

#[gpui_kit::test]
fn a_saved_secret_is_marked_and_explains_why_it_is_read_only(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_notes_page(root.path(), cx);
    cx.update_window(settings.into(), |_, window, _| {
        assert!(window.within(SAVED).find("saved-token-state").visible());
        assert!(window.within(EMPTY).try_find("saved-token-state").is_none());
    })
    .unwrap();
    assert!(!hint_in(settings, SAVED, cx), "shown on click");
    click_in(settings, (EMPTY, "saved-token-input"), cx);
    assert!(
        !hint_in(settings, EMPTY, cx),
        "only a saved token is locked"
    );
    click_in(settings, (SAVED, "saved-token-input"), cx);
    assert!(hint_in(settings, SAVED, cx));
    assert_eq!(type_text(settings, "test-only blocked input", cx), "");
}

#[gpui_kit::test]
fn clearing_a_saved_secret_accepts_a_replacement(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_notes_page(root.path(), cx);
    click_in(settings, (SAVED, "saved-token-input"), cx);
    click_in(settings, (SAVED, "clear-saved-token"), cx);
    assert!(!hint_in(settings, SAVED, cx));
    cx.update_window(settings.into(), |_, window, _| {
        assert!(window.within(SAVED).try_find("saved-token-state").is_none());
    })
    .unwrap();
    click_in(settings, (SAVED, "secret-input"), cx);
    assert_eq!(
        type_text(settings, "test-only replacement", cx),
        "test-only replacement"
    );
}
