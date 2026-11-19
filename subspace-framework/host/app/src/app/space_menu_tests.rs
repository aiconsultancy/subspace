//! Closing a Space from its switcher tab's right-click menu.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, Render, Styled as _, TestAppContext, Window,
    div, px,
};
use serde_json::Value;
use subspace_chat::OPEN_KEY;
use subspace_core::config::Config;
use subspace_dashboard::{CurrentProduct, Dashboard, DashboardEvent, Services, prefs};
use subspace_product::{BoardDescriptor, ValidatedProduct, validate_product};

use super::navigation::NAVIGATION_KEY;
use super::space_menu::{CLOSE_SPACE, LAST_SPACE, SPACE_SETTINGS};
use super::{ACTIVE_KEY, SubspaceApp};
use crate::settings::Settings;
use crate::{BoardRegistration, registry};

const IDS: [&str; 3] = ["alpha", "beta", "gamma"];
const TITLES: [&str; 3] = ["Alpha", "Beta", "Gamma"];

/// One of three plain Spaces that can take focus.
struct Plain<const N: usize> {
    focus: FocusHandle,
}
impl<const N: usize> EventEmitter<DashboardEvent> for Plain<N> {}
impl<const N: usize> Focusable for Plain<N> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl<const N: usize> Render for Plain<N> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).size_full()
    }
}
impl<const N: usize> Dashboard for Plain<N> {
    const ID: &'static str = IDS[N];
    const TITLE: &'static str = TITLES[N];
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

/// The product: Gamma is off by default, as some real Spaces are.
fn product() -> ValidatedProduct {
    let mut descriptor_set = subspace_product::fixture::descriptor();
    descriptor_set.boards = IDS
        .iter()
        .map(|id| BoardDescriptor {
            id: (*id).into(),
            aliases: Vec::new(),
            default_enabled: *id != "gamma",
        })
        .collect();
    validate_product(&descriptor_set).unwrap()
}

/// The shell with Alpha, Beta and Gamma (turned on in the config file), in
/// a rooted window so context menus can open.
fn open(root: &std::path::Path, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<SubspaceApp>) {
    let path = root.join("config.json");
    std::fs::write(&path, r#"{"boards": {"gamma": true}}"#).unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        crate::theme::preview_tests::nominal_controls(cx);
        prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
        prefs::set(cx, NAVIGATION_KEY, "horizontal");
        prefs::set(cx, OPEN_KEY, "0");
        let mut context = (*cx.global::<CurrentProduct>().0).clone();
        context.product = product();
        cx.set_global(CurrentProduct(Arc::new(context)));
        let boards = vec![
            BoardRegistration::new::<Plain<0>>(),
            BoardRegistration::new::<Plain<1>>(),
            BoardRegistration::new::<Plain<2>>(),
        ];
        registry::retain_provider(boards.clone(), None, cx);
        cx.set_global(registry::IncludedBoards(boards.clone()));
        cx.set_global(registry::BoardFactories(boards));
        let config = subspace_core::config::load(&path).0;
        cx.set_global(Services {
            config: Arc::new(config.clone()),
            credentials: Arc::new(subspace_core::credentials::Credentials::from_system(
                &config,
            )),
            tools: subspace_agent_tools::Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file: path,
        });
    });
    let handle = cx.open_window(gpui_kit::size(px(1400.), px(600.)), |window, cx| {
        SubspaceApp::new(Vec::new(), window, cx)
    });
    let app = handle.update(cx, |_, _, cx| cx.entity()).unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(app.clone(), window, cx)
        });
    })
    .unwrap();
    settle(handle.into(), cx);
    (handle.into(), app)
}

fn settle(window: AnyWindowHandle, cx: &mut TestAppContext) {
    for _ in 0..4 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

fn ids(app: &Entity<SubspaceApp>, cx: &mut TestAppContext) -> Vec<&'static str> {
    app.read_with(cx, |app, _| {
        app.dashboards.iter().map(|board| board.id).collect()
    })
}

fn active(app: &Entity<SubspaceApp>, cx: &mut TestAppContext) -> &'static str {
    app.read_with(cx, |app, _| app.dashboards[app.active].id)
}

fn show(window: AnyWindowHandle, app: &Entity<SubspaceApp>, id: &str, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            let index = app
                .dashboards
                .iter()
                .position(|board| board.id == id)
                .unwrap();
            app.switch_to(index, window, cx);
        });
    })
    .unwrap();
    settle(window, cx);
}

/// Right-clicks Space `id`'s tab and returns the menu item labelled `label`.
fn menu_item(
    window: AnyWindowHandle,
    id: &str,
    label: &str,
    cx: &mut TestAppContext,
) -> gpui_kit::test::ElementSnapshot {
    cx.update_window(window, |_, window, cx| {
        window.right_click(ElementId::Name(format!("space-tab-{id}").into()), cx);
    })
    .unwrap();
    settle(window, cx);
    cx.update_window(window, |_, window, _| {
        window.within("popup-menu").find_by_label(label)
    })
    .unwrap()
}

/// Right-clicks Space `id`'s tab and chooses `label` from its menu.
fn choose(window: AnyWindowHandle, id: &str, label: &str, cx: &mut TestAppContext) {
    let item = menu_item(window, id, label, cx);
    let target = item.path().last().cloned().unwrap();
    cx.update_window(window, |_, window, cx| {
        window.within("popup-menu").click(target, cx);
    })
    .unwrap();
    settle(window, cx);
}

