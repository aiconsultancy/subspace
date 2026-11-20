//! The one tracker backend a build talks to. The product chooses it once,
//! before any Space starts; every client the app builds afterwards uses it.
//! A NETT build never installs the Subspace API origin, so nothing in it can
//! reach Subspace API. Features NETT does not serve yet stay out of the
//! Spaces until NETT's capability list names them.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::Error;

/// Which service the Issues client talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Subspace API: projects, initiatives with fleets, chat uploads and the
    /// issue update stream.
    Subspace,
    /// NETT: issue teams inside a NETT organisation.
    Nett,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

/// Records the build's backend. A second, different choice is refused, so
/// one process can never hold both.
pub(crate) fn claim(backend: Backend) -> Result<(), Error> {
    if *BACKEND.get_or_init(|| backend) == backend {
        Ok(())
    } else {
        Err(Error::InvalidRequest)
    }
}

/// The configured backend. Before the product chooses one, as in tests and
/// tools that build a loopback client, it is Subspace API.
pub fn backend() -> Backend {
    BACKEND.get().copied().unwrap_or(Backend::Subspace)
}

/// The service's name in messages and the Issues header.
pub fn service_name() -> &'static str {
    match backend() {
        Backend::Subspace => "Subspace API",
        Backend::Nett => "NETT",
    }
}

/// Features a backend may or may not serve. Subspace API serves them all.
/// NETT is adding most of them, and says which it serves for the selected
/// organisation; until it does, a NETT build leaves them out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Feature {
    /// Initiatives with IRC channels, stakeholders and a fleet.
    InitiativeChannels,
    /// Temporary chat uploads for pastes and files in Channels.
    ChatUploads,
    /// Standalone issue attachments, and uploading comment images.
    Attachments,
    /// Moving an issue to another project with a preview.
    IssueMoves,
    /// Repository mapping, the reverse lookup and deployment statuses. The
    /// desktop does not use NETT's yet.
    Repositories,
    /// The issue update stream.
    Updates,
}

impl Feature {
    const ALL: [Self; 6] = [
        Self::InitiativeChannels,
        Self::ChatUploads,
        Self::Attachments,
        Self::IssueMoves,
        Self::Repositories,
        Self::Updates,
    ];

    /// The name NETT uses for the feature in its capability list.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::InitiativeChannels => "initiativeChannels",
            Self::ChatUploads => "chatUploads",
            Self::Attachments => "attachments",
            Self::IssueMoves => "issueMoves",
            Self::Repositories => "repositories",
            Self::Updates => "updates",
        }
    }

    fn bit(self) -> u8 {
        1 << Self::ALL
            .iter()
            .position(|feature| *feature == self)
            .unwrap_or(0)
    }
}

/// An API origin, organisation and account: NETT's capability list may
/// differ from one person to the next in the same organisation.
type Scope = (String, String, u64);

/// How long a capability list read from NETT is used before it is read
/// again: NETT can enable a feature for an organisation while the app runs.
pub(crate) const SERVED_FOR: Duration = Duration::from_secs(5 * 60);
/// How long a list that could not be read again keeps answering before the
/// next try, doubling with each failure in a row up to [`SERVED_FOR`].
const SERVED_RETRY: Duration = Duration::from_secs(30);

/// One organisation and account's capability list, when to read it again,
/// and how many times it changed since it was first read. A list due to be
/// read again still answers until it is.
#[derive(Clone, Copy)]
struct Served {
    bits: u8,
    next: Instant,
    revision: u64,
    /// Reads in a row that failed since it was last read.
    failures: u32,
}

/// The features NETT says it serves, per API origin, organisation and
/// account. Another account never sees what was read for the previous one.
static SERVED: Mutex<BTreeMap<Scope, Served>> = Mutex::new(BTreeMap::new());

fn served_table() -> std::sync::MutexGuard<'static, BTreeMap<Scope, Served>> {
    SERVED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn scope(origin: &str, org: &str, account: u64) -> Scope {
    (origin.to_owned(), org.to_owned(), account)
}

/// Records what NETT serves for one organisation and account, used until
/// [`SERVED_FOR`] has passed. Returns whether it differs from the list
/// known before, if there was one; a list that differs is a new revision,
/// whoever read it.
pub(crate) fn record_served(origin: &str, org: &str, account: u64, features: &[Feature]) -> bool {
    let bits = features
        .iter()
        .fold(0, |bits, feature| bits | feature.bit());
    let next = Instant::now() + SERVED_FOR;
    let mut table = served_table();
    let key = scope(origin, org, account);
    let before = table.get(&key).copied();
    let changed = before.is_some_and(|before| before.bits != bits);
    let revision = before.map_or(0, |before| before.revision.wrapping_add(u64::from(changed)));
    table.insert(
        key,
        Served {
            bits,
            next,
            revision,
            failures: 0,
        },
    );
    changed
}

