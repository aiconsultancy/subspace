//! Built-in boards that Settings enables and disables while the app runs.

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, Global, IntoElement, Render, SharedString,
    TestAppContext, Window, WindowHandle, div,
};
use subspace_core::config::Config;
use subspace_dashboard::{CurrentProduct, Dashboard, DashboardEvent, Services, prefs};
use subspace_product::{BoardDescriptor, ValidatedProduct, validate_product};

use super::super::{ACTIVE_KEY, SubspaceApp};
use super::{Removal, removal};
use crate::settings::{CONFIRM_QUIT_KEY, Settings};
use crate::{BoardRegistration, registry};

const RUNNING_SHELL: &str = "A shell is running";

#[derive(Default)]
struct Background {
    started: usize,
    stopped: usize,
}
impl Global for Background {}

/// A board that hot loads: disabling it drops its view and stops its work.
struct Hot {
    focus: FocusHandle,
}
impl EventEmitter<DashboardEvent> for Hot {}
impl Focusable for Hot {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Hot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl Dashboard for Hot {
    const ID: &'static str = "hot";
    const TITLE: &'static str = "Hot";
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn start_background(cx: &mut App) {
        cx.default_global::<Background>().started += 1;
    }
    fn stop_background(cx: &mut App) {
        cx.default_global::<Background>().stopped += 1;
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

/// A board that owns processes: disabling it only hides its view.
struct Kept {
    focus: FocusHandle,
}
impl EventEmitter<DashboardEvent> for Kept {}
impl Focusable for Kept {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Kept {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl Dashboard for Kept {
    const ID: &'static str = "kept";
    const TITLE: &'static str = "Kept";
    const RETAIN_WHEN_DISABLED: bool = true;
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn quit_warnings(&self, _: &App) -> Vec<SharedString> {
        vec![RUNNING_SHELL.into()]
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

fn descriptor(id: &str) -> BoardDescriptor {
    BoardDescriptor {
        id: id.into(),
        aliases: Vec::new(),
        former_ids: Vec::new(),
        default_enabled: true,
    }
}

fn product() -> ValidatedProduct {
    let mut descriptor_set = subspace_product::fixture::descriptor();
    descriptor_set.boards = vec![descriptor("hot"), descriptor("kept")];
    validate_product(&descriptor_set).unwrap()
}

fn open(root: &std::path::Path, cx: &mut TestAppContext) -> WindowHandle<SubspaceApp> {
    let path = root.join("config.json");
    std::fs::write(&path, "{}").unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        let mut context = (*cx.global::<CurrentProduct>().0).clone();
        context.product = product();
        cx.set_global(CurrentProduct(Arc::new(context)));
        let boards = vec![
            BoardRegistration::new::<Hot>(),
            BoardRegistration::new::<Kept>(),
        ];
        registry::retain_provider(boards.clone(), None, cx);
        cx.set_global(registry::IncludedBoards(boards.clone()));
        cx.set_global(registry::BoardFactories(boards));
        let config = Config::default();
        cx.set_global(Services {
            config: Arc::new(config.clone()),
            credentials: Arc::new(subspace_core::credentials::Credentials::from_system(
                &config,
            )),
            tools: subspace_agent_tools::Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file: path,
        });
        registry::sync_backgrounds(cx);
    });
    cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx))
}

fn apply(window: WindowHandle<SubspaceApp>, disabled: &[&str], cx: &mut TestAppContext) {
    let boards: BTreeMap<String, bool> = disabled
        .iter()
        .map(|id| ((*id).to_owned(), false))
        .collect();
    window
        .update(cx, |_, window, cx| {
            let config = Config {
                boards,
                ..Default::default()
            };
            registry::apply_board_selection(config, window, cx).unwrap();
        })
        .unwrap();
    cx.run_until_parked();
}

fn ids(app: &SubspaceApp) -> Vec<&'static str> {
    app.dashboards.iter().map(|board| board.id).collect()
}

fn background(cx: &App) -> (usize, usize) {
    let background = cx.global::<Background>();
    (background.started, background.stopped)
}

#[gpui_kit::test]
fn disabling_the_active_board_closes_its_tab_and_enabling_it_creates_a_new_view(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let window = open(directory.path(), cx);
    let first = window
        .update(cx, |app, _, cx| {
            assert_eq!(ids(app), ["hot", "kept"]);
            assert_eq!(app.dashboards[app.active].id, "hot");
            assert_eq!(background(cx), (1, 0));
            app.dashboards[0].focus_handle(cx)
        })
        .unwrap();
    apply(window, &["hot"], cx);
    window
        .update(cx, |app, _, cx| {
            assert_eq!(ids(app), ["kept"]);
            assert_eq!(app.dashboards[app.active].id, "kept");
            assert!(app.parked.is_empty());
            assert_eq!(prefs::get(cx, ACTIVE_KEY).as_deref(), Some("kept"));
            assert_eq!(background(cx), (1, 1));
        })
        .unwrap();
    apply(window, &[], cx);
    window
        .update(cx, |app, window, cx| {
            assert_eq!(ids(app), ["hot", "kept"]);
            assert_ne!(app.dashboards[0].focus_handle(cx), first);
            assert_eq!(app.dashboards[app.active].id, "kept");
            assert_eq!(background(cx), (2, 1));
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_retained_board_hides_while_disabled_and_shows_the_same_view_again(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let window = open(directory.path(), cx);
    let kept = window
        .update(cx, |app, window, cx| {
            app.switch_to(1, window, cx);
            app.dashboards[1].focus_handle(cx)
        })
        .unwrap();
    apply(window, &["kept"], cx);
    window
        .update(cx, |app, _, cx| {
            assert_eq!(ids(app), ["hot"]);
            assert_eq!(app.dashboards[app.active].id, "hot");
            assert_eq!(app.parked.len(), 1);
            assert_eq!(app.parked[0].handle.focus_handle(cx), kept);
            prefs::set(cx, CONFIRM_QUIT_KEY, "true");
            app.request_quit(cx);
            let warnings = app.quit_warnings.take().unwrap_or_default();
            assert!(warnings.iter().any(|warning| warning == RUNNING_SHELL));
        })
        .unwrap();
    apply(window, &[], cx);
    window
        .update(cx, |app, window, cx| {
            assert_eq!(ids(app), ["hot", "kept"]);
            assert_eq!(app.dashboards[1].focus_handle(cx), kept);
            assert!(app.parked.is_empty());
            window.remove_window();
        })
        .unwrap();
}

#[gpui_kit::test]
fn settings_save_applies_board_choices_without_a_restart(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let window = open(directory.path(), cx);
    for (enabled, expected) in [(false, vec!["kept"]), (true, vec!["hot", "kept"])] {
        let restart = window
            .update(cx, |app, window, cx| {
                let settings = Settings::open(window, cx).unwrap();
                app.settings = Some(settings.clone());
                settings.update(cx, |settings, cx| {
                    settings.draft_board_enabled("hot", enabled);
                    settings.save(window, cx)
                })
            })
            .unwrap();
        assert_eq!(restart, Ok(false));
        cx.run_until_parked();
        window
            .update(cx, |app, _, cx| {
                assert_eq!(ids(app), expected);
                let config = &cx.global::<Services>().config;
                assert_eq!(config.board_enabled("hot"), enabled);
                app.settings = None;
            })
            .unwrap();
    }
    let saved = subspace_core::config::load(&directory.path().join("config.json")).0;
    assert!(saved.board_enabled("hot"));
}

#[test]
fn only_a_disabled_retained_board_is_parked() {
    let mut kept = BoardRegistration::new::<Kept>();
    assert_eq!(removal(&kept, &["hot"]), Removal::Park);
    assert_eq!(removal(&kept, &["kept"]), Removal::Close);
    kept.retain_when_disabled = false;
    assert_eq!(removal(&kept, &["hot"]), Removal::Close);
    assert_eq!(
        removal(&BoardRegistration::new::<Hot>(), &[]),
        Removal::Close
    );
}