fn saved_boards(root: &std::path::Path) -> Option<Value> {
    let text = std::fs::read_to_string(root.join("config.json")).unwrap();
    serde_json::from_str::<Value>(&text)
        .unwrap()
        .get("boards")
        .cloned()
}

fn running_config(cx: &mut TestAppContext) -> Config {
    cx.read(|cx| (*cx.global::<Services>().config).clone())
}

#[gpui_kit::test]
fn closing_a_background_space_removes_its_tab_and_keeps_the_current_one(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    assert_eq!(ids(&app, cx), IDS);
    assert_eq!(active(&app, cx), "alpha");
    choose(window, "beta", CLOSE_SPACE, cx);
    assert_eq!(ids(&app, cx), ["alpha", "gamma"]);
    assert_eq!(active(&app, cx), "alpha");
    assert!(!running_config(cx).board_enabled("beta"));
    assert_eq!(
        saved_boards(root.path()),
        Some(serde_json::json!({"beta": false, "gamma": true}))
    );
    app.read_with(cx, |app, _| {
        let toast = &app.toasts.last().unwrap().toast.text;
        assert_eq!(toast, "Closed Beta. Turn it on in Settings to reopen it.");
    });
}

#[gpui_kit::test]
fn closing_the_current_space_moves_to_the_previous_tab_and_focuses_it(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    show(window, &app, "gamma", cx);
    choose(window, "gamma", CLOSE_SPACE, cx);
    assert_eq!(ids(&app, cx), ["alpha", "beta"]);
    assert_eq!(active(&app, cx), "beta");
    cx.update_window(window, |_, window, cx| {
        let app = app.read(cx);
        assert!(
            app.dashboards[app.active]
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(prefs::get(cx, ACTIVE_KEY).as_deref(), Some("beta"));
    })
    .unwrap();

    // The first tab has no previous tab, so the next one takes over.
    show(window, &app, "alpha", cx);
    choose(window, "alpha", CLOSE_SPACE, cx);
    assert_eq!(ids(&app, cx), ["beta"]);
    assert_eq!(active(&app, cx), "beta");
    cx.update_window(window, |_, window, cx| {
        let app = app.read(cx);
        assert!(
            app.dashboards[app.active]
                .focus_handle(cx)
                .is_focused(window)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_last_space_cannot_be_closed(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    choose(window, "beta", CLOSE_SPACE, cx);
    choose(window, "gamma", CLOSE_SPACE, cx);
    assert_eq!(ids(&app, cx), ["alpha"]);
    let saved = saved_boards(root.path());

    menu_item(window, "alpha", CLOSE_SPACE, cx);
    cx.update_window(window, |_, window, _| {
        let reason = window.within("popup-menu").try_find_by_label(LAST_SPACE);
        assert!(reason.is_some(), "the menu says why");
    })
    .unwrap();
    // The disabled item does nothing, and neither does a direct call.
    choose(window, "alpha", CLOSE_SPACE, cx);
    assert!(app.read_with(cx, |app, _| !app.can_close_space()));
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.close_space("alpha", window, cx));
    })
    .unwrap();
    settle(window, cx);
    assert_eq!(ids(&app, cx), ["alpha"]);
    assert_eq!(saved_boards(root.path()), saved);
}

/// Turns Space `id` on or off with its Settings switch and Save.
fn settings_save(
    window: AnyWindowHandle,
    app: &Entity<SubspaceApp>,
    id: &str,
    enabled: bool,
    cx: &mut TestAppContext,
) {
    let restart = cx
        .update_window(window, |_, window, cx| {
            let settings = Settings::open(window, cx).unwrap();
            app.update(cx, |app, _| app.settings = Some(settings.clone()));
            settings.update(cx, |settings, cx| {
                settings.draft_board_enabled(id, enabled);
                settings.save(window, cx)
            })
        })
        .unwrap();
    assert_eq!(restart, Ok(false));
    settle(window, cx);
    app.update(cx, |app, _| app.settings = None);
}

#[gpui_kit::test]
fn closing_from_the_tab_saves_what_the_settings_switch_saves(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    for id in ["gamma", "beta"] {
        choose(window, id, CLOSE_SPACE, cx);
        let from_tab = (saved_boards(root.path()), running_config(cx).boards);
        settings_save(window, &app, id, true, cx);
        assert!(ids(&app, cx).contains(&id), "Settings reopens {id}");
        settings_save(window, &app, id, false, cx);
        let from_settings = (saved_boards(root.path()), running_config(cx).boards);
        assert_eq!(from_tab, from_settings, "{id}");
        assert!(!ids(&app, cx).contains(&id));
        settings_save(window, &app, id, true, cx);
    }
    // Gamma is off by default: closing it drops its explicit choice.
    choose(window, "gamma", CLOSE_SPACE, cx);
    assert_eq!(saved_boards(root.path()), None);
}

#[gpui_kit::test]
fn space_settings_opens_that_spaces_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = open(root.path(), cx);
    choose(window, "beta", SPACE_SETTINGS, cx);
    assert_eq!(active(&app, cx), "alpha");
    let settings = app.read_with(cx, |app, _| app.settings.clone()).unwrap();
    assert!(settings.read_with(cx, |settings, _| settings.shows_board_page("beta")));
}
