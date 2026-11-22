//! Space Lab is an opt-in alpha for native runtime-backed dashboards.
//!
//! It does not make production security or runtime-management claims. Runtime
//! work is explicit and uses the MXC worker boundary.

pub mod contracts;
pub mod custom_recipes;
pub mod dotenv;
pub mod editors;
pub mod environments;
mod help_images;
pub mod lab_tools;
pub mod net_probe;
pub mod private_settings;
pub mod project;
mod project_browser;
pub mod project_export;
pub mod project_runtime;
pub mod project_service;
pub mod project_settings;
pub mod project_tools;
pub mod project_view;
mod shell;
pub mod studio;
mod workbench;

pub use workbench::Workbench;

use anyhow::{Result, bail};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, Global,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use std::path::PathBuf;
use std::sync::Arc;
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{Dashboard, DashboardEvent, ToolEnvironment};

const PROTOTYPE_VIEWS: subspace_framework::core::features::FeatureSpec =
    subspace_framework::core::features::FeatureSpec {
        id: "board.space-lab.prototype-views",
        title: "Show prototype views",
        description: "Show the retained Canvas and Dashboard prototypes. Save and restart to apply.",
        stage: subspace_framework::core::features::FeatureStage::Experimental,
        default_enabled: false,
        toggleable: true,
    };

/// The dashboard is either fully rooted in product application data or an
/// inert error view. The unavailable variant owns no Studio or environment
/// manager and therefore cannot start watchers or invoke persistence actions.
pub enum SpaceLab {
    Ready(Entity<Workbench>),
    Unavailable {
        focus: FocusHandle,
        message: SharedString,
    },
}

impl EventEmitter<DashboardEvent> for SpaceLab {}

impl Focusable for SpaceLab {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Ready(board) => board.read(cx).focus_handle(cx),
            Self::Unavailable { focus, .. } => focus.clone(),
        }
    }
}

impl Render for SpaceLab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Ready(board) => board.clone().into_any_element(),
            Self::Unavailable { message, .. } => v_flex()
                .size_full()
                .justify_center()
                .items_center()
                .gap_2()
                .child(h_flex().child("Space Lab unavailable"))
                .child(div().child(message.clone()))
                .into_any_element(),
        }
    }
}

impl Dashboard for SpaceLab {
    const ID: &'static str = "space-lab";
    const TITLE: &'static str = "Space Lab";
    const ICON: Option<&'static str> = Some("icons/flask-16.svg");
    const ARTWORK: subspace_framework::dashboard::BoardArtwork =
        subspace_framework::dashboard::BoardArtwork::new(&[(
            "icons/flask-16.svg",
            include_bytes!("../assets/flask-16.svg"),
        )])
        .with_order(8);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");
    const HELP_IMAGES: &'static [subspace_framework::dashboard::HelpImage] = help_images::IMAGES;
    /// Disabling the board keeps its project workers running until quit.
    const RETAIN_WHEN_DISABLED: bool = true;

    fn settings() -> subspace_framework::dashboard::settings::BoardSettings {
        subspace_framework::dashboard::settings::BoardSettings {
            description: "An experimental, opt-in feature. Use it at your own risk. It builds native dashboards from JSON definitions and Python, Node or Bun source, and you can create, run and inspect projects with charts and tables. Available on macOS ARM64 only, and off by default.",
            features: &[PROTOTYPE_VIEWS],
            ..Default::default()
        }
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        install_project_lifecycle(cx);
        match workbench_root() {
            Ok(root)
                if mxc_probe::supports_platform(std::env::consts::OS, std::env::consts::ARCH) =>
            {
                Self::Ready(cx.new(|cx| Workbench::new_at(root, window, cx)))
            }
            Ok(_) => Self::Unavailable {
                focus: cx.focus_handle(),
                message: "Space Lab requires macOS ARM64; no worker fallback is available.".into(),
            },
            Err(error) => Self::Unavailable {
                focus: cx.focus_handle(),
                message: format!("{error:#}").into(),
            },
        }
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Self::Ready(board) = self {
            board.update(cx, |board, cx| board.activate(window, cx));
        }
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Self::Ready(board) = self {
            board.update(cx, |board, cx| board.deactivate(window, cx));
        }
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Self::Ready(board) = self {
            board.update(cx, |board, cx| board.reload(window, cx));
        }
    }

    fn status(&self, cx: &App) -> Option<SharedString> {
        match self {
            Self::Ready(board) => board.read(cx).status(cx),
            Self::Unavailable { message, .. } => Some(message.clone()),
        }
    }

    fn busy(&self, cx: &App) -> bool {
        matches!(self, Self::Ready(board) if board.read(cx).busy(cx))
    }

    fn docks(&self, cx: &App) -> SpaceDocks {
        match self {
            Self::Ready(board) => board.read(cx).space_docks(),
            Self::Unavailable { .. } => SpaceDocks::NONE,
        }
    }

