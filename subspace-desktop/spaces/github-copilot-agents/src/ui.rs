//! The words and ids the filters, the menus, the commands, the status bar and
//! the docks read as.

use crate::filters::Counts;
use crate::keys::Picking;
use crate::model::AgentRow;
use crate::navigator::ALL_VIEW;

/// The value that stands for "every one of them" in a filter menu.
pub(crate) const ALL: &str = "";

pub(crate) fn menu_title(picking: Picking) -> &'static str {
    picking_words(picking).0
}

pub(crate) fn button_id(picking: Picking) -> &'static str {
    picking_words(picking).1
}

pub(crate) fn shortcut(picking: Picking) -> &'static str {
    picking_words(picking).2
}

/// What a filter's button reads while the filter keeps everything: its
/// title. With the navigator hidden the kind filter's button is the view
/// menu, so it names the view on show instead.
pub(crate) fn unset_label(picking: Picking, navigator_open: bool) -> &'static str {
    match (picking, navigator_open) {
        (Picking::Kind, false) => ALL_VIEW,
        _ => menu_title(picking),
    }
}

/// A filter menu's title, button id and shortcut, in one place per menu.
fn picking_words(picking: Picking) -> (&'static str, &'static str, &'static str) {
    match picking {
        Picking::Kind => ("Kind", "github-copilot-agents-kind", "1"),
        Picking::Group => ("State", "github-copilot-agents-state", "2"),
        Picking::Repo => ("Repository", "github-copilot-agents-repo", "3"),
    }
}

/// What the monitor command reads for the row it would act on.
pub(crate) fn monitor_command(row: &AgentRow) -> &'static str {
    if row.monitor {
        "Stop monitoring this task"
    } else {
        "Monitor this task"
    }
}

pub(crate) fn monitor_message(title: &str, on: bool) -> String {
    let what = if title.trim().is_empty() {
        "the task"
    } else {
        title
    };
    if on {
        format!("Monitoring {what}: it will alert when it finishes or asks for an answer")
    } else {
        format!("No longer monitoring {what}")
    }
}

pub(crate) fn toggle_label(what: &str, on: bool) -> String {
    format!("{what}: {}", if on { "on" } else { "off" })
}

/// What is known of the GitHub account the tasks belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Account<'a> {
    /// GitHub named the work login.
    SignedIn(&'a str),
    /// The first load is still running.
    Checking,
    /// A load failed before GitHub named a login.
    Unavailable,
    /// Nothing has loaded yet.
    Unknown,
}

impl<'a> Account<'a> {
    /// The login once GitHub named it, else where the first load stands.
    pub(crate) fn of(login: Option<&'a str>, loading: bool, failed: bool) -> Self {
        match login {
            Some(login) => Self::SignedIn(login),
            None if loading => Self::Checking,
            None if failed => Self::Unavailable,
            None => Self::Unknown,
        }
    }

    /// The account as the status bar's context: the service, then the
    /// account, in the words the other GitHub Spaces use.
    pub(crate) fn scope(self) -> String {
        match self {
            Self::SignedIn(login) => format!("GitHub · Signed in as {login}"),
            Self::Checking => "GitHub · Checking account…".to_owned(),
            Self::Unavailable => "GitHub · Account unavailable".to_owned(),
            Self::Unknown => "GitHub".to_owned(),
        }
    }

    /// The account as the navigator's footer line.
    pub(crate) fn footer(self) -> String {
        match self {
            Self::SignedIn(login) => format!("Signed in to GitHub as {login}"),
            Self::Checking => "Checking the GitHub account…".to_owned(),
            Self::Unavailable => "GitHub account unavailable".to_owned(),
            Self::Unknown => "GitHub".to_owned(),
        }
    }
}

/// The status bar's line: the account `scope`, the counts, and how many
/// rows show while a filter hides some.
pub(crate) fn status_line(scope: &str, counts: Counts, shown: usize) -> String {
    let filtered = match shown == counts.total {
        true => String::new(),
        false => format!(" · {shown} shown"),
    };
    format!(
        "{scope} · {} rows · {} active · {} waiting for input{filtered}",
        counts.total, counts.active, counts.waiting
    )
}

/// What a dock stores while it shows, and while it is hidden.
const DOCK_SHOWN: &str = "1";
const DOCK_HIDDEN: &str = "0";

/// Whether a dock shows, from its stored value: it does unless it was hidden.
pub(crate) fn dock_shown(stored: Option<&str>) -> bool {
    stored != Some(DOCK_HIDDEN)
}

