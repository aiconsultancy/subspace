//! The NETT organisation the app works in. Every tracker request names it in
//! its path and in `x-org-id`. Choosing another organisation advances the
//! scope generation, which retires every client, hub and read made for the
//! previous one, exactly as an account change does.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// One organisation from NETT's auth context.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Organisation {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_personal_org: bool,
}

impl Organisation {
    /// Personal organisations cannot use the issue tracker.
    pub fn eligible(&self) -> bool {
        !self.is_personal_org && uuid::Uuid::parse_str(&self.id).is_ok()
    }
}

/// Where the organisation choice stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Status {
    /// The auth context has not been read for this account yet.
    #[default]
    Unknown,
    /// An organisation is selected.
    Selected,
    /// Several organisations are eligible and none is saved.
    Choose,
    /// The account has no business organisation.
    None,
}

#[derive(Debug, Clone, Default)]
struct State {
    /// The account generation the list was read under.
    account: Option<u64>,
    user: Option<String>,
    organisations: Vec<Organisation>,
    selected: Option<String>,
    status: Status,
    generation: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    account: None,
    user: None,
    organisations: Vec::new(),
    selected: None,
    status: Status::Unknown,
    generation: 0,
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Advances whenever the selected organisation changes.
pub(crate) fn generation() -> u64 {
    lock().generation
}

/// The selected organisation, when there is one for this account.
pub(crate) fn selected(account: Option<u64>) -> Option<String> {
    let state = lock();
    (state.account == account)
        .then(|| state.selected.clone())
        .flatten()
}

/// What the organisation menu shows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Snapshot {
    pub organisations: Vec<Organisation>,
    pub selected: Option<String>,
    pub status: Status,
    pub generation: u64,
}

impl Snapshot {
    /// A snapshot for tests and previews of the organisation menu.
    pub fn new(
        organisations: Vec<Organisation>,
        selected: Option<String>,
        status: Status,
        generation: u64,
    ) -> Self {
        Self {
            organisations,
            selected,
            status,
            generation,
        }
    }
}

/// What the organisation menu shows for the signed-in account. Nothing read
/// for another account is shown, even while the new account's auth context
/// is still being read or cannot be read.
pub fn snapshot() -> Snapshot {
    // Read before locking: noticing an account change clears this state.
    let current = crate::native::account_generation();
    let state = lock();
    if state.account != current {
        return Snapshot {
            organisations: Vec::new(),
            selected: None,
            status: Status::Unknown,
            generation: state.generation,
        };
    }
    Snapshot {
        organisations: state
            .organisations
            .iter()
            .filter(|org| org.eligible())
            .cloned()
            .collect(),
        selected: state.selected.clone(),
        status: state.status.clone(),
        generation: state.generation,
    }
}

/// Records what NETT's auth context says for the account it was read
/// under, `account`, and resolves the selection: the saved organisation if
/// it is still accessible, otherwise the only eligible one. A reply read
/// under an account that is no longer signed in is ignored. Returns whether
/// the selection changed.
pub(crate) fn observe(
    account: Option<u64>,
    user: &str,
    organisations: Vec<Organisation>,
    saved: Option<String>,
) -> bool {
    let current = crate::native::account_generation();
    let mut state = lock();
    if account != current {
        return false;
    }
    let same_account = state.account == account && state.user.as_deref() == Some(user);
    let previous = state.selected.clone().filter(|_| same_account);
    let (selected, status) = resolve(&organisations, previous.or(saved));
    let changed = selected != state.selected || !same_account;
    state.account = account;
    state.user = Some(user.to_owned());
    state.organisations = organisations;
    state.status = status;
    if changed {
        state.selected = selected;
        state.generation = state.generation.wrapping_add(1);
    }
    changed
}

/// Chooses an organisation the person picked. Returns false when it is not
/// one of the account's eligible organisations.
pub fn select(id: &str) -> bool {
    let current = crate::native::account_generation();
    let mut state = lock();
    if state.account != current {
        return false;
    }
    // The organisation's own spelling of its ID, whatever the caller sent.
    let Some(id) = state
        .organisations
        .iter()
        .find(|org| org.eligible() && org.id.eq_ignore_ascii_case(id))
        .map(|org| org.id.clone())
    else {
        return false;
    };
    if state.selected.as_deref() != Some(id.as_str()) {
        state.selected = Some(id);
        state.status = Status::Selected;
        state.generation = state.generation.wrapping_add(1);
    }
    true
}