    fn toggle_dock(&mut self, dock: SpaceDock, _window: &mut Window, cx: &mut Context<Self>) {
        if let Self::Ready(board) = self {
            board.update(cx, |board, cx| board.toggle_space_dock(dock, cx));
        }
    }

    fn agent_context(&self, cx: &App) -> Option<subspace_framework::dashboard::AgentContext> {
        match self {
            Self::Ready(board) => board.read(cx).agent_context(cx),
            Self::Unavailable { .. } => None,
        }
    }

    fn agent_tools(
        environment: &ToolEnvironment,
    ) -> Vec<Arc<dyn subspace_framework::dashboard::AgentTool>> {
        workbench_root()
            .map(|root| crate::lab_tools::tools_for_root(environment, root))
            .unwrap_or_default()
    }
}

struct ProjectLifecycle {
    _quit: Subscription,
}

impl Global for ProjectLifecycle {}

fn install_project_lifecycle(cx: &mut App) {
    if cx.has_global::<ProjectLifecycle>() {
        return;
    }
    let quit = cx.on_app_quit(|cx| {
        cx.background_spawn(async {
            if let Err(error) = crate::project_service::shutdown_all() {
                eprintln!("Space Lab worker shutdown failed: {error}");
            }
        })
    });
    cx.set_global(ProjectLifecycle { _quit: quit });
}

/// Returns the product-owned Space Lab root beside Subspace's state file.
///
/// This function never falls back to the current directory or a temporary
/// directory. The development override is retained for isolated smoke tests.
pub fn workbench_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("SUBSPACE_WORKBENCH_ROOT") {
        let root = PathBuf::from(root);
        return validate_root(root, "SUBSPACE_WORKBENCH_ROOT");
    }
    root_from_state_path(subspace_framework::core::state::path().as_deref())
}

fn root_from_state_path(state: Option<&std::path::Path>) -> Result<PathBuf> {
    let Some(state) = state else {
        bail!("Subspace application data directory is unavailable; Space Lab is disabled")
    };
    let Some(parent) = state.parent() else {
        bail!("Subspace state path has no parent; Space Lab is disabled")
    };
    validate_root(parent.join("boards").join("space-lab"), "Subspace state")
}

fn validate_root(root: PathBuf, source: &str) -> Result<PathBuf> {
    if root.is_absolute() {
        Ok(root)
    } else {
        bail!("{source} resolved to a relative Space Lab root; Space Lab is disabled")
    }
}

#[cfg(test)]
mod tests {
    use super::root_from_state_path;
    use crate::contracts::{Dataset, Parameters};
    use crate::environments::EnvironmentManager;
    use crate::studio::Studio;
    use std::path::PathBuf;

    #[test]
    fn missing_application_data_disables_board_without_fallback() {
        assert!(root_from_state_path(None).is_err());
    }

    #[test]
    fn relative_override_is_rejected_without_touching_filesystem() {
        assert!(super::validate_root(PathBuf::from("space-lab"), "test").is_err());
    }

    #[test]
    fn root_is_a_board_sibling_of_state_file() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let state = directory.path().join("subspace").join("state.json");
        let root = root_from_state_path(Some(&state)).expect("injected state path should resolve");
        assert_eq!(
            root,
            directory
                .path()
                .join("subspace")
                .join("boards")
                .join("space-lab")
        );
        assert!(!root.exists());
    }

    #[test]
    fn catalog_and_draft_are_read_only_until_explicit_actions() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let root = directory.path().join("space-lab");
        let manager = EnvironmentManager::new(root.join("environments"));
        let catalog = manager.catalog().expect("catalog defaults are readable");
        assert!(!catalog.is_empty());
        assert!(!root.exists());

        let studio = Studio::new(root.clone());
        let draft = studio.new_draft("market").expect("market draft");
        assert_eq!(draft.parameters.start, Parameters::default().start);
        assert!(!root.exists());
    }

    #[test]
    fn snapshot_validation_rejects_changed_parameters() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let studio = Studio::new(directory.path().join("space-lab"));
        let draft = studio.new_draft("market").expect("market draft");
        let dataset = Dataset {
            columns: Vec::new(),
            rows: Vec::new(),
            labels: Vec::new(),
            series: Vec::new(),
            summary: String::new(),
            provenance: format!(
                "params={}:{}:{}:{}:{}",
                draft.parameters.start,
                draft.parameters.end,
                draft.parameters.currency,
                draft.parameters.service,
                draft.parameters.group_by
            ),
        };
        assert!(crate::studio::snapshot_matches(&draft, &dataset));
        let mut changed = draft;
        changed.parameters.currency = "EUR".into();
        assert!(!crate::studio::snapshot_matches(&changed, &dataset));
    }
}
