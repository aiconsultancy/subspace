//! Native project workspace. Sessions belong to the root service, not this view.

mod commands;
mod creation;
mod facts;
mod observation;
mod sync;
mod view;

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
pub(crate) use creation::example;

use std::path::PathBuf;

use anyhow::Result;
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Subscription, Task, Window,
};
use subspace_framework::dashboard::AgentContext;

use crate::contracts::EnvironmentInfo;
use crate::editors::InstalledEditor;
use crate::project::LoadedProject;
use crate::project_service::{ProjectPresentation, ProjectService};
use crate::project_settings::ProjectSettings;
use crate::project_view::{ProjectView, ProjectViewEvent};
use crate::shell::{Shell, ShellRequest};

const AGENT_CONTEXT_MAX_BYTES: usize = 16 * 1024;

pub(crate) struct ProjectBrowser {
    root: PathBuf,
    focus: FocusHandle,
    shell: Shell,
    projects: Vec<LoadedProject>,
    environments: Vec<EnvironmentInfo>,
    editors: Vec<InstalledEditor>,
    presentation: Option<ProjectPresentation>,
    dashboard: Option<Entity<ProjectView>>,
    dashboard_subscription: Option<Subscription>,
    settings: Option<Entity<ProjectSettings>>,
    settings_subscription: Option<Subscription>,
    creating: bool,
    project_id: Entity<InputState>,
    _create_subscription: Subscription,
    environment: Entity<SelectState<SearchableVec<String>>>,
    busy: bool,
    status: String,
    error: Option<String>,
    task: Option<Task<()>>,
    observation: Option<Task<()>>,
    seen_outcomes: std::collections::BTreeSet<(String, String)>,
}

pub(super) enum Outcome {
    Catalog(
        Vec<LoadedProject>,
        Vec<EnvironmentInfo>,
        Vec<InstalledEditor>,
    ),
    Opened(Box<ProjectPresentation>),
    Closed,
    Notice(String),
}

/// The owner of the shell facts hears what the browser's switch, view menu
/// and navigator switch ask for.
impl EventEmitter<ShellRequest> for ProjectBrowser {}