/// Which revision of the capability list this organisation and account
/// holds, `None` before one was read. It moves whenever a read finds the
/// list changed, so a waiting reader notices a change another read found.
pub(crate) fn served_revision(origin: &str, org: &str, account: u64) -> Option<u64> {
    served_table()
        .get(&scope(origin, org, account))
        .map(|served| served.revision)
}

/// Whether the capability list for this organisation and account was read
/// and is not yet due to be read again.
pub(crate) fn served_current(origin: &str, org: &str, account: u64) -> bool {
    served_current_at(origin, org, account, Instant::now())
}

pub(crate) fn served_current_at(origin: &str, org: &str, account: u64, now: Instant) -> bool {
    served_table()
        .get(&scope(origin, org, account))
        .is_some_and(|served| now < served.next)
}

/// Whether a capability list was ever read for this organisation and
/// account, however old it is.
pub(crate) fn served_known(origin: &str, org: &str, account: u64) -> bool {
    served_table().contains_key(&scope(origin, org, account))
}

/// Asks for the capability list to be read again the next time a feature
/// is checked, as an explicit reload does. Until then it still answers.
pub(crate) fn reread_served(origin: &str, org: &str, account: u64) {
    if let Some(served) = served_table().get_mut(&scope(origin, org, account)) {
        served.next = Instant::now();
    }
}

/// A list that could not be read again keeps answering for a while, so a
/// failing read is not repeated for every feature check: 30 seconds after
/// the first failure, twice as long after each failure in a row, and at
/// most [`SERVED_FOR`].
pub(crate) fn served_unreadable(origin: &str, org: &str, account: u64) {
    if let Some(served) = served_table().get_mut(&scope(origin, org, account)) {
        let wait = SERVED_RETRY
            .checked_mul(2u32.saturating_pow(served.failures))
            .map_or(SERVED_FOR, |wait| wait.min(SERVED_FOR));
        served.failures = served.failures.saturating_add(1);
        served.next = Instant::now() + wait;
    }
}

/// Whether NETT serves `feature` for this organisation and account.
pub(crate) fn nett_serves(origin: &str, org: Option<&str>, account: u64, feature: Feature) -> bool {
    org.is_some_and(|org| {
        served_table()
            .get(&scope(origin, org, account))
            .is_some_and(|served| served.bits & feature.bit() != 0)
    })
}

/// What the configured NETT origin serves for the selected organisation.
fn selected_nett_serves(feature: Feature) -> bool {
    let Some(origin) = crate::native::origin() else {
        return false;
    };
    let account = crate::native::account_generation();
    let org = crate::nett::scope::selected(account);
    nett_serves(
        origin.as_str(),
        org.as_deref(),
        account.unwrap_or_default(),
        feature,
    )
}

/// What the Spaces show for the configured backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// Initiatives carry IRC channels and fleets, so Channels can offer its
    /// initiative picker and switch network for an initiative.
    pub initiative_channels: bool,
    /// Temporary chat uploads for pastes and files in Channels.
    pub chat_uploads: bool,
    /// Merge and deployment notices for captains in Channels.
    pub captain_notices: bool,
    /// Moving an issue to another project with a preview.
    pub issue_moves: bool,
    /// One-line issue summaries, required when creating an issue. NETT has
    /// no summaries and is not adding them.
    pub issue_summaries: bool,
    /// An issue owner separate from the assignee, which NETT does not have.
    pub issue_owners: bool,
    /// Adding, saving and removing standalone attachments on an issue,
    /// which NETT serves per organisation. Comment images and files need no
    /// capability on either backend.
    pub standalone_attachments: bool,
    /// Changes made elsewhere arrive over the issue update stream.
    pub live_updates: bool,
}

impl Capabilities {
    /// What `backend` offers: everything for Subspace API, and for NETT what
    /// its capability list names for the selected organisation.
    pub fn of(backend: Backend) -> Self {
        Self::with(backend, |feature| match backend {
            Backend::Subspace => true,
            Backend::Nett => selected_nett_serves(feature),
        })
    }

    pub(crate) fn with(backend: Backend, serves: impl Fn(Feature) -> bool) -> Self {
        let subspace = matches!(backend, Backend::Subspace);
        Self {
            initiative_channels: serves(Feature::InitiativeChannels),
            chat_uploads: serves(Feature::ChatUploads),
            // Notices route by an agent assignee's ship; NETT assignees are
            // people, and an agent delegate on issues is out of scope.
            captain_notices: subspace && serves(Feature::InitiativeChannels),
            issue_moves: serves(Feature::IssueMoves),
            issue_summaries: subspace,
            issue_owners: subspace,
            standalone_attachments: !subspace && serves(Feature::Attachments),
            live_updates: serves(Feature::Updates),
        }
    }
}

