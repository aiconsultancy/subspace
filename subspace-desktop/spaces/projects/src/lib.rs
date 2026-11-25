//! Projects and their milestones, served by the active API account.

mod editor;
mod initiative;
mod milestone_page;
mod project_editor;
mod requests;
mod view;

use std::time::Duration;

use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription, Task,
    Window,
};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::projects::{
    Milestone, MilestoneChange, MilestoneFields, PlanningProject, ProjectFields, ProjectUpdate,
};
use subspace_api_client::{Error, signed_in_generation};
use subspace_framework::dashboard::{
    Dashboard, DashboardEvent, Modal, ReturnLocation,
    contracts::BoardContracts,
    menu::{Menu, MenuAnchors},
    settings::BoardSettings,
};
use subspace_framework::lucide::IconName;

const ACCOUNT_WATCH: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Overview,
    Milestones,
}

struct Record {
    project: PlanningProject,
    milestones: Result<Vec<Milestone>, Error>,
    update: Result<Option<ProjectUpdate>, Error>,
}

enum Write {
    Project(ProjectFields),
    Add(MilestoneFields),
    Change(Milestone, MilestoneChange),
}

pub struct Projects {
    focus: FocusHandle,
    account: Option<u64>,
    active: bool,
    initiatives: Entity<Catalog>,
    initiative_id: Option<String>,
    initiative_hidden: bool,
    initiative_menu: Option<Entity<Menu>>,
    menu_anchors: MenuAnchors,
    projects: Result<Vec<PlanningProject>, Error>,
    selected: Option<String>,
    record: Option<Result<Record, Error>>,
    tab: Tab,
    milestone_pages: milestone_page::Pages,
    editor: Option<editor::Editor>,
    error: Option<String>,
    blocked_write: bool,
    directory_task: Option<Task<()>>,
    record_task: Option<Task<()>>,
    write_task: Option<Task<()>>,
    _account_task: Task<()>,
    _initiative_changes: Subscription,
}

impl EventEmitter<DashboardEvent> for Projects {}

impl Focusable for Projects {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Dashboard for Projects {
    const ID: &'static str = "projects";
    const TITLE: &'static str = "Projects";
    const DESCRIPTION: &'static str = "Browse projects and manage their milestones";
    const ICON: Option<&'static str> = Some("icons/folder-open.svg");
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> BoardSettings {
        BoardSettings {
            icon: Some(IconName::FolderOpen),
            description: Self::DESCRIPTION,
            notifications: false,
            background_updates: false,
            ..Default::default()
        }
    }

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let initiatives = Catalog::shared(cx);
        let (initiative_id, initiative_hidden) = initiative::scope(initiatives.read(cx));
        let changes = cx.observe(&initiatives, |this, _, cx| this.follow_initiative(cx));
        Self {
            focus: cx.focus_handle(),
            account: signed_in_generation(),
            active: false,
            initiatives,
            initiative_id,
            initiative_hidden,
            initiative_menu: None,
            menu_anchors: MenuAnchors::default(),
            projects: Ok(Vec::new()),
            selected: None,
            record: None,
            tab: Tab::Overview,
            milestone_pages: milestone_page::Pages::default(),
            editor: None,
            error: None,
            blocked_write: false,
            directory_task: None,
            record_task: None,
            write_task: None,
            _account_task: cx.spawn(async |this, cx| {
                loop {
                    cx.background_executor().timer(ACCOUNT_WATCH).await;
                    if this.update(cx, |this, cx| this.follow_account(cx)).is_err() {
                        break;
                    }
                }
            }),
            _initiative_changes: changes,
        }
    }

    fn activate(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.active = true;
        self.follow_account(cx);
        if !self.busy(cx) {
            self.refresh(cx);
        }
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active = false;
        self.milestone_pages.preview = None;
        self.menu_anchors.dismiss(window, cx);
        self.initiative_menu = None;
    }

    fn contracts() -> BoardContracts {
        milestone_page::CONTRACTS
    }

