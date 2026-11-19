//! Run details load from the list's open hook, never from render.

mod fetch;

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div,
};
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::list::{
    Detail, Group, GroupNote, ListDashboard, ListState, Row, Section,
};
use subspace_framework::dashboard::row_menu::RowMenu;
use subspace_framework::dashboard::{Tokens, feedback};
use subspace_framework::lucide::IconName;

use crate::{Actions, model::RunTarget, notes, rows};

#[derive(Default)]
pub(crate) struct RunDetail {
    pub target: Option<RunTarget>,
    pub jobs: Vec<crate::graph::GraphJob>,
    pub loading: bool,
    pub error: Option<String>,
    pub log: Option<String>,
    pub selected_job: Option<u64>,
}

pub(crate) fn new_graph(cx: &mut Context<Actions>) -> Entity<crate::graph::FlowGraph> {
    let graph = cx.new(crate::graph::FlowGraph::new);
    cx.subscribe(&graph, |this, _, event: &crate::graph::JobSelected, cx| {
        this.detail.selected_job = Some(event.0);
        cx.notify();
    })
    .detach();
    graph
}

pub(crate) fn flow(this: &Actions, window: &mut Window, cx: &mut Context<Actions>) -> AnyElement {
    if this.detail.target.is_none() {
        return feedback::empty(
            &Tokens::current(cx),
            "actions-flow-empty",
            "No run selected",
            "Open a run in Runs, then select Flow.",
        )
        .into_any_element();
    }
    let selected = this
        .detail
        .jobs
        .iter()
        .find(|job| Some(job.id) == this.detail.selected_job);
    let weak = cx.entity().downgrade();
    let panel = selected.map(|job| {
        let header = subspace_framework::dashboard::panel::PanelHeader {
            kind: "Job".into(),
            id: job.id.to_string().into(),
            label: Some(job.name.clone().into()),
            ..Default::default()
        };
        let body = div()
            .child(subspace_framework::dashboard::rich::plain_text(
                "actions-flow-job-detail",
                &format!("{} · {}\n\n{}", job.status, job.duration, job.steps),
            ))
            .into_any_element();
        let actions = subspace_framework::dashboard::panel::PanelActions::new(
            move |_, cx| {
                _ = weak.update(cx, |this, cx| {
                    this.detail.selected_job = None;
                    cx.notify();
                });
            },
            None,
        );
        subspace_framework::dashboard::panel::frame(header, body, actions, window, cx)
    });
    gpui_kit::component::h_flex()
        .size_full()
        .child(div().flex_1().min_w_0().h_full().child(this.graph.clone()))
        .children(panel)
        .into_any_element()
}

impl ListDashboard for Actions {
    fn list_side_inset(&self) -> Option<gpui_kit::Pixels> {
        Some(crate::toolbar::LIST_SIDE_INSET)
    }

    fn comfortable_rows(&self) -> bool {
        true
    }

    fn list(&self) -> &ListState {
        &self.list
    }
    fn list_mut(&mut self) -> &mut ListState {
        &mut self.list
    }
    fn can_navigate(&self, _cx: &App) -> bool {
        self.pending.is_none()
    }

    fn detail(&self, key: &str) -> Option<Detail> {
        let record = self.records().iter().find(|record| record.key == key)?;
        let mut detail = rows::detail(record);
        if record.run.as_ref() == self.detail.target.as_ref() {
            detail.sections.extend(self.detail.jobs.iter().map(|job| {
                Section::text(
                    format!("{} · {} · {}", job.name, job.status, job.duration),
                    job.steps.clone(),
                )
            }));
            if self.detail.jobs.is_empty() && !self.detail.loading {
                detail.sections.push(Section::text(
                    "Jobs",
                    "No jobs yet. A queued run may not have started.",
                ));
            }
            if let Some(error) = &self.detail.error {
                detail
                    .sections
                    .push(Section::text("Load failed", error.clone()));
            }
            if let Some(log) = &self.detail.log {
                detail.sections.push(Section::text("Job log", log.clone()));
            }
        }
        Some(detail)
    }

    fn on_open(&mut self, key: SharedString, cx: &mut Context<Self>) {
        let target = self
            .records()
            .iter()
            .find(|record| record.key == key.as_ref())
            .and_then(|record| record.run.clone());
        if let Some(target) = target {
            self.load_detail(target, cx);
        }
    }

    fn on_panel_toggled(&mut self, open: bool, _cx: &mut Context<Self>) {
        if !open {
            self.detail.log = None;
        }
    }

    fn panel_extra(&self, _key: &str, cx: &App) -> Option<AnyElement> {
        self.detail.loading.then(|| {
            feedback::loading_line(
                &Tokens::current(cx),
                "actions-jobs-loading",
                "Loading jobs…",
            )
            .into_any_element()
        })
    }

    fn empty_message(&self) -> (SharedString, SharedString) {
        if self.repos.is_empty() {
            (
                "No favourite repositories".into(),
                "Star repositories with Repositories… in the PRs board, or add them in Settings."
                    .into(),
            )
        } else {
            let (title, message) = notes::no_matches(&self.filters);
            (title.into(), message.into())
        }
    }

    /// With repositories, the list is empty only when the filters hide every
    /// loaded run, so the next step is clearing them.
    fn empty_actions(&self, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        if self.repos.is_empty() {
            return None;
        }
        let clear = cx.listener(|this: &mut Self, _: &ClickEvent, _, cx| {
            crate::menus::clear_filters(this, cx);
        });
        let button = feedback::recovery_action(
            "actions-clear-filters",
            Intent::Primary,
            IconName::FunnelX,
            "Clear filters",
            clear,
        );
        Some(vec![button.into_any_element()])
    }

    fn group_note(&self, group: &Group, _cx: &App) -> Option<GroupNote> {
        self.notes.get(&group.key).cloned()
    }

    fn row_menu(&self, row: &Row) -> RowMenu {
        let mut menu = RowMenu::from_row(row);
        if let Some(record) = self
            .records()
            .iter()
            .find(|record| record.key == row.key.as_ref())
        {
            menu = menu.link("GitHub", &record.link);
        }
        menu
    }

    fn group_menu(&self, group: &Group) -> Option<RowMenu> {
        let repo = crate::model::group_repo(&group.key);
        Some(
            RowMenu::new(repo)
                .link("GitHub", &format!("https://github.com/{repo}"))
                .opens_details(false),
        )
    }

    fn row_submenus(
        board: &Entity<Self>,
        menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let this = board.read(cx);
        let Some(repo) = selected_repository(this) else {
            return menu;
        };
        let repo = repo.to_owned();
        let weak = board.downgrade();
        menu.separator().item(
            PopupMenuItem::new("Refresh repository")
                .icon(IconName::RefreshCw)
                .disabled(this.login.is_none())
                .on_click(move |_, _, cx| {
                    _ = weak.update(cx, |this, cx| this.refresh_repo(repo.clone(), cx));
                }),
        )
    }
}

pub(crate) fn selected_repository(actions: &Actions) -> Option<&str> {
    let selected = actions.list.selected()?;
    actions
        .list
        .groups()
        .iter()
        .find(|group| &group.key == selected)
        .map(|group| crate::model::group_repo(&group.key))
}
