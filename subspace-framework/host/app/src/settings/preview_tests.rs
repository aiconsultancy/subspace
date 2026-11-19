use std::path::Path;

use gpui_kit::component::Theme;
use gpui_kit::{TestAppContext, WindowHandle, px};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::features::PI_CHAT;
use subspace_core::settings::SettingsFile;
use subspace_core::state::State;
use subspace_dashboard::density::{self, Density, DensityPreview};
use subspace_dashboard::{Tokens, panel, prefs};

use super::Settings;
use super::size_tests::type_font_size;
use crate::theme::{self, THEME_PREF};

pub(super) fn open_settings(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    open_settings_with_config(root, "{}", cx)
}

fn open_settings_with_config(
    root: &Path,
    source: &str,
    cx: &mut TestAppContext,
) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");
        theme::initialize(cx);
        prefs::save(cx).unwrap();
    });
    let path = root.join("config.json");
    std::fs::write(&path, source).unwrap();
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

#[gpui_kit::test]
fn settings_preview_uses_the_density_guard_and_cancel_restores_it(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(cx.global::<DensityPreview>().0);
            settings.themes.update(cx, |themes, cx| {
                themes.set_selected_value(&"Solarized Dark".to_owned(), window, cx);
            });
            settings.preview(window, cx);
            cx.global_mut::<Density>().set("--row-h", 41.0);
            panel::save_density(cx);
            prefs::save(cx).unwrap();
            settings.restore_preview(window, cx).unwrap();
            assert!(!cx.global::<DensityPreview>().0);
            assert_eq!(Theme::global(cx).theme_name().as_ref(), "Solarized Light");
            assert_eq!(cx.global::<Density>().get("--row-h"), 28.0);
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get(THEME_PREF), Some("Solarized Light"));
    assert!(state.get(density::STATE_KEY).is_none());
}

#[gpui_kit::test]
fn settings_save_leaves_density_persistence_enabled_after_close(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.save(window, cx).unwrap();
            settings.restore_preview(window, cx).unwrap();
            assert!(!cx.global::<DensityPreview>().0);
            cx.global_mut::<Density>().set("--row-h", 41.0);
            panel::save_density(cx);
            prefs::save(cx).unwrap();
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get(density::STATE_KEY), Some(r#"{"--row-h":"41px"}"#));
}

#[gpui_kit::test]
fn settings_save_persists_included_board_choices(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings_with_config(
        root.path(),
        r#"{"boards":{"github":true,"untouched":false,"unknown":true}}"#,
        cx,
    );
    settings
        .update(cx, |settings, window, cx| {
            settings.form.set_board_enabled("github", false);
            settings.form.set_board_enabled("github", true);
            assert!(
                settings.save(window, cx).unwrap(),
                "board changes need restart"
            );
        })
        .unwrap();

    let config = subspace_core::config::load(&root.path().join("config.json")).0;
    assert!(config.board_enabled("github"));
    assert!(config.board_enabled("github-copilot-agents"));
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(saved["boards"]["untouched"], false);
    assert_eq!(saved["boards"]["unknown"], true);
    assert!(saved["boards"].get("github").is_none());
}

#[gpui_kit::test]
fn disabling_pi_updates_only_the_draft_engine_and_cancel_writes_nothing(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.chat_engine.update(cx, |engine, cx| {
                engine.set_selected_value(&"Pi (Experimental)".to_owned(), window, cx);
            });
            settings.toggle_feature(&PI_CHAT, true, window, cx);
            settings.toggle_feature(&PI_CHAT, false, window, cx);
            assert_eq!(
                settings
                    .chat_engine
                    .read(cx)
                    .selected_value()
                    .map(String::as_str),
                Some("Off")
            );
            assert_eq!(
                settings.notice.as_deref(),
                Some(super::CHAT_OFF_AFTER_RESTART)
            );
            assert!(!settings.form.feature_enabled(&PI_CHAT));
            settings.restore_preview(window, cx).unwrap();
        })
        .unwrap();
    let config = subspace_core::config::load(&root.path().join("config.json")).0;
    assert_eq!(config.chat.engine, subspace_core::config::ChatEngine::Off);
    assert!(!config.features.contains_key(PI_CHAT.id));
}