/// What the configured backend offers.
pub fn capabilities() -> Capabilities {
    Capabilities::of(backend())
}

/// The features named in NETT's capability list.
pub(crate) fn parse_features(names: &[String]) -> Vec<Feature> {
    Feature::ALL
        .into_iter()
        .filter(|feature| names.iter().any(|name| name == feature.name()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Backend, Capabilities, Feature, nett_serves, parse_features, record_served};

    #[test]
    fn nett_offers_only_what_it_says_it_serves_for_each_organisation() {
        let subspace = Capabilities::of(Backend::Subspace);
        assert!(subspace.chat_uploads && subspace.initiative_channels && subspace.issue_summaries);
        let origin = "http://127.0.0.1:1/api";
        let served = parse_features(&[
            "chatUploads".into(),
            "initiativeChannels".into(),
            "other".into(),
            // Named by NETT, but nothing the desktop offers.
            "issueSubscribers".into(),
            "notificationUnread".into(),
            "issueDocumentArchive".into(),
            "chatUploads".into(),
        ]);
        assert_eq!(served, [Feature::InitiativeChannels, Feature::ChatUploads]);
        record_served(origin, "org-a", 1, &served);
        record_served(origin, "org-b", 1, &[]);
        let scope = |org| move |feature| nett_serves(origin, Some(org), 1, feature);
        let a = Capabilities::with(Backend::Nett, scope("org-a"));
        assert!(a.chat_uploads && a.initiative_channels && !a.captain_notices);
        assert!(!a.issue_moves && !a.issue_summaries && !a.issue_owners && !a.live_updates);
        assert!(!a.standalone_attachments && !subspace.standalone_attachments);
        record_served(
            origin,
            "org-c",
            1,
            &parse_features(&["updates".into(), "attachments".into()]),
        );
        let c = Capabilities::with(Backend::Nett, scope("org-c"));
        assert!(c.live_updates && c.standalone_attachments);
        let b = Capabilities::with(Backend::Nett, scope("org-b"));
        assert!(!b.chat_uploads && !b.initiative_channels);
        assert!(!nett_serves(origin, None, 1, Feature::ChatUploads));
    }

    #[test]
    fn a_list_that_changes_is_a_new_revision_and_one_read_again_unchanged_is_not() {
        let origin = "http://127.0.0.1:3/api";
        assert_eq!(super::served_revision(origin, "org-a", 1), None);
        assert!(!record_served(origin, "org-a", 1, &[]));
        assert_eq!(super::served_revision(origin, "org-a", 1), Some(0));
        assert!(!record_served(origin, "org-a", 1, &[]));
        assert_eq!(super::served_revision(origin, "org-a", 1), Some(0));
        assert!(record_served(origin, "org-a", 1, &[Feature::Updates]));
        assert_eq!(super::served_revision(origin, "org-a", 1), Some(1));
        // Another account's list moves on its own.
        assert_eq!(super::served_revision(origin, "org-a", 2), None);
    }

    #[test]
    fn a_list_that_keeps_failing_is_tried_again_less_and_less_often() {
        use super::{SERVED_FOR, served_current_at, served_unreadable};
        use std::time::{Duration, Instant};
        let (origin, org) = ("http://127.0.0.1:4/api", "org-a");
        record_served(origin, org, 1, &[Feature::Updates]);
        let mut waits = Vec::new();
        for _ in 0..6 {
            let before = Instant::now();
            served_unreadable(origin, org, 1);
            // How long the list answers before the next try.
            let wait = [30, 60, 120, 240, 300]
                .into_iter()
                .map(Duration::from_secs)
                .find(|wait| {
                    !served_current_at(origin, org, 1, before + *wait + Duration::from_secs(1))
                })
                .unwrap_or(SERVED_FOR);
            waits.push(wait.as_secs());
        }
        assert_eq!(
            waits,
            [30, 60, 120, 240, 300, 300],
            "doubling, at most five minutes"
        );
        // A list read again starts over.
        record_served(origin, org, 1, &[Feature::Updates]);
        let before = Instant::now();
        served_unreadable(origin, org, 1);
        assert!(!served_current_at(
            origin,
            org,
            1,
            before + Duration::from_secs(31)
        ));
    }

    #[test]
    fn what_one_account_was_served_is_not_another_accounts() {
        let origin = "http://127.0.0.1:2/api";
        record_served(
            origin,
            "org-a",
            1,
            &[Feature::Attachments, Feature::Updates],
        );
        assert!(nett_serves(origin, Some("org-a"), 1, Feature::Attachments));
        // The next person in the same organisation has their own list.
        assert!(!super::served_known(origin, "org-a", 2));
        assert!(!nett_serves(origin, Some("org-a"), 2, Feature::Attachments));
        assert!(!nett_serves(origin, Some("org-a"), 2, Feature::Updates));
    }
}
