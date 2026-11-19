//! The docked terminal's Layout choices and its font page, which appear only
//! for a product with a terminal and apply on Save.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, TestAppContext, Window,
    WindowHandle, div,
};
use serde_json::{Map, Value, json};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::dock_panel::{BottomSpan, DockPanelRegistration, Placement};
use subspace_dashboard::prefs;
use subspace_dashboard::settings::{BoardSettingsEditor, SettingsEditor};

use super::Settings;
use crate::terminal::{LABELS_KEY, PLACEMENT_KEY, SPAN_KEY};

/// A font editor that drafts one family name.
struct FontEditor {
    family: String,
}

impl Render for FontEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.family.clone())
    }
}

impl BoardSettingsEditor for FontEditor {
    fn snapshot(&self, _: &App) -> Value {
        json!(self.family)
    }

    fn values(&self, _: &App) -> Result<Map<String, Value>, String> {
        Ok(Map::from_iter([("fontFamily".into(), json!(self.family))]))
    }

    fn set_enabled(&mut self, _: bool, _: &mut Context<Self>) {}
}

fn factory(saved: Map<String, Value>, _: bool, _: &mut Window, cx: &mut App) -> SettingsEditor {
    let family = saved
        .get("fontFamily")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    SettingsEditor::new(cx.new(|_| FontEditor { family }))
}

fn registration() -> DockPanelRegistration {
    DockPanelRegistration {
        settings_editor: Some(factory),
        ..crate::app::terminal_fixture::registration()
    }
}

fn open(root: &std::path::Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        crate::terminal::install(Some(registration()), cx);
    });
    let path = root.join("config.json");
    std::fs::write(&path, r#"{"terminal":{"fontFamily":"Menlo","other":1}}"#).unwrap();
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

#[gpui_kit::test]
fn layout_shows_the_terminal_choices_and_saves_them_to_the_state_file(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-app:layout", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(settings.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("terminal-placement").visible());
        assert!(window.find("terminal-bottom-span").visible());
        assert_eq!(window.find("terminal-button-labels").checked(), Some(false));
        assert!(window.try_find(("toolbar-label", 1_usize)).is_some());
        window.click("terminal-button-labels", cx);
    })
    .unwrap();
    settings
        .update(cx, |settings, window, cx| {
            let terminal = settings.terminal.as_ref().unwrap();
            let (placement, span) = (terminal.placement.clone(), terminal.span.clone());
            placement.update(cx, |choice, cx| {
                choice.set_selected_value(&Placement::Left.label().to_owned(), window, cx);
            });
            span.update(cx, |choice, cx| {
                choice.set_selected_value(&BottomSpan::FullWidth.label().to_owned(), window, cx);
            });
            assert!(settings.has_unsaved_changes(cx));
            assert_eq!(prefs::get(cx, PLACEMENT_KEY), None, "applies on Save");
            assert!(!settings.save(window, cx).unwrap(), "no restart needed");
            assert!(!settings.has_unsaved_changes(cx));
            assert_eq!(prefs::get(cx, PLACEMENT_KEY).as_deref(), Some("left"));
            assert_eq!(prefs::get(cx, SPAN_KEY).as_deref(), Some("full-width"));
            assert_eq!(prefs::get(cx, LABELS_KEY).as_deref(), Some("1"));
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_terminal_page_saves_fonts_in_the_config_files_terminal_object(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-app:terminal", cx);
    })
    .unwrap();
    cx.run_until_parked();
    settings
        .update(cx, |settings, window, cx| {
            assert_eq!(settings.page, super::SettingsPage::Terminal);
            let editor = settings
                .terminal
                .as_ref()
                .and_then(|terminal| terminal.fonts.as_ref())
                .unwrap()
                .editor
                .view()
                .downcast::<FontEditor>()
                .ok()
                .unwrap();
            assert_eq!(editor.read(cx).family, "Menlo");
            editor.update(cx, |editor, cx| {
                editor.family = "Fixture Mono".into();
                cx.notify();
            });
            assert!(settings.has_unsaved_changes(cx));
            assert!(!settings.save(window, cx).unwrap());
        })
        .unwrap();
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(
        saved["terminal"],
        json!({"fontFamily": "Fixture Mono", "other": 1})
    );
}