#[gpui_kit::test]
fn disabling_selected_pi_saves_chat_off_and_the_shared_false_flag(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings_with_config(
        root.path(),
        r#"{"chat":{"engine":"pi"},"features":{"app.pi-chat":true}}"#,
        cx,
    );
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(
                settings
                    .chat_engine
                    .read(cx)
                    .selected_value()
                    .map(String::as_str),
                Some("Pi (Experimental)")
            );
            settings.toggle_feature(&PI_CHAT, false, window, cx);
            assert!(settings.save(window, cx).unwrap());
        })
        .unwrap();
    let config = subspace_core::config::load(&root.path().join("config.json")).0;
    assert_eq!(config.chat.engine, subspace_core::config::ChatEngine::Off);
    assert_eq!(config.features.get(PI_CHAT.id), Some(&false));
}

#[gpui_kit::test]
fn pi_engine_save_requires_the_shared_experiment_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.chat_engine.update(cx, |engine, cx| {
                engine.set_selected_value(&"Pi (Experimental)".to_owned(), window, cx);
            });
            let error = settings.save(window, cx).unwrap_err();
            assert!(error.contains("Enable it in Experiments"));
            settings.toggle_feature(&PI_CHAT, true, window, cx);
            assert!(settings.save(window, cx).unwrap());
        })
        .unwrap();
    let config = subspace_core::config::load(&root.path().join("config.json")).0;
    assert_eq!(config.chat.engine, subspace_core::config::ChatEngine::Pi);
    assert!(config.feature_enabled(&PI_CHAT));
}

#[gpui_kit::test]
fn saved_font_sizes_restore_independently_to_the_live_theme(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        prefs::set(
            cx,
            density::STATE_KEY,
            r#"{"--heading-font":"19px","--row-font":"16px","--mono-font":"17.5px"}"#,
        );
        theme::initialize(cx);
        assert_eq!(cx.global::<Density>().get("--heading-font"), 19.);
        assert_eq!(cx.global::<Density>().get("--row-font"), 16.);
        assert_eq!(cx.global::<Density>().get("--mono-font"), 17.5);
        assert_eq!(Theme::global(cx).font_size, px(14.) * (16. / 13.));
        assert_eq!(Theme::global(cx).mono_font_size, px(17.5));
    });
}

fn set_font_sizes(settings: WindowHandle<Settings>, values: [f32; 3], cx: &mut TestAppContext) {
    settings
        .update(cx, |settings, window, cx| {
            for (input, value) in settings.font_sizes.iter().zip(values) {
                type_font_size(input, &value.to_string(), window, cx);
            }
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn font_size_preview_and_theme_switch_do_not_save_and_cancel_restores_sizes(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    let state_file = root.path().join("state.json");
    let original = std::fs::read(&state_file).unwrap();
    set_font_sizes(settings, [20., 15.5, 17.], cx);
    settings
        .update(cx, |settings, window, cx| {
            settings.themes.update(cx, |themes, cx| {
                themes.set_selected_value(&"Solarized Dark".to_owned(), window, cx);
            });
            settings.preview(window, cx);
            let tokens = Tokens::current(cx);
            assert_eq!(tokens.heading_size(15.), px(20.));
            assert_eq!(tokens.row_font, px(15.5));
            assert_eq!(tokens.mono_size(13.), px(17.));
            assert_eq!(Theme::global(cx).mono_font_size, px(17.));
            panel::save_density(cx);
            prefs::save(cx).unwrap();
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(cx.global::<Density>().get("--heading-font"), 15.);
            assert_eq!(cx.global::<Density>().get("--row-font"), 13.);
            assert_eq!(cx.global::<Density>().get("--mono-font"), 13.);
            assert_eq!(Theme::global(cx).font_size, px(14.));
            assert_eq!(Theme::global(cx).mono_font_size, px(13.));
        })
        .unwrap();
    assert_eq!(std::fs::read(&state_file).unwrap(), original);
}

#[gpui_kit::test]
fn settings_save_and_reload_keep_sizes_and_unrelated_density(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    set_font_sizes(settings, [18., 14.5, 16.5], cx);
    settings
        .update(cx, |settings, window, cx| {
            cx.global_mut::<Density>().set("--row-h", 37.);
            assert!(
                !settings.save(window, cx).unwrap(),
                "font sizes do not need a restart"
            );
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(Tokens::current(cx).heading_font, px(18.));
            assert!(!cx.global::<DensityPreview>().0);
            prefs::init_file(Some(root.path().join("state.json")), cx);
            theme::initialize(cx);
            assert_eq!(cx.global::<Density>().get("--row-h"), 37.);
            assert_eq!(cx.global::<Density>().get("--heading-font"), 18.);
            assert_eq!(cx.global::<Density>().get("--row-font"), 14.5);
            assert_eq!(Theme::global(cx).font_size, px(14.) * (14.5 / 13.));
            assert_eq!(Theme::global(cx).mono_font_size, px(16.5));
        })
        .unwrap();
}
