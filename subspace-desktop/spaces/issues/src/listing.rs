//! What the list has read this session and why a read gave no list (spec
//! sections 3, 5 and 7). Pure state: the views draw it and the hub's
//! results change it.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use subspace_api_client::hub::ScopeGeneration;
use subspace_api_client::model::IssueListItem;

use crate::model::StateKind;

/// Activating the board reads the list again once its last read is older.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(60);

/// One state group of the list, from the grouped read: a workflow state of
/// one project, or a state type for All projects.
#[derive(Debug, Clone)]
pub(crate) struct IssueGroup {
    /// The group's position in the hub's list, which its pages name.
    pub(crate) index: usize,
    /// The workflow state's id, or the state type for All projects.
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) kind: StateKind,
    /// Subspace API's total for the group, when the read gives one.
    pub(crate) total: Option<u32>,
    pub(crate) rows: Vec<IssueListItem>,
    pub(crate) more: More,
}

/// The rows past the loaded ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum More {
    /// Every row is loaded.
    None,
    /// A cursor waits: the next page is read near the group's end.
    Available,
    /// A folded Done, Cancelled or Duplicate group, read when unfolded.
    Unread,
    Loading,
    /// *Couldn't load more*, with Retry.
    Failed,
}

/// Why a read gave no list. Only the board's own wording reaches the
/// screen; nothing from a response body does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    Edge,
    Forbidden,
    /// A NETT account with several organisations must choose one.
    Organisation,
    Unexpected {
        status: Option<u16>,
    },
    Network,
    Offline,
}

impl Failure {
    /// Only losing the connection keeps what this session already read,
    /// under the Offline banner; any other failure stands in for the list.
    pub(crate) fn keeps_content(&self) -> bool {
        *self == Failure::Offline
    }

    /// States about the account rather than the list hide the tabs and
    /// toolbar, which can't help.
    pub(crate) fn is_account_level(&self) -> bool {
        matches!(
            self,
            Failure::Edge | Failure::Forbidden | Failure::Organisation
        )
    }
}

/// What the list read, and when.
#[derive(Debug, Clone)]
pub(crate) struct Content {
    pub(crate) groups: Vec<IssueGroup>,
    pub(crate) read_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    Idle,
    Loading,
    Failed(Failure),
}

/// One grouped list as the hub read it: its scope and installation counter.
/// Its groups' pages leave it as it is; any replacement list, from a
/// Reload, a stale activation or a cursor's recovery, has another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListId {
    pub(crate) generation: ScopeGeneration,
    pub(crate) installation: u64,
}

/// The list's content and read status for the current scope.
#[derive(Debug, Clone)]
pub(crate) struct Listing {
    pub(crate) hidden_subissues_count: Option<u32>,
    pub(crate) my_issues_count: Option<u32>,
    pub(crate) content: Option<Content>,
    pub(crate) status: Status,
    /// Completed empty first pages in `paged_for`. Pending reads are separate:
    /// a refresh cancels them without ending their groups. State ids only.
    paged: HashSet<String>,
    pending: HashSet<String>,
    request_revision: u64,
    paged_for: Option<ListId>,
}

impl Default for Listing {
    fn default() -> Self {
        Listing {
            hidden_subissues_count: None,
            my_issues_count: None,
            content: None,
            status: Status::Idle,
            paged: HashSet::new(),
            pending: HashSet::new(),
            request_revision: 0,
            paged_for: None,
        }
    }
}

impl Listing {
    pub(crate) fn start_read(&mut self) {
        self.status = Status::Loading;
    }

    /// The hub's groups belong to `list`: a new list may read every first
    /// page again.
    pub(crate) fn follow_list(&mut self, list: Option<ListId>) {
        if self.paged_for != list {
            self.paged.clear();
            self.pending.clear();
            self.paged_for = list;
        }
    }

    pub(crate) fn follow_request(&mut self, revision: u64) {
        if self.request_revision != revision {
            self.pending.clear();
            self.request_revision = revision;
        }
    }

