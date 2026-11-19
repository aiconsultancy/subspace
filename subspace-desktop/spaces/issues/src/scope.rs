//! Pure presentation of connection, loading and failed-read states.
use crate::listing::{Failure, Listing, Status, updated};
pub(crate) const SIGN_IN_MESSAGE: &str = "Sign in under Settings › Account, then try again.";
pub(crate) const NETWORK_MESSAGE: &str = "Couldn't reach Subspace API. Try again.";
use std::time::Instant;
use subspace_api_client::hub::LiveStatus;

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
    let (title, message) = match failure {
        Failure::Edge => ("Sign-in required", SIGN_IN_MESSAGE),
        Failure::Forbidden => (
            "Access denied",
            "Subspace API refused access. Try again after access is restored.",
        ),
        Failure::Unexpected { .. } => (
            "Couldn't load issues",
            "Subspace API couldn't be read. Try again, or copy the details.",
        ),
        Failure::Network | Failure::Offline => {
            ("Offline", "Couldn't reach Subspace API. Try again.")
        }
    };
    ScreenText {
        title: title.into(),
        message: message.into(),
        error: true,
    }
}
pub(crate) fn screen_actions(screen: &Screen) -> Vec<ScreenAction> {
    match screen {
        Screen::Failed(Failure::Edge | Failure::Forbidden) => vec![ScreenAction::Retry],
        _ => vec![ScreenAction::Retry, ScreenAction::CopyDetails],
    }
}
pub(crate) fn offline_text(read_at: Instant, now: Instant) -> (String, String) {
    (
        "Offline".into(),
        format!(
            "Couldn't reach Subspace API. Try again. Showing issues as last read, {}.",
            updated(read_at, now)
        ),
    )
}
pub(crate) fn scope_line(profile: &str, access: &Access) -> String {
    match access {
        Access::Ready { display_name } => format!("{profile} · Signed in as {display_name}"),
        Access::Failed(Failure::Edge) => format!("{profile} · Sign-in required"),
        Access::Failed(_) => format!("{profile} · Offline"),
        Access::Starting => profile.into(),
    }
}
/// The live indicator's words, or none before the board is signed in.
pub(crate) fn live_line(access: &Access, live: LiveStatus) -> Option<&'static str> {
    if !matches!(access, Access::Ready { .. }) {
        return None;
    }
    // qual:allow(dry, repeated_matches) reason: "LiveStatus has this one word mapping; rustqual groups it with ViewTab and PrState only because their four-arm matches share one shape. The match stays exhaustive so a new status cannot go unlabelled."
    Some(match live {
        LiveStatus::Connecting => "Connecting…",
        LiveStatus::Live => "Live",
        LiveStatus::Reconnecting => "Reconnecting…",
        LiveStatus::Polling => "Polling every 30 s",
    })
}

pub(crate) fn status_line(
    profile: &str,
    access: &Access,
    listing: &Listing,
    now: Instant,
) -> String {
    if listing.offline().is_some() {
        return format!("{profile} · Offline");
    }
    match (&listing.content, listing.issue_count()) {
        (Some(content), Some(count)) => format!(
            "{profile} · {count} {} · {}",
            if count == 1 { "issue" } else { "issues" },
            updated(content.read_at, now)
        ),
        _ if listing.status == Status::Loading => format!("{profile} · Loading issues…"),
        _ => scope_line(profile, access),
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
