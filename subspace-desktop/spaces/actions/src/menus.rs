//! Shared searchable menus for account and run filters.

use std::collections::BTreeSet;

use gpui_kit::{App, Context, Entity, Window};
use subspace_framework::dashboard::anchored::Anchor;
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::dashboard::{Glyph, Tone, prefs};

use crate::{
    Actions,
    model::{Filters, StatusFilter},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Picker {
    Account,
    Branch,
    Workflow,
    Actor,
    Status,
    Grouping,
    Log,
}

impl Picker {
    pub(crate) const FILTERS: [Self; 5] = [
        Self::Status,
        Self::Branch,
        Self::Workflow,
        Self::Actor,
        Self::Grouping,
    ];

    pub(crate) fn id(self) -> String {
        format!("actions-{}", self.title())
    }
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Account => "Account",
            Self::Branch => "Branch",
            Self::Workflow => "Workflow",
            Self::Actor => "Actor",
            Self::Status => "Status",
            Self::Grouping => "Grouping",
            Self::Log => "Failed job log",
        }
    }

    pub(crate) fn filter_value(self, filters: &Filters) -> Option<String> {
        match self {
            Self::Status => Some(filters.status.title().to_owned()),
            Self::Branch => filters.branch.clone(),
            Self::Workflow => filters.workflow.clone(),
            Self::Actor => filters.actor.clone(),
            Self::Grouping => Some(
                if filters.by_status {
                    "Status"
                } else {
                    "Workflow"
                }
                .into(),
            ),
            Self::Account | Self::Log => None,
        }
    }
}

pub(crate) fn open_picker(
    this: &mut Actions,
    kind: Picker,
    anchor: Option<Entity<Anchor>>,
    window: &mut Window,
    cx: &mut Context<Actions>,
) {
    this.filter_popup = None;
    let choices = picker_values(this, kind);
    let options = choices
        .into_iter()
        .map(|value| MenuOption::new(value.clone(), value).icon(Glyph::new("◇", Tone::Muted)))
        .collect();
    let contents = MenuContents::new(kind.title(), "Filter choices", options);
    let anchor = anchor.unwrap_or_else(|| this.menu_anchors.anchor(kind.id(), cx));
    let Some(menu) = this.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe(&menu, move |this, _, event, cx| {
        this.menu = None;
        if let MenuEvent::Pick(value) = event {
            pick(this, kind, value.to_string(), cx);
        }
        cx.notify();
    })
    .detach();
    this.menu = Some(menu);
    cx.notify();
}

pub(crate) fn picker_values(this: &Actions, kind: Picker) -> Vec<String> {
    match kind {
        Picker::Log => this
            .detail
            .jobs
            .iter()
            .filter(|job| matches!(job.status.as_str(), "failure" | "timed_out"))
            .map(|job| format!("{} · {}", job.id, job.name))
            .collect(),
        Picker::Account => this.accounts.clone(),
        Picker::Status => StatusFilter::ALL
            .iter()
            .map(|s| s.title().to_owned())
            .collect(),
        Picker::Grouping => vec!["Workflow".into(), "Status".into()],
        _ => run_choices(this, kind),
    }
}

fn run_choices(this: &Actions, kind: Picker) -> Vec<String> {
    let mut values = BTreeSet::new();
    for run in this
        .snapshot()
        .into_iter()
        .flat_map(|data| &data.runs)
        .filter_map(|record| record.run.as_ref())
    {
        let value = match kind {
            Picker::Branch => &run.branch,
            Picker::Workflow => &run.workflow,
            _ => &run.actor,
        };
        values.insert(value.clone());
    }
    std::iter::once("All".into()).chain(values).collect()
}

pub(crate) fn pick(this: &mut Actions, kind: Picker, value: String, cx: &mut Context<Actions>) {
    let filter = (value != "All").then(|| value.clone());
    match kind {
        Picker::Log => {
            if let Some(id) = value.split(' ').next().and_then(|id| id.parse().ok()) {
                this.load_job_log(id, cx);
            }
            return;
        }
        Picker::Account => {
            this.switch_account(value, cx);
            return;
        }
        Picker::Branch => this.filters.branch = filter,
        Picker::Workflow => this.filters.workflow = filter,
        Picker::Actor => this.filters.actor = filter,
        Picker::Status => {
            this.filters.status = StatusFilter::ALL
                .into_iter()
                .find(|s| s.title() == value)
                .unwrap_or_default()
        }
        Picker::Grouping => this.filters.by_status = value == "Status",
    }
    save_filters(&this.filters, cx);
    this.rebuild(cx);
}

/// The empty state's Clear filters: every filter back to All, saved, with
/// the grouping kept.
pub(crate) fn clear_filters(this: &mut Actions, cx: &mut Context<Actions>) {
    this.filter_popup = None;
    this.filters = this.filters.cleared();
    save_filters(&this.filters, cx);
    this.rebuild(cx);
}

/// Saves the filters and grouping as preferences, which are not secrets.
fn save_filters(filters: &Filters, cx: &mut App) {
    let grouping = if filters.by_status {
        "Status"
    } else {
        "Workflow"
    };
    prefs::set(cx, "actions.grouping", grouping);
    prefs::set(
        cx,
        "actions.branch",
        filters.branch.clone().unwrap_or_default(),
    );
    prefs::set(
        cx,
        "actions.workflow",
        filters.workflow.clone().unwrap_or_default(),
    );
    prefs::set(
        cx,
        "actions.actor",
        filters.actor.clone().unwrap_or_default(),
    );
    prefs::set(cx, "actions.status", filters.status.title());
}
