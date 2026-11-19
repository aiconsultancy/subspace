mod form;
mod persistence;

use gpui_kit::component::select::{SelectDelegate, SelectItem};
use gpui_kit::{App, TestAppContext, Window};
use subspace_dashboard::{prefs, refresh};

use super::{Settings, form::Picker, organization_tests::open_settings, pickers::FontItem};

fn choose<D>(choice: &Picker<D>, value: &str, window: &mut Window, cx: &mut App)
where
    D: SelectDelegate + 'static,
    D::Item: SelectItem<Value = String>,
{
    choice.update(cx, |choice, cx| {
        choice.set_selected_value(&value.to_owned(), window, cx)
    });
    assert_eq!(
        choice.read(cx).selected_value().map(String::as_str),
        Some(value)
    );
}

fn assert_dirty(settings: &Settings, expected: bool, cx: &App) {
    assert_eq!(settings.has_unsaved_changes(cx), expected);
}

#[gpui_kit::test]
fn new_editor_is_clean_and_reverted_preferences_are_clean(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, _, cx| {
            assert!(!settings.has_unsaved_changes(cx));
            settings.check_updates = !settings.check_updates;
            assert!(settings.has_unsaved_changes(cx));
            settings.check_updates = !settings.check_updates;
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
}

#[gpui_kit::test]
fn preferences_and_live_previews_compare_to_the_opening_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            for field in 0..4 {
                let toggle = |settings: &mut Settings| match field {
                    0 => settings.confirm_quit = !settings.confirm_quit,
                    1 => settings.check_updates = !settings.check_updates,
                    2 => settings.auto_refresh = !settings.auto_refresh,
                    _ => settings.approve_all_default = !settings.approve_all_default,
                };
                toggle(settings);
                assert_dirty(settings, true, cx);
                toggle(settings);
                assert_dirty(settings, false, cx);
            }
            for index in 0..settings.toolbar.labels.len() {
                settings.toolbar.labels[index] = !settings.toolbar.labels[index];
                assert_dirty(settings, true, cx);
                settings.toolbar.labels[index] = !settings.toolbar.labels[index];
                assert_dirty(settings, false, cx);
            }
            settings.toolbar.menu = !settings.toolbar.menu;
            assert_dirty(settings, true, cx);
            settings.toolbar.menu = !settings.toolbar.menu;
            assert_dirty(settings, false, cx);
            choose(&settings.themes, "Solarized Dark", window, cx);
            settings.preview(window, cx);
            assert_dirty(settings, true, cx);
            choose(&settings.themes, "Solarized Light", window, cx);
            settings.preview(window, cx);
            assert_dirty(settings, false, cx);
            for input in &settings.font_sizes {
                let original = input.read(cx).value().to_string();
                input.update(cx, |input, cx| input.set_value("16", window, cx));
                assert_dirty(settings, true, cx);
                input.update(cx, |input, cx| input.set_value(original, window, cx));
                assert_dirty(settings, false, cx);
            }
        })
        .unwrap();
}

#[gpui_kit::test]
fn layout_notifications_and_polling_are_net_changes(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            // Non-macOS hosts offer only Silent. Exercise sound draft comparison
            // with real Select state, independent of installed system sounds.
            settings.sources[0].sound.update(cx, |choice, cx| {
                choice.set_items(
                    gpui_kit::component::select::SearchableVec::new(vec![
                        "Silent".into(),
                        "Ping".into(),
                    ]),
                    window,
                    cx,
                );
                choice.set_selected_value(&"Silent".to_owned(), window, cx);
            });
            assert_dirty(settings, false, cx);
            for (choice, changed) in [
                (&settings.board_navigation.mode, "Vertical tabs"),
                (&settings.sources[0].mode, "Both"),
                (&settings.sources[0].style, "Persistent"),
                (&settings.sources[0].sound, "Ping"),
                (&settings.background_updates[0].interval, "5 minutes"),
            ] {
                let original = choice.read(cx).selected_value().unwrap().clone();
                choose(choice, changed, window, cx);
                assert_dirty(settings, true, cx);
                choose(choice, &original, window, cx);
                assert_dirty(settings, false, cx);
            }
            settings.board_navigation.collapsed = !settings.board_navigation.collapsed;
            assert_dirty(settings, true, cx);
            settings.board_navigation.collapsed = !settings.board_navigation.collapsed;
            settings.notifications.sound = !settings.notifications.sound;
            assert_dirty(settings, true, cx);
            settings.notifications.sound = !settings.notifications.sound;
            settings.notifications.sounds = !settings.notifications.sounds;
            assert_dirty(settings, true, cx);
            settings.notifications.sounds = !settings.notifications.sounds;
            settings.background_updates[0].enabled = !settings.background_updates[0].enabled;
            assert_dirty(settings, true, cx);
            settings.background_updates[0].enabled = !settings.background_updates[0].enabled;
            assert_dirty(settings, false, cx);
            // Changes elsewhere must neither dirty this untouched editor nor clean it.
            prefs::set(cx, super::CONFIRM_QUIT_KEY, "true");
            refresh::Preferences {
                enabled: false,
                interval_seconds: Some(75),
            }
            .save("future", cx);
            assert_dirty(settings, false, cx);
            settings.notifications.os = !settings.notifications.os;
            prefs::set(cx, crate::notifications::OS_KEY, "1");
            assert_dirty(settings, true, cx);
        })
        .unwrap();
}

#[gpui_kit::test]
fn every_font_role_tracks_the_draft_after_live_preview_and_revert(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            for index in 0..settings.fonts.len() {
                let original = settings.fonts[index]
                    .read(cx)
                    .selected_value()
                    .unwrap()
                    .clone();
                // The headless platform enumerates no installed fonts. Keep the
                // real Select entity and provide one test-only fallback choice.
                let alternate = "Test-only unavailable font";
                settings.fonts[index].update(cx, |choice, cx| {
                    choice.set_items(
                        gpui_kit::component::select::SearchableVec::new(vec![
                            FontItem::new(original.clone()),
                            FontItem::new(alternate.into()),
                        ]),
                        window,
                        cx,
                    );
                    choice.set_selected_value(&original, window, cx);
                });
                assert_dirty(settings, false, cx);
                choose(&settings.fonts[index], alternate, window, cx);
                settings.preview(window, cx);
                assert_dirty(settings, true, cx);
                choose(&settings.fonts[index], &original, window, cx);
                settings.preview(window, cx);
                assert_dirty(settings, false, cx);
            }
        })
        .unwrap();
}