/// What a dock stores for being shown or hidden.
pub(crate) fn dock_value(shown: bool) -> &'static str {
    if shown { DOCK_SHOWN } else { DOCK_HIDDEN }
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{Subject, SubjectKind};

    use super::{
        Account, button_id, dock_shown, dock_value, menu_title, monitor_command, monitor_message,
        shortcut, status_line, toggle_label, unset_label,
    };
    use crate::filters::Counts;
    use crate::keys::Picking;
    use crate::model::{AgentRow, Kind};

    fn row(monitor: bool) -> AgentRow {
        AgentRow {
            key: "cloud:t1".into(),
            kind: Kind::Cloud,
            id: "t1".into(),
            title: "Fix the login button".into(),
            repo: "Owner/one".into(),
            state: "in_progress".into(),
            created_at: String::new(),
            updated_at: String::new(),
            monitor,
            subject: Some(Subject {
                kind: SubjectKind::Chat,
                id: "t1".into(),
            }),
            html_url: String::new(),
            pull_request: None,
        }
    }

    #[test]
    fn the_monitor_command_offers_the_change_not_the_state() {
        assert_eq!(monitor_command(&row(false)), "Monitor this task");
        assert_eq!(monitor_command(&row(true)), "Stop monitoring this task");
    }

    #[test]
    fn the_monitor_message_names_the_task_it_changed() {
        assert!(monitor_message("Fix it", true).starts_with("Monitoring Fix it"));
        assert_eq!(
            monitor_message("  ", false),
            "No longer monitoring the task"
        );
    }

    #[test]
    fn each_filter_has_its_own_title_button_and_key() {
        let all = [Picking::Kind, Picking::Group, Picking::Repo];
        let titles: Vec<&str> = all.iter().copied().map(menu_title).collect();
        let ids: Vec<&str> = all.iter().copied().map(button_id).collect();
        let keys: Vec<&str> = all.iter().copied().map(shortcut).collect();
        assert_eq!(titles, ["Kind", "State", "Repository"]);
        assert_eq!(
            ids,
            [
                "github-copilot-agents-kind",
                "github-copilot-agents-state",
                "github-copilot-agents-repo"
            ]
        );
        assert_eq!(keys, ["1", "2", "3"]);
    }

    #[test]
    fn the_kind_button_names_the_view_while_the_navigator_is_hidden() {
        assert_eq!(unset_label(Picking::Kind, true), "Kind");
        assert_eq!(unset_label(Picking::Kind, false), "All tasks and chats");
        for picking in [Picking::Group, Picking::Repo] {
            assert_eq!(unset_label(picking, false), menu_title(picking));
            assert_eq!(unset_label(picking, true), menu_title(picking));
        }
    }

    #[test]
    fn a_toggle_reads_its_current_setting() {
        assert_eq!(toggle_label("Alert sound", true), "Alert sound: on");
        assert_eq!(toggle_label("Alert sound", false), "Alert sound: off");
    }

    #[test]
    fn the_account_is_the_login_or_where_the_first_load_stands() {
        assert_eq!(
            Account::of(Some("octo-dev"), true, true),
            Account::SignedIn("octo-dev"),
            "a known login outlasts a later load and a later failure"
        );
        assert_eq!(Account::of(None, true, true), Account::Checking);
        assert_eq!(Account::of(None, false, true), Account::Unavailable);
        assert_eq!(Account::of(None, false, false), Account::Unknown);
    }

    #[test]
    fn the_account_reads_as_status_context_and_as_a_footer_line() {
        let known = Account::SignedIn("octo-dev");
        assert_eq!(known.scope(), "GitHub · Signed in as octo-dev");
        assert_eq!(known.footer(), "Signed in to GitHub as octo-dev");
        assert_eq!(Account::Checking.scope(), "GitHub · Checking account…");
        assert_eq!(Account::Checking.footer(), "Checking the GitHub account…");
        assert_eq!(Account::Unavailable.scope(), "GitHub · Account unavailable");
        assert_eq!(Account::Unavailable.footer(), "GitHub account unavailable");
        assert_eq!(Account::Unknown.scope(), "GitHub");
        assert_eq!(Account::Unknown.footer(), "GitHub");
    }

    #[test]
    fn the_status_line_leads_with_the_account_and_counts_what_a_filter_hides() {
        let counts = Counts {
            total: 12,
            active: 3,
            waiting: 1,
            watched_waiting: 0,
        };
        assert_eq!(
            status_line("GitHub · Signed in as octo-dev", counts, 12),
            "GitHub · Signed in as octo-dev · 12 rows · 3 active · 1 waiting for input"
        );
        assert_eq!(
            status_line("GitHub", counts, 5),
            "GitHub · 12 rows · 3 active · 1 waiting for input · 5 shown"
        );
    }

    #[test]
    fn a_dock_shows_unless_it_was_hidden() {
        assert!(dock_shown(None), "a dock shows before any choice is stored");
        assert!(dock_shown(Some(dock_value(true))));
        assert!(!dock_shown(Some(dock_value(false))));
        assert_eq!(dock_value(false), "0");
        assert!(dock_shown(Some("anything else")));
    }
}
