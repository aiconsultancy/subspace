use super::{Settings, form::SettingsPage, preview_tests::open_settings};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{App, AppContext as _, Context, TestAppContext, Window, WindowHandle, px, size};
use subspace_dashboard::avatar::{AGENT_ICONS_KEY, AgentIcons, DEFAULT_AGENT_ICONS};
use subspace_dashboard::prefs;

fn selected(settings: &Settings, cx: &App) -> Option<String> {
    settings
        .agent_icons
        .choice
        .read(cx)
        .selected_value()
        .cloned()
}

fn choose(settings: &mut Settings, label: &str, window: &mut Window, cx: &mut Context<Settings>) {
    settings.agent_icons.choice.update(cx, |choice, cx| {
        choice.set_selected_value(&label.to_owned(), window, cx);
    });
}

/// Settings opened on a state file of their own, kept while the test runs.
fn opened(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Settings>) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    (root, settings)
}

#[gpui_kit::test]
fn icons_default_to_small_with_nothing_stored_and_show_on_appearance(cx: &mut TestAppContext) {
    let (_root, settings) = opened(cx);
    cx.simulate_window_resize(settings.into(), size(px(1000.), px(1400.)));
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(AgentIcons::current(cx), DEFAULT_AGENT_ICONS);
            assert_eq!(prefs::get(cx, AGENT_ICONS_KEY), None);
            assert_eq!(selected(settings, cx).as_deref(), Some("Small"));
            settings.page = SettingsPage::Appearance;
            cx.notify();
        })
        .unwrap();
    cx.update_window(*settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("agent-icons").find("input").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn each_icons_choice_saves_applies_live_and_reloads(cx: &mut TestAppContext) {
    let (root, settings) = opened(cx);
    for icons in [AgentIcons::Off, AgentIcons::Large, AgentIcons::Small] {
        settings
            .update(cx, |settings, window, cx| {
                choose(settings, icons.label(), window, cx);
                assert!(!settings.save(window, cx).unwrap(), "no restart");
                assert_eq!(AgentIcons::current(cx), icons, "applied live");
            })
            .unwrap();
        cx.update(|cx| {
            prefs::init_file(Some(root.path().join("state.json")), cx);
            assert_eq!(AgentIcons::current(cx), icons);
            assert_eq!(
                prefs::get(cx, AGENT_ICONS_KEY).as_deref(),
                Some(icons.key())
            );
        });
    }
    let reopened = open_settings(root.path(), cx);
    reopened
        .update(cx, |settings, _, cx| {
            assert_eq!(selected(settings, cx).as_deref(), Some("Small"));
        })
        .unwrap();
}
