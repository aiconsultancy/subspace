//! The shared list's hooks. One click on a run selects it and loads its
//! jobs for the inspector; Enter, a double click or the inspector's Open
//! opens it in a tab beside the view. The list's own wide detail panel is
//! not used: the inspector replaces it.

mod fetch;

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, IntoElement, ParentElement as _,
    SharedString, Styled as _, div,
};
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::list::{
    Detail, Group, GroupNote, ListDashboard, ListState, Row,
};
use subspace_framework::dashboard::row_menu::{MenuCommand, MenuGroup, RowMenu};
use subspace_framework::dashboard::{Tokens, feedback};
use subspace_framework::lucide::IconName;

use crate::{Actions, model::RunTarget, notes, rows};

/// The room the list keeps beside the inspector, so the toolbar above the
/// list still shows its filters, Trigger and the dock switches.
const MIN_LIST_WIDTH_PX: f32 = 320.;

/// One run's jobs, the job a Flow click selected and the log asked for.
/// The selection's sits on the board; the opened tab keeps its own copy.
#[derive(Clone, Default)]
pub(crate) struct RunDetail {
    pub target: Option<RunTarget>,
    pub jobs: Vec<crate::graph::GraphJob>,
    pub loading: bool,
    /// Why the jobs did not load. A log that fails says so in `log`.
    pub error: Option<String>,
    pub log: LogState,
    pub selected_job: Option<u64>,
    /// The jobs read this copy waits for or last took. An answer to any
    /// other read, such as an earlier selection's, is dropped.
    pub request: u64,
    /// The log read this copy waits for or last took.
    pub log_request: u64,
}

/// A failed job's log in the opened tab: asked for, shown, or failed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum LogState {
    #[default]
    None,
    Loading,
    Loaded(String),
    Failed(String),
}

/// Whether two targets are the same run, whatever their attempts.
pub(crate) fn same_run(a: &RunTarget, b: &RunTarget) -> bool {
    a.repo == b.repo && a.id == b.id
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

/// Flow's content: the selected run's job graph, or what to do while no
/// run is selected.
pub(crate) fn flow(this: &Actions, cx: &App) -> AnyElement {
    if this.detail.target.is_none() {
        return feedback::empty(
            &Tokens::current(cx),
            "actions-flow-empty",
            "No run selected",
            "Select a run in Runs, then select Flow.",
        )
        .into_any_element();
    }
    h_flex()
        .size_full()
        .child(div().flex_1().min_w_0().h_full().child(this.graph.clone()))
        .into_any_element()
}

impl ListDashboard for Actions {
    /// Rows and group headers start at the workspace's one text edge.
    fn list_side_inset(&self) -> Option<gpui_kit::Pixels> {
        Some(self.text_edge)
    }

    fn comfortable_rows(&self) -> bool {
        true
    }

    fn min_list_width(&self) -> f32 {
        MIN_LIST_WIDTH_PX
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

    /// None: the inspector shows the selection, so the shared list never
    /// draws its own wide detail panel.
    fn detail(&self, _key: &str) -> Option<Detail> {
        None
    }

    fn row_leading(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        crate::lanes::leading(self, row, cx)
    }

    fn row_trailing(&self, row: &Row, cx: &App) -> Option<AnyElement> {
        crate::lanes::trailing(self, row, cx)
    }

    fn row_accessible_label(&self, row: &Row, _cx: &App) -> SharedString {
        self.record(&row.key).map_or_else(
            || row.title.clone(),
            |record| rows::accessible_label(record).into(),
        )
    }

    /// One click selects a row and fills the inspector; a double click or
    /// Enter opens it. Group headers still fold on one click.
    fn activates_on_click(&self, _row: &Row) -> bool {
        false
    }

    /// A selected run loads its jobs for the inspector and Flow. Choosing
    /// another line while the opened tab shows brings back the list that
    /// holds it, so the tab never shows one run while the inspector and the
    /// writes act on another.
    fn on_selection_changed(&mut self, selected: Option<SharedString>, cx: &mut Context<Self>) {
        if let Some(open) = self.open.as_mut()
            && open.active
            && selected.as_deref() != Some(open.key.as_str())
        {
            open.active = false;
        }
        let target =
            selected.and_then(|key| self.record(&key).and_then(|record| record.run.clone()));
        if let Some(target) = target {
            self.load_detail(target, cx);
        }
    }

    /// Opening shows the row in a tab beside the view. The list's open flag
    /// goes straight back down, so moving the selection opens nothing more.
    fn on_open(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.list.close_detail();
        self.open_tab(&key, cx);
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
        if let Some(record) = self.record(&row.key) {
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

    /// Refresh repository for a repository header, in the Act group;
    /// unavailable until GitHub is signed in.
    fn row_commands(board: &Entity<Self>, cx: &App) -> Vec<MenuCommand> {
        let this = board.read(cx);
        let Some(repo) = selected_repository(this) else {
            return Vec::new();
        };
        let repo = repo.to_owned();
        let weak = board.downgrade();
        let refresh = MenuCommand::new(
            MenuGroup::Act,
            "Refresh repository",
            IconName::RefreshCw,
            move |_, cx| {
                _ = weak.update(cx, |this, cx| this.refresh_repo(repo.clone(), cx));
            },
        );
        vec![refresh.disabled(this.login.is_none(), "Sign in to GitHub to refresh it.")]
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
