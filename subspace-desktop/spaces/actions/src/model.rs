//! Session-only presentation state and pure selection rules.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    Runs,
    Deployments,
    Releases,
    Flow,
}

impl View {
    pub(crate) const ALL: [Self; 4] = [Self::Runs, Self::Deployments, Self::Releases, Self::Flow];

    pub(crate) fn title(self) -> &'static str {
        self.presentation().0
    }

    pub(crate) fn icon(self) -> subspace_framework::lucide::IconName {
        self.presentation().1
    }

    fn presentation(self) -> (&'static str, subspace_framework::lucide::IconName) {
        use subspace_framework::lucide::IconName;
        match self {
            Self::Runs => ("Runs", IconName::Play),
            Self::Deployments => ("Deployments", IconName::Cloud),
            Self::Releases => ("Releases", IconName::Tag),
            Self::Flow => ("Flow", IconName::GitBranch),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Filters {
    pub status: StatusFilter,
    pub branch: Option<String>,
    pub workflow: Option<String>,
    pub actor: Option<String>,
    pub by_status: bool,
}

impl Filters {
    /// Whether any filter can hide a run. Grouping hides none.
    pub(crate) fn narrows(&self) -> bool {
        self.status != StatusFilter::All
            || self.branch.is_some()
            || self.workflow.is_some()
            || self.actor.is_some()
    }

    /// Every filter back to All, with the grouping kept.
    pub(crate) fn cleared(&self) -> Self {
        Self {
            by_status: self.by_status,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StatusFilter {
    #[default]
    All,
    Active,
    Failed,
    Success,
}

impl StatusFilter {
    pub(crate) const ALL: [Self; 4] = [Self::All, Self::Active, Self::Failed, Self::Success];

    pub(crate) fn title(self) -> &'static str {
        self.words().0
    }

    /// The word before "runs" in a note, such as "failed"; empty for All.
    pub(crate) fn adjective(self) -> &'static str {
        self.words().1
    }

    /// The filter's title and its adjective, in one place per variant.
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::All => ("All", ""),
            Self::Active => ("Active", "active"),
            Self::Failed => ("Failed", "failed"),
            Self::Success => ("Success", "successful"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunTarget {
    pub repo: String,
    pub id: u64,
    pub workflow_id: u64,
    pub workflow: String,
    pub path: String,
    pub sha: String,
    pub branch: String,
    pub actor: String,
    pub attempt: u64,
}

/// A record carries service facts; shared row and detail controls render it.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub key: String,
    pub repo: String,
    pub title: String,
    pub status: String,
    pub active: bool,
    pub link: String,
    pub properties: Vec<(String, String)>,
    pub sections: Vec<(String, String)>,
    pub run: Option<RunTarget>,
    pub approval: Option<(u64, bool)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub runs: Vec<Record>,
    pub deployments: Vec<Record>,
    pub releases: Vec<Record>,
    pub errors: BTreeMap<String, String>,
    /// `owner/name: Source` for each source read successfully at least once,
    /// so an empty list can tell "nothing there" from "never read".
    pub loaded: BTreeSet<String>,
}

pub(crate) fn active(status: &str) -> bool {
    matches!(
        status,
        "queued" | "in_progress" | "waiting" | "requested" | "pending"
    )
}

pub(crate) fn matches(record: &Record, filters: &Filters) -> bool {
    let Some(run) = &record.run else { return true };
    let status = match filters.status {
        StatusFilter::All => true,
        StatusFilter::Active => record.active,
        StatusFilter::Failed => matches!(record.status.as_str(), "failure" | "timed_out"),
        StatusFilter::Success => record.status == "success",
    };
    status
        && filters
            .branch
            .as_ref()
            .is_none_or(|value| *value == run.branch)
        && filters
            .workflow
            .as_ref()
            .is_none_or(|value| *value == run.workflow)
        && filters
            .actor
            .as_ref()
            .is_none_or(|value| *value == run.actor)
}

const GROUP_SEPARATOR: &str = " · ";

pub(crate) fn group_name(record: &Record, by_status: bool) -> String {
    let suffix = if by_status {
        &record.status
    } else {
        record.run.as_ref().map_or("", |run| run.workflow.as_str())
    };
    format!("{}{GROUP_SEPARATOR}{suffix}", record.repo)
}

pub(crate) fn group_repo(key: &str) -> &str {
    key.split_once(GROUP_SEPARATOR)
        .map_or(key, |(repo, _)| repo)
}

#[cfg(test)]
mod tests {
    use super::{
        Filters, Record, RunTarget, StatusFilter, View, active, group_name, group_repo, matches,
    };

    #[test]
    fn repository_group_keys_name_the_same_repo_with_or_without_a_suffix() {
        assert_eq!(group_repo("octo/app"), "octo/app");
        assert_eq!(group_repo(&group_name(&record(), false)), "octo/app");
        assert_eq!(group_repo(&group_name(&record(), true)), "octo/app");
    }

    fn record() -> Record {
        Record {
            key: "octo/app#1".into(),
            repo: "octo/app".into(),
            title: "Build".into(),
            status: "failure".into(),
            active: false,
            link: String::new(),
            properties: Vec::new(),
            sections: Vec::new(),
            approval: None,
            run: Some(RunTarget {
                repo: "octo/app".into(),
                id: 1,
                workflow_id: 2,
                workflow: "CI".into(),
                path: String::new(),
                sha: String::new(),
                branch: "main".into(),
                actor: "octocat".into(),
                attempt: 1,
            }),
        }
    }

    #[test]
    fn filters_compose_without_losing_failed_runs() {
        let row = record();
        let mut filters = Filters {
            status: StatusFilter::Failed,
            branch: Some("main".into()),
            ..Filters::default()
        };
        assert!(matches(&row, &filters));
        filters.actor = Some("someone-else".into());
        assert!(!matches(&row, &filters));
        filters.actor = None;
        filters.status = StatusFilter::Active;
        assert!(!matches(&row, &filters));
        assert_eq!(group_name(&row, false), "octo/app · CI");
        assert_eq!(group_name(&row, true), "octo/app · failure");
    }

    #[test]
    fn grouping_alone_narrows_nothing_and_clearing_keeps_it() {
        let grouped = Filters {
            by_status: true,
            ..Filters::default()
        };
        assert!(!grouped.narrows());
        let filtered = Filters {
            status: StatusFilter::Failed,
            actor: Some("mona".into()),
            ..grouped.clone()
        };
        assert!(filtered.narrows());
        assert_eq!(filtered.cleared(), grouped);
        assert!(
            Filters {
                branch: Some("main".into()),
                ..Filters::default()
            }
            .narrows()
        );
        assert_eq!(
            StatusFilter::ALL.map(StatusFilter::adjective),
            ["", "active", "failed", "successful"]
        );
    }

    #[test]
    fn active_states_and_all_four_views_are_explicit() {
        assert!(active("waiting"));
        assert!(active("queued"));
        assert!(!active("completed"));
        assert_eq!(
            View::ALL.map(View::title),
            ["Runs", "Deployments", "Releases", "Flow"]
        );
        assert_eq!(
            StatusFilter::ALL.map(StatusFilter::title),
            ["All", "Active", "Failed", "Success"]
        );
    }
}
