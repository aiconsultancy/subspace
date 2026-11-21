//! Throwaway native board that composes managed Python/Node/Bun environments,
//! native chart rendering and saved presets into one authoring workbench.
//! Not a Framework proposal.
//!
//! The struct is split into cohesive state groups ([`state::ViewState`],
//! [`state::DraftInputs`], [`state::JobState`]) and the behaviour is split
//! across sibling modules by responsibility: [`bootstrap`] (initial load),
//! [`draft`] (profile/preset/parameter editing), [`watch`] (background
//! preset polling), [`jobs`] (background actions), and [`view`] (rendering).

mod bootstrap;
mod draft;
mod jobs;
mod state;
mod view;
mod watch;

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Subscription, Window,
};
use subspace_framework::dashboard::workspace::{SpaceDock, SpaceDocks};
use subspace_framework::dashboard::{DashboardEvent, prefs};

use crate::environments::EnvironmentManager;
use crate::shell::{self, Shell, ShellRequest};
use crate::studio::Studio;

use state::{DraftInputs, JobState, ViewState};

/// Maximum serialized agent-context size; larger drafts drop the snapshot
/// rather than send an oversized blob.
const AGENT_CONTEXT_MAX_BYTES: usize = 24 * 1024;

/// Parameter names a control can bind, in the fixed order the brief lists.
const ADDABLE_PARAMETERS: [(&str, &str); 5] = [
    ("start", "Start date"),
    ("end", "End date"),
    ("currency", "Currency"),
    ("service", "Service"),
    ("group_by", "Group by"),
];

pub struct Workbench {
    focus: FocusHandle,
    studio: Studio,
    environments: EnvironmentManager,
    view: ViewState,
    prototype_views: bool,
    inputs: DraftInputs,
    jobs: JobState,
    draft: crate::contracts::Preset,
    presets: Vec<crate::contracts::Preset>,
    catalog: Vec<crate::contracts::EnvironmentInfo>,
    selected_env: Option<String>,
    last_result: Option<crate::contracts::RunResult>,
    dashboard_layout: crate::contracts::DashboardLayout,
    recipe_source: String,
    networked: bool,
    projects: Entity<crate::project_browser::ProjectBrowser>,
    _shell_requests: Subscription,
}

impl EventEmitter<DashboardEvent> for Workbench {}

impl Focusable for Workbench {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Workbench {
    pub(crate) fn new_at(
        root: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let projects =
            cx.new(|cx| crate::project_browser::ProjectBrowser::new(root.clone(), window, cx));
        let shell_requests = cx.subscribe_in(
            &projects,
            window,
            |this: &mut Self, _, request: &ShellRequest, window, cx| {
                this.apply_shell_request(*request, window, cx);
            },
        );
        let navigator = shell::dock_shown(prefs::get(cx, shell::NAVIGATOR_KEY).as_deref());
        let loaded = bootstrap::load(root);
        let inputs = DraftInputs::new(&loaded.draft.title, window, cx);
        let mut workbench = Self {
            focus: cx.focus_handle(),
            studio: loaded.studio,
            environments: loaded.environments,
            view: ViewState::new(navigator),
            prototype_views: subspace_framework::dashboard::services(cx)
                .is_some_and(|services| services.config.feature_enabled(&crate::PROTOTYPE_VIEWS)),
            inputs,
            jobs: JobState::new(
                "Adjust the controls and Run, or explore the saved Dashboard and Environments.",
            ),
            draft: loaded.draft,
            presets: loaded.presets,
            catalog: loaded.catalog,
            selected_env: None,
            last_result: loaded.last_result,
            dashboard_layout: loaded.dashboard_layout,
            recipe_source: loaded.recipe_source,
            networked: loaded.networked,
            projects,
            _shell_requests: shell_requests,
        };
        workbench.rebuild_param_inputs(window, cx);
        workbench.sync_shell(cx);
        workbench
    }

    /// The shell facts every view draws from.
    fn shell(&self) -> Shell {
        Shell {
            views: shell::view_tab_order(self.prototype_views),
            navigator: self.view.navigator,
        }
    }

    /// Gives the project browser the shell facts it draws its own copy from.
    fn sync_shell(&mut self, cx: &mut Context<Self>) {
        let shell = self.shell();
        self.projects
            .update(cx, |projects, cx| projects.set_shell(shell, cx));
    }