    pub(crate) fn loaded(&mut self, groups: Vec<IssueGroup>, now: Instant) {
        self.content = Some(Content {
            groups,
            read_at: now,
        });
        self.status = Status::Idle;
    }

    pub(crate) fn failed(&mut self, failure: Failure) {
        if !failure.keeps_content() {
            self.content = None;
        }
        self.status = Status::Failed(failure);
    }

    /// Forget rows and paging state when the connection or list changes.
    pub(crate) fn clear(&mut self) {
        self.drop_content();
        self.paged.clear();
        self.pending.clear();
        self.paged_for = None;
    }

    /// Drops what was read but remembers which first pages were asked
    /// for, as while the board is hidden.
    pub(crate) fn drop_content(&mut self) {
        self.hidden_subissues_count = None;
        self.my_issues_count = None;
        self.content = None;
        self.status = Status::Idle;
    }

    /// Records that the group's first page was asked for.
    pub(crate) fn asked_first_page(&mut self, group: &str) {
        self.pending.insert(group.to_owned());
    }

    /// A group with nothing read yet: a first page to ask for, unless one
    /// was asked for already and came back empty.
    pub(crate) fn settle_unread(&mut self, groups: &mut [IssueGroup]) {
        for group in groups {
            if group.more != More::Loading
                && self.pending.remove(&group.key)
                && group.more == More::Unread
            {
                self.paged.insert(group.key.clone());
            }
            if group.more == More::Unread && self.paged.contains(&group.key) {
                group.more = More::None;
            }
        }
    }

    /// Never read, or read more than a minute ago, and not reading now.
    pub(crate) fn is_stale(&self, now: Instant) -> bool {
        if self.status == Status::Loading {
            return false;
        }
        self.content
            .as_ref()
            .is_none_or(|content| now.saturating_duration_since(content.read_at) > STALE_AFTER)
    }

    /// The failure shown over kept content: the Offline banner.
    pub(crate) fn offline(&self) -> Option<&Failure> {
        match (&self.content, &self.status) {
            (Some(_), Status::Failed(failure)) => Some(failure),
            _ => None,
        }
    }

    /// Issues in the list: the groups' totals, or their rows without one.
    pub(crate) fn issue_count(&self) -> Option<u32> {
        let groups = &self.content.as_ref()?.groups;
        Some(
            groups
                .iter()
                .map(|group| group.total.unwrap_or(group.rows.len() as u32))
                .sum(),
        )
    }

    pub(crate) fn group_mut(&mut self, key: &str) -> Option<&mut IssueGroup> {
        self.content
            .as_mut()?
            .groups
            .iter_mut()
            .find(|group| group.key == key)
    }
}

/// The scoped search can also return text that mentions the identifier.
pub(crate) fn prefer_exact(groups: &mut Vec<IssueGroup>, query: &str) {
    if query.is_empty() {
        return;
    }
    let exact = groups.iter().enumerate().find_map(|(index, group)| {
        let row = group
            .rows
            .iter()
            .position(|item| item.fields.identifier.eq_ignore_ascii_case(query))?;
        Some((index, row))
    });
    if let Some((index, row)) = exact {
        let mut group = groups.swap_remove(index);
        group.rows = vec![group.rows.swap_remove(row)];
        group.total = Some(1);
        group.more = More::None;
        groups.clear();
        groups.push(group);
    }
}

const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;

/// *updated n min ago*, from the last read.
pub(crate) fn updated(read_at: Instant, now: Instant) -> String {
    let minutes = now.saturating_duration_since(read_at).as_secs() / SECONDS_PER_MINUTE;
    match minutes {
        0 => "updated just now".to_owned(),
        1..MINUTES_PER_HOUR => format!("updated {minutes} min ago"),
        _ => format!("updated {} h ago", minutes / MINUTES_PER_HOUR),
    }
}

#[cfg(test)]
#[path = "tests/listing.rs"]
pub(crate) mod tests;