impl ProjectBrowser {
    pub(crate) fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project_id = cx.new(|cx| InputState::new(window, cx).placeholder("my-dashboard"));
        let create_subscription = cx.subscribe_in(
            &project_id,
            window,
            |this: &mut Self, _, event, window, cx| {
                if this.creating
                    && matches!(
                        event,
                        gpui_kit::component::input::InputEvent::PressEnter {
                            secondary: false,
                            shift: false
                        }
                    )
                {
                    this.create(window, cx);
                }
            },
        );
        Self {
            root,
            focus: cx.focus_handle(),
            shell: Shell::default(),
            projects: Vec::new(),
            environments: Vec::new(),
            editors: Vec::new(),
            presentation: None,
            dashboard: None,
            dashboard_subscription: None,
            settings: None,
            settings_subscription: None,
            creating: false,
            project_id,
            _create_subscription: create_subscription,
            environment: cx
                .new(|cx| SelectState::new(SearchableVec::new(Vec::new()), None, window, cx)),
            busy: false,
            status: "Open a project or create one. Starting a worker is always explicit.".into(),
            error: None,
            task: None,
            observation: None,
            seen_outcomes: Default::default(),
        }
    }

    pub(crate) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.observation.is_some() {
            return;
        }
        self.refresh_catalog(window, cx);
        self.observe(window, cx);
    }

    /// Cancels presentation observation only. Never closes or stops a root-owned worker.
    pub(crate) fn deactivate(&mut self) {
        self.observation = None;
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy
    }

    /// The line for the shell's status bar: the guidance, the worker's state
    /// or the latest result, and `Working…` while an operation is pending.
    pub(crate) fn status(&self) -> String {
        facts::status_line(self.busy, &self.status)
    }

    pub(crate) fn agent_context(&self) -> Option<AgentContext> {
        let project = self.presentation.as_ref()?;
        let definition = &project.working.definition;
        let json = serde_json::to_string(definition).ok()?;
        let bounded = if json.len() <= AGENT_CONTEXT_MAX_BYTES {
            json
        } else {
            "Use project inspect for this larger definition.".into()
        };
        Some(AgentContext {
            key: format!("board-project:{}", definition.id),
            label: format!("Project: {}", definition.title),
            content: format!(
                "Board Lab alpha project. Working definition revision: {}; source revision: {}. Running revision: {:?}. Session: {}; input revision: {}. Definition (variable names only, no private settings): {}. Use board-owned project tools and versioned native schemas. Saving does not start or restart execution.",
                project.working.revision.definition,
                project.working.revision.source,
                project.applied_revision,
                project.live.session_id(),
                project.live.input_revision(),
                bounded,
            ),
        })
    }

    fn run(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&ProjectService) -> Result<Outcome> + Send + 'static,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let root = self.root.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let service = crate::project_service::shared(&root)?;
                    operation(&service)
                })
                .await;
            _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.finish(result, window, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn finish(&mut self, result: Result<Outcome>, window: &mut Window, cx: &mut Context<Self>) {
        match result {
            Ok(Outcome::Opened(project)) => {
                self.creating = false;
                self.accept(*project, window, cx);
            }
            Ok(Outcome::Catalog(projects, environments, editors)) => {
                self.projects = projects;
                self.environments = environments;
                self.editors = editors;
                self.sync_environments(window, cx);
            }
            Ok(Outcome::Closed) => {
                self.presentation = None;
                self.dashboard = None;
                self.dashboard_subscription = None;
                self.status = "Project closed; its worker is stopped.".into();
            }
            Ok(Outcome::Notice(message)) => self.status = message,
            Err(error) => self.fail(error.to_string(), cx),
        }
    }

    fn fail(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }

    fn accept(
        &mut self,
        presentation: ProjectPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_dashboard(&presentation, window, cx);
        if let Some(error) = &presentation.error {
            self.error = Some(error.clone());
        }
        if !self
            .projects
            .iter()
            .any(|project| project.definition.id == presentation.working.definition.id)
        {
            self.projects.push(presentation.working.clone());
        }
        self.status = format!(
            "{:?} · {}",
            presentation.status, presentation.working.definition.title
        );
        self.presentation = Some(presentation);
        cx.notify();
    }

    fn sync_dashboard(
        &mut self,
        presentation: &ProjectPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same = self.presentation.as_ref().is_some_and(|current| {
            current.working.definition.id == presentation.working.definition.id
        });
        if same && let Some(view) = &self.dashboard {
            if self
                .presentation
                .as_ref()
                .is_none_or(|current| !sync::same_live(&current.live, &presentation.live))
            {
                view.update(cx, |view, cx| view.sync(&presentation.live, window, cx));
            }
        } else {
            let view = cx.new(|cx| ProjectView::new(&presentation.live, window, cx));
            let id = presentation.working.definition.id.clone();
            self.dashboard_subscription =
                Some(
                    cx.subscribe_in(&view, window, move |this, _, event, window, cx| {
                        if this.selected_id().as_deref() == Some(&id) {
                            this.dispatch(event.clone(), window, cx);
                        }
                    }),
                );
            self.dashboard = Some(view);
        }
    }

    fn dispatch(&mut self, event: ProjectViewEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            self.fail(
                "A project operation is pending. Wait, then try the control or action again."
                    .into(),
                cx,
            );
            return;
        }
        let Some(id) = self.selected_id() else {
            return;
        };
        self.run(window, cx, move |service| {
            match event {
                ProjectViewEvent::Input {
                    key,
                    value,
                    expected_revision,
                    session_id,
                } => {
                    service.set_value(
                        &id,
                        &key,
                        value,
                        &crate::project_service::InputRevision::new(session_id, expected_revision),
                    )?;
                }
                ProjectViewEvent::Action {
                    action,
                    expected_revision,
                    session_id,
                } => {
                    service.invoke(
                        &id,
                        &action,
                        &crate::project_service::InputRevision::new(session_id, expected_revision),
                    )?;
                }
            }
            Ok(Outcome::Opened(Box::new(service.open(&id)?)))
        });
    }

    fn selected_id(&self) -> Option<String> {
        self.presentation
            .as_ref()
            .map(|project| project.working.definition.id.clone())
    }
}

#[cfg(test)]
mod tests;
