//! Pure presentation of connection, loading and failed-read states.
use crate::listing::{Failure, Listing, Status, updated};
pub(crate) const SIGN_IN_MESSAGE: &str = "Sign in under Settings › Account, then try again.";
/// The words for a list that couldn't be reached, naming the service.
pub(crate) fn network_message() -> String {
    subspace_api_client::unreachable_message()
}
/// The words for a read that failed, naming the service.
pub(crate) fn read_error_message() -> String {
    format!("{} couldn't be read.", subspace_api_client::service_name())
}
use std::time::Instant;
use subspace_api_client::hub::LiveStatus;
use subspace_api_client::nett;

/// Why NETT has no organisation to work in: a choice still to make, which
/// the Organisation menu offers once there is more than one, or none the
/// account can use, where that menu never appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoOrganisation {
    Choose,
    NoneEligible,
}
impl NoOrganisation {
    pub(crate) fn current() -> Self {
        Self::from_status(&nett::snapshot().status)
    }
    pub(crate) fn from_status(status: &nett::Status) -> Self {
        if *status == nett::Status::None {
            Self::NoneEligible
        } else {
            Self::Choose
        }
    }
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Choose => "Choose an organisation",
            Self::NoneEligible => "No organisation available",
        }
    }
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Choose => "Choose the organisation to work in from the Organisation menu.",
            Self::NoneEligible => {
                "This account has no NETT organisation with issue tracking. Ask an administrator to add you to one, then try again."
            }
        }
    }
    /// The connection line in Settings.
    pub(crate) fn settings_line(self) -> &'static str {
        match self {
            Self::Choose => "Choose an organisation in Issues",
            Self::NoneEligible => "No organisation available",
        }
    }
    pub(crate) fn actions(self) -> Vec<ScreenAction> {
        match self {
            Self::Choose => Vec::new(),
            Self::NoneEligible => vec![ScreenAction::Retry],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Access {
    Starting,
    Ready { display_name: String },
    Failed(Failure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Screen {
    Failed(Failure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Body {
    Loading { controls: bool },
    List,
    Screen { screen: Screen, controls: bool },
}
impl Body {
    pub(crate) fn controls(&self) -> bool {
        match self {
            Self::Loading { controls } | Self::Screen { controls, .. } => *controls,
            Self::List => true,
        }
    }
}

pub(crate) fn body(access: &Access, listing: &Listing) -> Body {
    match access {
        Access::Starting => Body::Loading { controls: false },
        Access::Failed(failure) => Body::Screen {
            screen: Screen::Failed(failure.clone()),
            controls: false,
        },
        Access::Ready { .. } => list_body(listing),
    }
}
fn list_body(listing: &Listing) -> Body {
    match (&listing.content, &listing.status) {
        (Some(_), _) => Body::List,
        (None, Status::Failed(failure)) => Body::Screen {
            controls: !failure.is_account_level(),
            screen: Screen::Failed(failure.clone()),
        },
        (None, _) => Body::Loading { controls: true },
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenAction {
    Retry,
    CopyDetails,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScreenText {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) error: bool,
}
pub(crate) fn screen_text(screen: &Screen, _profile: &str) -> ScreenText {
    let Screen::Failed(failure) = screen;
    let service = subspace_api_client::service_name();
    let (title, message) = match failure {
        Failure::Edge => ("Sign-in required", SIGN_IN_MESSAGE.to_owned()),
        Failure::Forbidden => (
            "Access denied",
            format!("{service} refused access. Try again after access is restored."),
        ),
        Failure::Organisation => {
            let state = NoOrganisation::current();
            (state.title(), state.message().to_owned())
        }
        Failure::Unexpected { .. } => (
            "Couldn't load issues",
            format!("{} Try again, or copy the details.", read_error_message()),
        ),
        Failure::Network | Failure::Offline => ("Offline", network_message()),
    };
    ScreenText {
        title: title.into(),
        message,
        error: !matches!(failure, Failure::Organisation),
    }
}
pub(crate) fn screen_actions(screen: &Screen) -> Vec<ScreenAction> {
    match screen {
        Screen::Failed(Failure::Organisation) => NoOrganisation::current().actions(),
        Screen::Failed(Failure::Edge | Failure::Forbidden) => vec![ScreenAction::Retry],
        _ => vec![ScreenAction::Retry, ScreenAction::CopyDetails],
    }
}
pub(crate) fn offline_text(read_at: Instant, now: Instant) -> (String, String) {
    (
        "Offline".into(),
        format!(
            "Couldn't reach {}. Try again. Showing issues as last read, {}.",
            subspace_api_client::service_name(),
            updated(read_at, now)
        ),
    )
}
pub(crate) fn scope_line(profile: &str, access: &Access) -> String {
    match access {
        Access::Ready { display_name } => format!("{profile} · Signed in as {display_name}"),
        Access::Failed(Failure::Edge) => format!("{profile} · Sign-in required"),
        Access::Failed(Failure::Organisation) => {
            format!("{profile} · {}", NoOrganisation::current().title())
        }
        Access::Failed(_) => format!("{profile} · Offline"),
        Access::Starting => profile.into(),
    }
}
/// How changes made elsewhere arrive, or nothing before the board is
/// signed in.
pub(crate) fn live_line(access: &Access, live: LiveStatus) -> Option<&'static str> {
    if !matches!(access, Access::Ready { .. }) {
        return None;
    }
    Some(match live {
        LiveStatus::Connecting => "Connecting…",
        LiveStatus::Live => "Live",
        LiveStatus::Reconnecting => "Reconnecting…",
        LiveStatus::Polling => "Polling every 30 s",
        LiveStatus::Off => "Live updates off: reload to refresh",
    })
}

/// The connection the status bar leads with: the service and account, then
/// how changes made elsewhere arrive, such as
/// *Subspace API · Signed in as Ada Fixture · Live*.
pub(crate) fn context_line(profile: &str, access: &Access, live: LiveStatus) -> String {
    let scope = scope_line(profile, access);
    match live_line(access, live) {
        Some(live) => format!("{scope} · {live}"),
        None => scope,
    }
}

/// The list's status bar line: the connection, then the count and when the
/// list was read. Offline, the account and *Offline* stand alone.
pub(crate) fn status_line(
    profile: &str,
    access: &Access,
    live: LiveStatus,
    listing: &Listing,
    now: Instant,
) -> String {
    if listing.offline().is_some() {
        return match access {
            Access::Failed(_) => scope_line(profile, access),
            _ => format!("{} · Offline", scope_line(profile, access)),
        };
    }
    let context = context_line(profile, access, live);
    match (&listing.content, listing.issue_count()) {
        (Some(content), Some(count)) => format!(
            "{context} · {count} {} · {}",
            if count == 1 { "issue" } else { "issues" },
            updated(content.read_at, now)
        ),
        _ if listing.status == Status::Loading => format!("{context} · Loading issues…"),
        _ => context,
    }
}
/// The list's status bar line with how many sub-issues it hides, when it
/// hides some, as the Sub-issues switch also says.
pub(crate) fn with_hidden(line: String, hidden: Option<u32>) -> String {
    match hidden {
        Some(count) => format!("{line} · {count} sub-issues hidden"),
        None => line,
    }
}

pub(crate) fn list_status(
    listing: &Listing,
    now: Instant,
) -> subspace_framework::dashboard::list::LoadStatus {
    use subspace_framework::dashboard::list::LoadStatus;
    let (Some(content), Some(_)) = (&listing.content, listing.offline()) else {
        return LoadStatus::Ready;
    };
    let (title, message) = offline_text(content.read_at, now);
    LoadStatus::Failed {
        title: title.into(),
        message: message.into(),
    }
}
#[cfg(test)]
#[path = "tests/scope.rs"]
mod tests;
