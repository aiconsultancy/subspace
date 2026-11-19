//! Offline product shell used by headless tests and native screenshot builds.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, div,
};
use subspace_agent_tools::Catalog;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner};
use subspace_dashboard::{CurrentProduct, Dashboard, DashboardEvent, Services, prefs};
use subspace_product::ProductContext;

use crate::app::SubspaceApp;
use crate::{product::ProductFiles, registry};

struct Offline(PathBuf);

impl Environment for Offline {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| self.0.to_string_lossy().into_owned())
    }
}

impl Runner for Offline {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("External processes are disabled in this offline fixture.".into())
    }
}

pub fn install_offline_fixture_services(root: &Path, cx: &mut App) {
    let config = Arc::new(Config::default());
    let credentials = Arc::new(Credentials::new(
        &config,
        Offline(root.join("gh-fixture")),
        Offline(root.to_owned()),
    ));
    cx.set_global(Services {
        config,
        credentials,
        tools: Catalog::default(),
        search: None,
        config_file: root.join("config.json"),
    });
}

/// Render the actual shell with one inert board and no installed user state.
pub fn offline_shell_fixture(
    context: ProductContext,
    root: &Path,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SubspaceApp> {
    cx.set_global(CurrentProduct(Arc::new(context)));
    cx.set_global(ProductFiles {
        config: root.join("config.json"),
        state: root.join("state.json"),
    });
    prefs::init_file(None, cx);
    prefs::set(cx, subspace_chat::OPEN_KEY, "0");
    prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
    install_offline_fixture_services(root, cx);
    #[cfg(feature = "report-issue")]
    crate::app::report_issue::install_fixture_target(cx);
    crate::theme::initialize(cx);
    crate::app::bind_keys(cx);
    cx.set_global(registry::BoardFactories(vec![
        registry::BoardRegistration::new::<FixtureBoard>(),
    ]));
    cx.new(|cx| SubspaceApp::new(Vec::new(), window, cx))
}

struct FixtureBoard(FocusHandle);

impl EventEmitter<DashboardEvent> for FixtureBoard {}

impl Focusable for FixtureBoard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.0.clone()
    }
}

impl Render for FixtureBoard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.0)
            .size_full()
            .p_4()
            .child("Offline fixture data. No services are connected.")
    }
}

impl Dashboard for FixtureBoard {
    const ID: &'static str = "fixture";
    const TITLE: &'static str = "Fixture board";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self(cx.focus_handle())
    }

    fn status(&self, _: &App) -> Option<SharedString> {
        Some("Offline fixture".into())
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}
