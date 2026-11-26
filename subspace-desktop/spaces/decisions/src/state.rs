//! What the Space holds between reads: the account and its directory, the
//! loaded decisions and how the last read went, and the words the status
//! bar and the list's states take from them. Pure.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use subspace_api_client::model::{DecisionListItem, Me, Project};
use subspace_framework::dashboard::list::LoadStatus;

use crate::load::Failure;
use crate::words::{self, People};

/// The signed-in account and what was read with it.
#[derive(Default)]
pub(crate) struct Session {
    /// The signed-in person's name, once the API answered.
    pub(crate) account: Option<Me>,
    /// Every project the account can see, archived ones included.
    pub(crate) projects: Arc<[Project]>,
    pub(crate) people: People,
    pub(crate) human_makers: Vec<(String, String)>,
    /// Every tag the reads have shown, for the Tag menu.
    pub(crate) tags: BTreeSet<String>,
}

impl Session {
    pub(crate) fn account_name(&self) -> Option<&str> {
        self.account.as_ref().map(|me| me.display_name.as_str())
    }
}

/// How the last read went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Status {
    #[default]
    Idle,
    Loading,
    Failed(Failure),
}

/// The loaded decisions.
#[derive(Default)]
pub(crate) struct Listing {
    /// None until a read lands, and again after an account-level failure.
    pub(crate) items: Option<Vec<DecisionListItem>>,
    /// The read stopped before the last page.
    pub(crate) more: bool,
    pub(crate) read_at: Option<Instant>,
    pub(crate) status: Status,
}

impl Listing {
    pub(crate) fn loading(&self) -> bool {
        self.status == Status::Loading
    }

    /// The failure that leaves nothing to show, which the Space draws in
    /// place of the list.
    pub(crate) fn blocked(&self) -> Option<Failure> {
        match (&self.items, self.status) {
            (None, Status::Failed(failure)) => Some(failure),
            _ => None,
        }
    }
}

/// The shared list's state: the skeleton until the first read lands, and a
/// banner over decisions kept from an earlier read when a read fails.
pub(crate) fn list_status(listing: &Listing) -> LoadStatus {
    match (&listing.items, listing.status) {
        (None, Status::Idle | Status::Loading) => LoadStatus::Loading,
        (Some(_), Status::Failed(failure)) => LoadStatus::Failed {
            title: failure.title().into(),
            message: failure.message().into(),
        },
        _ => LoadStatus::Ready,
    }
}

/// The status bar: the service, the account, then the count and when it
/// was read, or how the read went.
pub(crate) fn status_line(
    service: &str,
    account: Option<&str>,
    listing: &Listing,
    now: Instant,
) -> String {
    let mut parts = vec![service.to_owned()];
    parts.extend(account.map(|name| format!("Signed in as {name}")));
    parts.extend(listing_part(listing, now));
    parts.join(" · ")
}

fn listing_part(listing: &Listing, now: Instant) -> Option<String> {
    if let Status::Failed(failure) = listing.status {
        return Some(failure.title().to_owned());
    }
    match (&listing.items, listing.read_at) {
        (Some(items), Some(read_at)) => Some(format!(
            "{} · {}",
            words::decisions(items.len(), listing.more),
            words::updated(read_at, now)
        )),
        _ if listing.loading() => Some("Loading decisions…".to_owned()),
        _ => None,
    }
}

/// The toolbar's count, once decisions are loaded.
pub(crate) fn count_line(listing: &Listing) -> Option<String> {
    listing
        .items
        .as_ref()
        .map(|items| words::decisions(items.len(), listing.more))
}

/// The navigator's footer: who is signed in, or why no one is.
pub(crate) fn account_line(service: &str, account: Option<&str>, listing: &Listing) -> String {
    match (account, listing.status) {
        (Some(name), _) => format!("Signed in as {name}"),
        (None, Status::Failed(failure)) if failure.account_level() => failure.title().to_owned(),
        (None, _) => service.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use subspace_framework::dashboard::list::LoadStatus;

    use super::{Listing, Status, account_line, count_line, list_status, status_line};
    use crate::fixtures;
    use crate::load::Failure;

    fn loaded(read_at: Instant) -> Listing {
        Listing {
            items: Some(fixtures::items()),
            more: false,
            read_at: Some(read_at),
            status: Status::Idle,
        }
    }

    #[test]
    fn the_status_bar_names_the_service_account_count_and_read_time() {
        let read_at = Instant::now();
        let later = read_at + Duration::from_secs(5 * 60);
        assert_eq!(
            status_line(
                "Subspace API",
                Some("local-operator"),
                &loaded(read_at),
                later
            ),
            "Subspace API · Signed in as local-operator · 8 decisions · updated 5 min ago"
        );
        let loading = Listing {
            status: Status::Loading,
            ..Listing::default()
        };
        assert_eq!(
            status_line("Subspace API", None, &loading, later),
            "Subspace API · Loading decisions…"
        );
        let offline = Listing {
            status: Status::Failed(Failure::Network),
            ..loaded(read_at)
        };
        assert_eq!(
            status_line("Subspace API", Some("local-operator"), &offline, later),
            "Subspace API · Signed in as local-operator · Offline"
        );
        assert_eq!(
            status_line("Subspace API", None, &Listing::default(), later),
            "Subspace API"
        );
    }

    #[test]
    fn the_list_shows_a_skeleton_first_and_a_banner_over_kept_decisions() {
        assert_eq!(list_status(&Listing::default()), LoadStatus::Loading);
        assert_eq!(list_status(&loaded(Instant::now())), LoadStatus::Ready);
        let offline = Listing {
            status: Status::Failed(Failure::Network),
            ..loaded(Instant::now())
        };
        assert!(matches!(list_status(&offline), LoadStatus::Failed { .. }));
        assert!(
            offline.blocked().is_none(),
            "decisions stay under the banner"
        );
        let signed_out = Listing {
            status: Status::Failed(Failure::SignIn),
            ..Listing::default()
        };
        assert_eq!(list_status(&signed_out), LoadStatus::Ready);
        assert_eq!(signed_out.blocked(), Some(Failure::SignIn));
    }

    #[test]
    fn the_count_and_the_account_line_follow_the_read() {
        let mut listing = loaded(Instant::now());
        assert_eq!(count_line(&listing).as_deref(), Some("8 decisions"));
        listing.more = true;
        assert_eq!(count_line(&listing).as_deref(), Some("8+ decisions"));
        assert_eq!(count_line(&Listing::default()), None);
        assert_eq!(
            account_line("Subspace API", Some("local-operator"), &listing),
            "Signed in as local-operator"
        );
        let signed_out = Listing {
            status: Status::Failed(Failure::SignIn),
            ..Listing::default()
        };
        assert_eq!(
            account_line("Subspace API", None, &signed_out),
            "Sign-in required"
        );
        assert_eq!(
            account_line("Subspace API", None, &Listing::default()),
            "Subspace API"
        );
    }
}