    fn apply_shell_request(
        &mut self,
        request: ShellRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ShellRequest::Show(view) = request;
        self.show_view(view, window, cx);
    }

    /// Shows a view. The project browser observes its service only while it
    /// is the view in front.
    fn show_view(&mut self, view: state::ViewTab, window: &mut Window, cx: &mut Context<Self>) {
        self.view.tab = view;
        self.projects.update(cx, |projects, cx| {
            if view == state::ViewTab::Projects {
                projects.activate(window, cx);
            } else {
                projects.deactivate();
            }
        });
        cx.notify();
    }

    /// The status bar's switches: the navigator's, as it shows. Board Lab
    /// has no inspector.
    pub(crate) fn space_docks(&self) -> SpaceDocks {
        SpaceDocks {
            navigator: Some(self.view.navigator),
            inspector: None,
        }
    }

    /// A dock's status bar switch was used.
    pub(crate) fn toggle_space_dock(&mut self, dock: SpaceDock, cx: &mut Context<Self>) {
        if dock == SpaceDock::Navigator {
            self.toggle_navigator(cx);
        }
    }

    /// Shows or hides the navigator in every view, and stores the choice.
    fn toggle_navigator(&mut self, cx: &mut Context<Self>) {
        self.view.navigator = !self.view.navigator;
        prefs::set(
            cx,
            shell::NAVIGATOR_KEY,
            shell::dock_value(self.view.navigator),
        );
        self.sync_shell(cx);
        cx.notify();
    }

    /// Opens the New project form from another view.
    fn new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(state::ViewTab::Projects, window, cx);
        self.projects
            .update(cx, |projects, cx| projects.begin_creating(window, cx));
    }

    pub(crate) fn busy(&self, cx: &App) -> bool {
        self.jobs.busy || self.projects.read(cx).busy()
    }

    pub(crate) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view.tab.visible(self.prototype_views) == state::ViewTab::Projects {
            self.projects
                .update(cx, |projects, cx| projects.activate(window, cx));
        }
        if self.jobs.watcher.is_none() {
            watch::watch_presets(self, window, cx);
        }
    }

    pub(crate) fn deactivate(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.jobs.watcher = None;
        self.projects
            .update(cx, |projects, _| projects.deactivate());
    }

    pub(crate) fn agent_context(
        &self,
        cx: &App,
    ) -> Option<subspace_framework::dashboard::AgentContext> {
        if self.view.tab.visible(self.prototype_views) == state::ViewTab::Projects {
            return self.projects.read(cx).agent_context();
        }
        let mut preset = self.draft.clone();
        preset.snapshot = self
            .last_result
            .as_ref()
            .filter(|result| crate::studio::snapshot_matches(&preset, &result.dataset))
            .map(|result| result.dataset.clone());
        let mut json = serde_json::to_string(&preset).ok()?;
        if json.len() > AGENT_CONTEXT_MAX_BYTES {
            preset.snapshot = None;
            json = serde_json::to_string(&preset).ok()?;
        }
        Some(subspace_framework::dashboard::AgentContext {
            key: format!("workbench:{}", preset.id),
            label: format!("Board canvas: {}", preset.title),
            content: format!(
                "Active Board Lab canvas with synthetic fixture data. Current preset JSON: {json}\nUse board_lab_edit/add_control to add typed controls. Use board_lab_run/recipe to refresh through MXC. Use board_lab_edit/save to persist a new revision; the native board watches saved definitions. For a native chat preview use board_lab_inspect/canvas then open_canvas. Do not use host shell/file tools for this workbench. This instruction is guidance, not a substitute for tool enforcement."
            ),
        })
    }

    pub(crate) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.catalog = self.environments.catalog().unwrap_or_default();
        self.presets = self.studio.presets().unwrap_or_default();
        self.rebuild_param_inputs(window, cx);
        cx.notify();
    }

    pub(crate) fn status(&self, cx: &App) -> Option<SharedString> {
        if self.view.tab.visible(self.prototype_views) == state::ViewTab::Projects {
            return Some(self.projects.read(cx).status().into());
        }
        Some(self.jobs.status.clone())
    }
}
