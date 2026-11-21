//! What the lists say while records load and when the filters match nothing.
//! Pure: the board passes in what it knows, and render only looks the
//! result up.

use std::collections::{BTreeSet, HashMap};

use gpui_kit::SharedString;
use subspace_framework::dashboard::list::{Group, GroupNote};

use crate::model::{Filters, StatusFilter, View};

/// What the board knows about each repository's records.
pub(crate) struct Progress<'a> {
    /// Repositories whose records are still on their way.
    pub loading: &'a BTreeSet<String>,
    /// `owner/name: Source` keys read successfully at least once.
    pub loaded: &'a BTreeSet<String>,
}

/// The source a view lists, as `Snapshot` names it in its keys.
fn source(view: View) -> &'static str {
    match view {
        View::Runs | View::Flow => "Runs",
        View::Deployments => "Deployments",
        View::Releases => "Releases",
    }
}

/// The key that marks `repo`'s records for `view` as read.
pub(crate) fn loaded_key(repo: &str, view: View) -> String {
    format!("{repo}: {}", source(view))
}

/// The note while `view`'s records load.
fn loading_text(view: View) -> &'static str {
    match view {
        View::Runs | View::Flow => "Loading runs…",
        View::Deployments => "Loading deployments…",
        View::Releases => "Loading releases…",
    }
}

