use super::{Settings, form::SettingsPage, preview_tests::open_settings};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, Render, TestAppContext, Window, WindowHandle, px,
    size,
};
use subspace_dashboard::{
    icon_button::{IconButton, IconButtonSize, PREFERENCE_KEY},
    prefs,
};

struct Probe;

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("existing-icon")
            .icon(subspace_lucide::IconName::Search)
            .accessibility_label("Search")
    }
}

fn selected_size(settings: &Settings, cx: &App) -> Option<String> {
    settings
        .icon_buttons
        .choice
        .read(cx)
        .selected_value()
        .cloned()
}

fn choose(settings: &mut Settings, label: &str, window: &mut Window, cx: &mut Context<Settings>) {
    settings.icon_buttons.choice.update(cx, |choice, cx| {
        choice.set_selected_value(&label.to_owned(), window, cx);
    });
}

fn probe_height(handle: WindowHandle<Probe>, cx: &mut TestAppContext) -> gpui_kit::Pixels {
    cx.update_window(*handle, |_, window, cx| {
        window.render_frame(cx);
        window.find("existing-icon").bounds().size.height
    })
    .unwrap()
}

#[gpui_kit::test]
fn appearance_has_a_small_default_and_keeps_the_existing_breadcrumb(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    cx.simulate_window_resize(settings.into(), size(px(1000.), px(1400.)));
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(IconButtonSize::current(cx), IconButtonSize::Small);
            assert_eq!(prefs::get(cx, PREFERENCE_KEY), None);
            assert_eq!(selected_size(settings, cx).as_deref(), Some("Small"));
            settings.page = SettingsPage::Appearance;
            cx.notify();
        })
        .unwrap();
    cx.update_window(*settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("settings-breadcrumb").visible());
        assert!(window.within("icon-button-size").find("input").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn saving_icon_size_persists_and_resizes_an_existing_view_without_restart(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    let probe = cx.open_window(size(px(400.), px(300.)), |_, _| Probe);
    assert_eq!(probe_height(probe, cx), px(24.));
    settings
        .update(cx, |settings, window, cx| {
            choose(settings, "Large", window, cx);
            assert!(settings.has_unsaved_changes(cx));
            assert_eq!(IconButtonSize::current(cx), IconButtonSize::Small);
            assert!(
                !settings.save(window, cx).unwrap(),
                "appearance needs no restart"
            );
            assert!(!settings.has_unsaved_changes(cx));
        })
        .unwrap();
    assert_eq!(probe_height(probe, cx), px(48.));
    cx.update(|cx| {
        prefs::init_file(Some(root.path().join("state.json")), cx);
        assert_eq!(IconButtonSize::current(cx), IconButtonSize::Large);
        assert_eq!(prefs::get(cx, PREFERENCE_KEY).as_deref(), Some("large"));
    });
    let reopened = open_settings(root.path(), cx);
    reopened
        .update(cx, |settings, _, cx| {
            assert_eq!(selected_size(settings, cx).as_deref(), Some("Large"));
        })
        .unwrap();
}

#[gpui_kit::test]
fn cancelling_an_icon_size_draft_does_not_change_live_or_saved_preferences(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            choose(settings, "Extra large", window, cx);
            settings.restore_preview(window, cx).unwrap();
            assert_eq!(IconButtonSize::current(cx), IconButtonSize::Small);
            assert_eq!(prefs::get(cx, PREFERENCE_KEY), None);
        })
        .unwrap();
    let saved = subspace_core::state::State::load(&root.path().join("state.json"));
    assert_eq!(saved.get(PREFERENCE_KEY), None);
}

#[gpui_kit::test]
fn failed_icon_preference_write_does_not_publish_or_keep_a_pending_value(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    cx.update(|cx| prefs::init_file(Some(root.path().to_owned()), cx));
    settings
        .update(cx, |settings, window, cx| {
            choose(settings, "Medium", window, cx);
            assert!(settings.icon_buttons.save(cx).is_err());
            assert_eq!(IconButtonSize::current(cx), IconButtonSize::Small);
            assert_eq!(prefs::get(cx, PREFERENCE_KEY), None);
        })
        .unwrap();
}
