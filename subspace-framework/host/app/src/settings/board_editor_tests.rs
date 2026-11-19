use std::{path::Path, sync::Arc};

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Global, IntoElement, Render, Subscription,
    TestAppContext, Window, WindowHandle,
};
use serde_json::{Map, Value, json};
use subspace_core::{
    credentials::{Credentials, GitHubAccounts},
    settings::SettingsFile,
};
use subspace_dashboard::{
    Services,
    settings::{BoardSettingsEditor, SettingsEditor},
};

use super::Settings;

struct FixtureEditor {
    input: Entity<InputState>,
    enabled: bool,
    _subscription: Subscription,
}
struct Fixture(Entity<FixtureEditor>);
impl Global for Fixture {}

impl Render for FixtureEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Input::new(&self.input)
            .id("fixture-editor-input")
            .disabled(!self.enabled)
    }
}

impl BoardSettingsEditor for FixtureEditor {
    fn snapshot(&self, cx: &App) -> Value {
        json!(self.input.read(cx).value())
    }
    fn values(&self, cx: &App) -> Result<Map<String, Value>, String> {
        let value: Vec<String> = serde_json::from_str(&self.input.read(cx).value())
            .map_err(|_| "Use an array of strings.".to_owned())?;
        Ok(Map::from_iter([("ordered".into(), json!(value))]))
    }
    fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }
}

pub(crate) fn factory(
    saved: Map<String, Value>,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> SettingsEditor {
    let editor = cx.new(|cx| {
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(
                saved.get("ordered").unwrap_or(&json!([])).to_string(),
                window,
                cx,
            );
            input
        });
        let subscription = cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify());
        FixtureEditor {
            input,
            enabled,
            _subscription: subscription,
        }
    });
    cx.set_global(Fixture(editor.clone()));
    SettingsEditor::new(editor)
}

fn board_file(root: &Path) -> std::path::PathBuf {
    subspace_core::config::board_file(&root.join("config.json"), "fixture")
}

fn open(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    let path = root.join("config.json");
    std::fs::write(&path, r#"{"extension":true}"#).unwrap();
    let board = board_file(root);
    std::fs::create_dir_all(board.parent().unwrap()).unwrap();
    std::fs::write(&board, r#"{"keep":7}"#).unwrap();
    subspace_core::private_file::set_owner_only(&board).unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        let mut board = super::navigation_tests::registration();
        board.id = "fixture";
        board.settings_editor = Some(factory);
        cx.set_global(crate::registry::IncludedBoards(vec![board]));
        let config = subspace_core::config::load_with_boards(&path, ["fixture"]).0;
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&config)),
            config: Arc::new(config),
            tools: subspace_agent_tools::Catalog::new(vec![]).unwrap(),
            search: None,
            config_file: path.clone(),
        });
    });
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

fn edit(value: &str, window: &mut Window, cx: &mut App) {
    let input = cx.global::<Fixture>().0.read(cx).input.clone();
    input.update(cx, |input, cx| {
        input.set_value(value.to_owned(), window, cx)
    });
}

#[gpui_kit::test]
fn board_owned_editor_save_validation_dirty_state_and_cancel_share_outer_transaction(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    let path = root.path().join("config.json");
    let board = board_file(root.path());
    let original = std::fs::read(&path).unwrap();
    let original_board = std::fs::read(&board).unwrap();
    settings
        .update(cx, |settings, window, cx| {
            assert!(!settings.has_unsaved_changes(cx));
            edit("invalid", window, cx);
            assert!(settings.has_unsaved_changes(cx));
            assert!(settings.save(window, cx).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(std::fs::read(&board).unwrap(), original_board);
            edit(r#"["second", "edited first"]"#, window, cx);
            assert!(
                cx.global::<Services>().config.board_settings["fixture"]
                    .get("ordered")
                    .is_none()
            );
            assert!(!settings.save(window, cx).unwrap());
            assert!(!settings.has_unsaved_changes(cx));
            assert_eq!(
                cx.global::<Services>().config.board_settings["fixture"]["ordered"],
                json!(["second", "edited first"])
            );
            assert_eq!(
                cx.global::<Services>().config.board_settings["fixture"]["keep"],
                7
            );
            edit(r#"["cancelled"]"#, window, cx);
            assert!(settings.has_unsaved_changes(cx));
            settings.restore_preview(window, cx).unwrap();
        })
        .unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&board).unwrap()).unwrap();
    assert_eq!(
        saved,
        json!({"keep": 7, "ordered": ["second", "edited first"]})
    );
    let main: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(main["extension"], true);
    assert!(
        !main.to_string().contains("ordered"),
        "board settings stay out of the config file"
    );
}

#[test]
fn startup_loads_each_registered_boards_own_file_and_names_broken_ones() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let board = board_file(root.path());
    std::fs::create_dir_all(board.parent().unwrap()).unwrap();
    std::fs::write(&board, r#"{"ordered":["a"]}"#).unwrap();
    subspace_core::private_file::set_owner_only(&board).unwrap();
    let broken = subspace_core::config::board_file(&path, "future");
    std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
    std::fs::write(&broken, "{ nope").unwrap();
    let mut fixture = super::navigation_tests::registration();
    fixture.id = "fixture";
    let boards = [fixture, super::navigation_tests::registration()];

    let (environment, notices) =
        crate::tool_environment(&path, &boards, subspace_agent_tools::ToolHost::App, false);

    let settings = &environment.config.board_settings;
    assert_eq!(settings.len(), 1);
    assert_eq!(
        settings["fixture"],
        *json!({"ordered": ["a"]}).as_object().unwrap()
    );
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].starts_with(&broken.display().to_string()),
        "{notices:?}"
    );
}

#[gpui_kit::test]
fn an_unreadable_board_file_is_named_and_its_editor_left_out(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let board = board_file(root.path());
    std::fs::create_dir_all(board.parent().unwrap()).unwrap();
    std::fs::write(&board, "{ nope").unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        // The test text system lacks Cupertino's Menlo; a font warning would
        // take the place of the board error.
        subspace_dashboard::prefs::set(cx, crate::theme::THEME_PREF, crate::theme::THEME_NAME);
        crate::theme::initialize(cx);
        let mut registration = super::navigation_tests::registration();
        registration.id = "fixture";
        registration.settings_editor = Some(factory);
        cx.set_global(crate::registry::IncludedBoards(vec![registration]));
    });
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    settings
        .update(cx, |settings, _, _| {
            assert!(settings.board_editors.is_empty());
            let error = settings.error.clone().unwrap_or_default();
            assert!(error.starts_with(&board.display().to_string()), "{error}");
        })
        .unwrap();
}