/// Forgets everything read for the previous account when the account
/// changes: its organisations, its selection and its private caches.
pub(crate) fn forget_account() {
    let mut state = lock();
    let had_selection = state.selected.is_some();
    state.account = None;
    state.user = None;
    state.organisations.clear();
    state.selected = None;
    state.status = Status::Unknown;
    if had_selection {
        state.generation = state.generation.wrapping_add(1);
    }
    drop(state);
    INITIATIVES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// The account's user, for keying the saved choice.
pub(crate) fn user() -> Option<String> {
    lock().user.clone()
}

/// The saved organisation if it is still eligible, else the only eligible
/// one, else none, with the status that leaves.
pub(crate) fn resolve(
    organisations: &[Organisation],
    preferred: Option<String>,
) -> (Option<String>, Status) {
    let eligible: Vec<_> = organisations.iter().filter(|org| org.eligible()).collect();
    if let Some(id) = preferred
        && let Some(org) = eligible.iter().find(|org| org.id.eq_ignore_ascii_case(&id))
    {
        return (Some(org.id.clone()), Status::Selected);
    }
    match eligible.as_slice() {
        [] => (None, Status::None),
        [only] => (Some(only.id.clone()), Status::Selected),
        _ => (None, Status::Choose),
    }
}

/// API origin, account, organisation and initiative.
pub(crate) type InitiativeKey = (String, u64, String, String);

/// Planning projects per account, organisation and initiative, briefly
/// cached so a list read and its group continuations agree without
/// re-reading them. What one person can see is never served to another.
static INITIATIVES: Mutex<BTreeMap<InitiativeKey, (std::time::Instant, Vec<String>)>> =
    Mutex::new(BTreeMap::new());
const INITIATIVE_PROJECTS_FOR: std::time::Duration = std::time::Duration::from_secs(60);

pub(crate) fn initiative_projects(key: &InitiativeKey) -> Option<Vec<String>> {
    let cache = INITIATIVES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .get(key)
        .filter(|(read, _)| read.elapsed() < INITIATIVE_PROJECTS_FOR)
        .map(|(_, projects)| projects.clone())
}

pub(crate) fn remember_initiative_projects(key: InitiativeKey, projects: Vec<String>) {
    INITIATIVES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, (std::time::Instant::now(), projects));
}

/// Saved choices, one per NETT API origin and user, in a small file beside
/// the app's state.
pub(crate) mod saved {
    use super::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    const FILE: &str = "nett-organisations.json";
    /// Off until the product turns it on, so tools and tests never touch
    /// the person's state directory.
    static ENABLED: AtomicBool = AtomicBool::new(false);

    pub(crate) fn enable() {
        ENABLED.store(true, Ordering::SeqCst);
    }

    fn key(origin: &str, user: &str) -> String {
        format!("{origin}|{user}")
    }

    fn path() -> Option<std::path::PathBuf> {
        if cfg!(test) || !ENABLED.load(Ordering::SeqCst) {
            return None;
        }
        subspace_framework::core::state::sibling(FILE)
    }

    fn read(path: &std::path::Path) -> BTreeMap<String, String> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub(crate) fn load(origin: &str, user: &str) -> Option<String> {
        read(&path()?).get(&key(origin, user)).cloned()
    }

    pub(crate) fn store(origin: &str, user: &str, organisation: &str) {
        let Some(path) = path() else {
            return;
        };
        let mut choices = read(&path);
        choices.insert(key(origin, user), organisation.to_owned());
        if let Ok(text) = serde_json::to_string_pretty(&choices) {
            let _ = subspace_framework::core::state::write_atomic(&path, &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Organisation, Status, resolve};

    fn org(id: u8, personal: bool) -> Organisation {
        Organisation {
            id: format!("00000000-0000-0000-0000-0000000000{id:02}"),
            name: format!("Org {id}"),
            is_personal_org: personal,
        }
    }

    #[test]
    fn the_saved_or_only_business_organisation_is_selected() {
        let one = vec![org(1, false), org(2, true)];
        assert_eq!(
            resolve(&one, None),
            (Some(one[0].id.clone()), Status::Selected)
        );
        let several = vec![org(1, false), org(3, false), org(2, true)];
        assert_eq!(resolve(&several, None), (None, Status::Choose));
        assert_eq!(
            resolve(&several, Some(several[1].id.to_uppercase())),
            (Some(several[1].id.clone()), Status::Selected)
        );
        assert_eq!(
            resolve(&several, Some(org(2, true).id)),
            (None, Status::Choose),
            "a personal organisation is never selected"
        );
        assert_eq!(resolve(&[org(2, true)], None), (None, Status::None));
    }
}