/// A repository's loaded runs that the filters leave empty, such as
/// "No failed CI runs on main by mona in the latest runs".
fn no_runs_text(filters: &Filters) -> String {
    if !filters.narrows() {
        return "No workflow runs".into();
    }
    let kind = [
        filters.status.adjective(),
        filters.workflow.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .filter(|word| !word.is_empty())
    .map(|word| format!("{word} "))
    .collect::<String>();
    let branch = filters
        .branch
        .as_ref()
        .map_or(String::new(), |branch| format!(" on {branch}"));
    let actor = filters
        .actor
        .as_ref()
        .map_or(String::new(), |actor| format!(" by {actor}"));
    format!("No {kind}runs{branch}{actor} in the latest runs")
}

/// The quiet line of a loaded repository with nothing to show in `view`.
fn empty_text(view: View, filters: &Filters) -> String {
    match view {
        View::Runs | View::Flow => no_runs_text(filters),
        View::Deployments => "No environments or pending reviews".into(),
        View::Releases => "No releases or tags".into(),
    }
}

/// Repository `repo`'s note: loading while its records are on their way,
/// empty once read with nothing to show, and none when it was never read
/// (the error banner names a failed read).
fn note(repo: &str, view: View, filters: &Filters, progress: &Progress) -> Option<GroupNote> {
    if progress.loading.contains(repo) {
        Some(GroupNote::Loading(loading_text(view).into()))
    } else if progress.loaded.contains(&loaded_key(repo, view)) {
        Some(GroupNote::Empty(empty_text(view, filters).into()))
    } else {
        None
    }
}

/// The note of each group with no rows. Such a group is a repository: the
/// list keeps one, named by the repository, when nothing of it matches.
pub(crate) fn group_notes(
    groups: &[Group],
    view: View,
    filters: &Filters,
    progress: &Progress,
) -> HashMap<SharedString, GroupNote> {
    groups
        .iter()
        .filter(|group| group.rows.is_empty())
        .filter_map(|group| {
            note(&group.key, view, filters, progress).map(|note| (group.key.clone(), note))
        })
        .collect()
}

/// Whether the filters hide every loaded run, so the board shows one empty
/// state with Clear filters instead of a quiet line under every group.
/// Never while a repository loads or a read failed: then the groups say
/// which repository is loading, and the banner what failed.
fn nothing_matches(
    groups: &[Group],
    view: View,
    filters: &Filters,
    progress: &Progress,
    failed: bool,
) -> bool {
    view == View::Runs
        && filters.narrows()
        && !failed
        && progress.loading.is_empty()
        && groups.iter().all(|group| group.rows.is_empty())
        && groups
            .iter()
            .any(|group| progress.loaded.contains(&loaded_key(&group.key, view)))
}

/// The groups the list shows: none when the filters hide every loaded run,
/// so the shared empty state can name them and offer Clear filters.
pub(crate) fn visible_groups(
    groups: Vec<Group>,
    view: View,
    filters: &Filters,
    progress: &Progress,
    failed: bool,
) -> Vec<Group> {
    if nothing_matches(&groups, view, filters, progress, failed) {
        Vec::new()
    } else {
        groups
    }
}

/// The active filters by name, such as "Status Failed · Branch main".
pub(crate) fn filter_summary(filters: &Filters) -> String {
    let status =
        (filters.status != StatusFilter::All).then(|| format!("Status {}", filters.status.title()));
    let named = [
        ("Branch", &filters.branch),
        ("Workflow", &filters.workflow),
        ("Actor", &filters.actor),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.as_ref().map(|value| format!("{label} {value}")));
    status
        .into_iter()
        .chain(named)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The board-level empty state's title and message, naming the filters.
pub(crate) fn no_matches(filters: &Filters) -> (String, String) {
    (
        "No matching runs".into(),
        format!(
            "No loaded run matches {}. Filters apply only to the latest runs that Subspace loaded.",
            filter_summary(filters)
        ),
    )
}

/// The status line's last part: the load in progress, else the age of the
/// last complete load.
pub(crate) fn freshness(loading: bool, age_seconds: Option<u64>) -> String {
    match (loading, age_seconds) {
        (true, _) => "Loading runs…".into(),
        (false, Some(seconds)) => format!("{seconds}s ago"),
        (false, None) => "not loaded".into(),
    }
}

/// What the board knows about the GitHub account it reads as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Account<'a> {
    SignedIn(&'a str),
    /// Discovery or the first load is still running.
    Checking,
    /// A read failed before any account was chosen.
    Unavailable,
    Unknown,
}

impl<'a> Account<'a> {
    /// A chosen login wins; without one, a running load is still checking
    /// and a failed read left the account unavailable.
    pub(crate) fn of(login: Option<&'a str>, loading: bool, failed: bool) -> Self {
        match login {
            Some(login) => Self::SignedIn(login),
            None if loading => Self::Checking,
            None if failed => Self::Unavailable,
            None => Self::Unknown,
        }
    }

    /// The status line's first part: the service, then the account.
    pub(crate) fn status(self) -> String {
        match self {
            Self::SignedIn(login) => format!("GitHub · {login}"),
            Self::Checking => "GitHub · Checking account…".into(),
            Self::Unavailable => "GitHub · Account unavailable".into(),
            Self::Unknown => "GitHub".into(),
        }
    }

    /// The navigator's footer.
    pub(crate) fn line(self) -> String {
        match self {
            Self::SignedIn(login) => format!("Signed in to GitHub as {login}"),
            Self::Checking => "Checking GitHub account…".into(),
            Self::Unavailable => "GitHub account unavailable".into(),
            Self::Unknown => "Not signed in to GitHub".into(),
        }
    }
}

/// What the shell's status line says about the board.
pub(crate) struct Status<'a> {
    pub account: Account<'a>,
    pub runs: usize,
    pub active: usize,
    /// The status filter's title, such as "Failed".
    pub filter: &'a str,
    /// A repository's run history was cut at the client's page limit.
    pub truncated: bool,
    /// See [`freshness`].
    pub freshness: String,
}

/// The status line: the account, the run counts, the status filter, a
/// notice when the run history is cut, then the freshness.
pub(crate) fn status_line(status: &Status) -> String {
    let notice = if status.truncated {
        " · run history truncated"
    } else {
        ""
    };
    format!(
        "{} · {} runs · {} active · {}{notice} · {}",
        status.account.status(),
        status.runs,
        status.active,
        status.filter,
        status.freshness
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use gpui_kit::SharedString;
    use subspace_framework::dashboard::list::{Group, GroupNote};
    use subspace_framework::dashboard::{Glyph, Tone};

    use super::{
        Account, Progress, Status, filter_summary, freshness, group_notes, loaded_key, no_matches,
        nothing_matches, status_line, visible_groups,
    };
    use crate::model::{Filters, StatusFilter, View};

    fn group(key: &str, rows: usize) -> Group {
        let row = subspace_framework::dashboard::list::Row {
            key: format!("{key}#row").into(),
            icon: Glyph::new("●", Tone::Green),
            id: None,
            title: "CI".into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        };
        Group {
            key: key.to_owned().into(),
            icon: Glyph::new("◇", Tone::Muted),
            name: key.to_owned().into(),
            count: None,
            rows: vec![row; rows],
        }
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn failed() -> Filters {
        Filters {
            status: StatusFilter::Failed,
            ..Filters::default()
        }
    }

    #[test]
    fn each_empty_repository_says_whether_it_is_loading_empty_or_unread() {
        let groups = [
            group("acme/web", 0),
            group("acme/api", 0),
            group("acme/docs", 0),
            group("acme/app · CI", 2),
        ];
        let loading = set(&["acme/web", "acme/app"]);
        let loaded = set(&["acme/api: Runs", "acme/app: Runs"]);
        let progress = Progress {
            loading: &loading,
            loaded: &loaded,
        };
        let notes = group_notes(&groups, View::Runs, &failed(), &progress);
        assert_eq!(
            notes[&SharedString::from("acme/web")],
            GroupNote::Loading("Loading runs…".into())
        );
        assert_eq!(
            notes[&SharedString::from("acme/api")],
            GroupNote::Empty("No failed runs in the latest runs".into())
        );
        assert!(
            !notes.contains_key(&SharedString::from("acme/docs")),
            "never read"
        );
        assert_eq!(notes.len(), 2, "a group with rows has no note");
        let releases = group_notes(&groups, View::Releases, &failed(), &progress);
        assert_eq!(
            releases[&SharedString::from("acme/web")],
            GroupNote::Loading("Loading releases…".into())
        );
        assert!(!releases.contains_key(&SharedString::from("acme/api")));
    }

    #[test]
    fn empty_notes_name_every_filter_in_one_phrase() {
        let loading = BTreeSet::new();
        let loaded = set(&["acme/web: Runs", "acme/web: Releases"]);
        let progress = Progress {
            loading: &loading,
            loaded: &loaded,
        };
        let text = |view, filters: &Filters| match group_notes(
            &[group("acme/web", 0)],
            view,
            filters,
            &progress,
        )
        .remove(&SharedString::from("acme/web"))
        {
            Some(GroupNote::Empty(text)) => text.to_string(),
            other => panic!("expected an empty note, got {other:?}"),
        };
        assert_eq!(text(View::Runs, &Filters::default()), "No workflow runs");
        let every = Filters {
            status: StatusFilter::Success,
            branch: Some("main".into()),
            workflow: Some("CI".into()),
            actor: Some("mona".into()),
            by_status: true,
        };
        assert_eq!(
            text(View::Runs, &every),
            "No successful CI runs on main by mona in the latest runs"
        );
        assert_eq!(
            text(
                View::Runs,
                &Filters {
                    actor: Some("hubot".into()),
                    ..Filters::default()
                }
            ),
            "No runs by hubot in the latest runs"
        );
        assert_eq!(text(View::Releases, &every), "No releases or tags");
        assert_eq!(
            filter_summary(&every),
            "Status Success · Branch main · Workflow CI · Actor mona"
        );
        assert_eq!(loaded_key("acme/web", View::Flow), "acme/web: Runs");
    }

    /// Whether the board-level empty state shows for `groups` in `view`.
    fn board_empty(groups: &[Group], view: View, filters: &Filters, state: [&[&str]; 2]) -> bool {
        let [loading, loaded] = state.map(set);
        let progress = Progress {
            loading: &loading,
            loaded: &loaded,
        };
        nothing_matches(groups, view, filters, &progress, false)
    }

    #[test]
    fn the_board_is_empty_only_when_loaded_runs_all_miss_the_filters() {
        let done: [&[&str]; 2] = [&[], &["acme/web: Runs"]];
        let empty = [group("acme/web", 0), group("acme/api", 0)];
        assert!(board_empty(&empty, View::Runs, &failed(), done));
        assert!(!board_empty(&empty, View::Runs, &Filters::default(), done));
        assert!(!board_empty(&empty, View::Releases, &failed(), done));
        let busy: [&[&str]; 2] = [&["acme/api"], &["acme/web: Runs"]];
        assert!(!board_empty(&empty, View::Runs, &failed(), busy));
        assert!(!board_empty(&empty, View::Runs, &failed(), [&[], &[]]));
        let shown = [group("acme/web · CI", 1), group("acme/api", 0)];
        assert!(!board_empty(&shown, View::Runs, &failed(), done));
        let [none, loaded] = done.map(set);
        let progress = Progress {
            loading: &none,
            loaded: &loaded,
        };
        assert!(
            !nothing_matches(&empty, View::Runs, &failed(), &progress, true),
            "a failed read keeps the groups and the banner"
        );
        let (title, message) = no_matches(&Filters {
            branch: Some("main".into()),
            ..failed()
        });
        assert_eq!(title, "No matching runs");
        assert!(message.starts_with("No loaded run matches Status Failed · Branch main."));
    }

    #[test]
    fn the_list_drops_its_groups_only_when_nothing_matches() {
        let none = BTreeSet::new();
        let loaded = set(&["acme/web: Runs"]);
        let progress = Progress {
            loading: &none,
            loaded: &loaded,
        };
        let empty = vec![group("acme/web", 0), group("acme/api", 0)];
        assert!(visible_groups(empty.clone(), View::Runs, &failed(), &progress, false).is_empty());
        let kept = visible_groups(empty.clone(), View::Runs, &failed(), &progress, true);
        assert_eq!(kept, empty, "a failed read keeps the groups");
        let unfiltered = visible_groups(
            empty.clone(),
            View::Runs,
            &Filters::default(),
            &progress,
            false,
        );
        assert_eq!(unfiltered, empty);
    }

    #[test]
    fn the_status_line_says_loading_until_the_load_ends() {
        assert_eq!(freshness(true, Some(4)), "Loading runs…");
        assert_eq!(freshness(true, None), "Loading runs…");
        assert_eq!(freshness(false, Some(4)), "4s ago");
        assert_eq!(freshness(false, None), "not loaded");
    }

    #[test]
    fn the_account_is_named_once_chosen_and_explained_until_then() {
        let signed_in = Account::of(Some("octo-dev"), true, true);
        assert_eq!(signed_in, Account::SignedIn("octo-dev"), "a login wins");
        assert_eq!(signed_in.status(), "GitHub · octo-dev");
        assert_eq!(signed_in.line(), "Signed in to GitHub as octo-dev");

        let checking = Account::of(None, true, true);
        assert_eq!(checking, Account::Checking, "a running load is checking");
        assert_eq!(checking.status(), "GitHub · Checking account…");
        assert_eq!(checking.line(), "Checking GitHub account…");

        let unavailable = Account::of(None, false, true);
        assert_eq!(unavailable.status(), "GitHub · Account unavailable");
        assert_eq!(unavailable.line(), "GitHub account unavailable");

        let unknown = Account::of(None, false, false);
        assert_eq!(unknown.status(), "GitHub");
        assert_eq!(unknown.line(), "Not signed in to GitHub");
    }

    #[test]
    fn the_status_line_carries_the_account_the_counts_and_the_freshness() {
        let mut status = Status {
            account: Account::SignedIn("octo-dev"),
            runs: 602,
            active: 3,
            filter: "All",
            truncated: false,
            freshness: freshness(false, Some(8)),
        };
        assert_eq!(
            status_line(&status),
            "GitHub · octo-dev · 602 runs · 3 active · All · 8s ago"
        );
        status.truncated = true;
        assert_eq!(
            status_line(&status),
            "GitHub · octo-dev · 602 runs · 3 active · All · run history truncated · 8s ago"
        );
        status.account = Account::Unknown;
        status.freshness = freshness(true, None);
        assert!(status_line(&status).starts_with("GitHub · 602 runs"));
        assert!(status_line(&status).ends_with("· Loading runs…"));
    }
}