    fn return_location(&self, _: &App) -> Option<ReturnLocation> {
        self.milestone_place()
    }

    fn return_to(&mut self, key: &str, _: &mut Window, cx: &mut Context<Self>) -> bool {
        self.return_to_milestone(key, cx)
    }

    fn busy(&self, _: &App) -> bool {
        self.directory_task.is_some() || self.record_task.is_some() || self.write_task.is_some()
    }

    fn status(&self, _: &App) -> Option<SharedString> {
        let service = subspace_api_client::service_name();
        let status = match &self.projects {
            Err(Error::SignInNeeded) => format!("{service} · Sign in required"),
            Err(_) => format!("{service} · Projects unavailable"),
            Ok(_) if self.directory_task.is_some() => format!("{service} · Loading projects"),
            Ok(_) => match self.visible_projects().count() {
                1 => format!("{service} · 1 project"),
                count => format!("{service} · {count} projects"),
            },
        };
        Some(status.into())
    }

    fn reload(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.write_task.is_none() {
            self.refresh(cx);
            self.reload_milestone_issues(cx);
        }
    }

    fn modal(&self, _: &App) -> Option<Modal> {
        if self.editor.is_some() {
            Some(Modal::Dialog)
        } else {
            self.initiative_menu.as_ref().map(|_| Modal::Menu)
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        self.initiative_menu = None;
        self.dismiss_editor(window, cx);
    }
}

// qual:allow(coupling, oi) reason: "The Space controls its own account, selection and request lifetimes."
impl Projects {
    fn follow_initiative(&mut self, cx: &mut Context<Self>) {
        let (id, hidden) = initiative::scope(self.initiatives.read(cx));
        if self.initiative_id != id || self.initiative_hidden != hidden {
            self.initiative_id = id;
            self.initiative_hidden = hidden;
            self.reconcile_selection(cx);
        }
        cx.notify();
    }

    fn follow_account(&mut self, cx: &mut Context<Self>) {
        let account = signed_in_generation();
        if account == self.account {
            return;
        }
        self.account = account;
        self.directory_task = None;
        self.record_task = None;
        self.write_task = None;
        self.projects = Ok(Vec::new());
        self.selected = None;
        self.record = None;
        self.editor = None;
        self.error = None;
        self.blocked_write = false;
        self.tab = Tab::Overview;
        self.milestone_pages = milestone_page::Pages::default();
        self.initiative_menu = None;
        if self.active {
            self.refresh(cx);
        }
        cx.notify();
    }

    fn reconcile_selection(&mut self, cx: &mut Context<Self>) {
        if self.editor.is_some() || self.write_task.is_some() {
            return;
        }
        let mut rows = self.visible_projects();
        let first = rows.next();
        let selected = self.selected.as_ref();
        if first
            .into_iter()
            .chain(rows)
            .any(|row| Some(&row.id) == selected)
        {
            return;
        }
        self.selected = first.map(|row| row.id.clone());
        self.record = None;
        self.error = None;
        self.blocked_write = false;
        self.tab = Tab::Overview;
        self.milestone_pages = milestone_page::Pages::default();
        self.read_record(cx);
    }

    fn in_scope(&self, project: &PlanningProject) -> bool {
        !self.initiative_hidden
            && self
                .initiative_id
                .as_ref()
                .is_none_or(|id| project.initiative_id.as_ref() == Some(id))
    }

    fn visible_projects(&self) -> impl Iterator<Item = &PlanningProject> {
        self.projects
            .iter()
            .flatten()
            .filter(|project| self.in_scope(project))
    }

    fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(id)
            || self.editor.is_some()
            || self.write_task.is_some()
        {
            return;
        }
        self.selected = Some(id.to_owned());
        self.record = None;
        self.error = None;
        self.blocked_write = false;
        self.tab = Tab::Overview;
        self.milestone_pages = milestone_page::Pages::default();
        self.read_record(cx);
        cx.notify();
    }
}
