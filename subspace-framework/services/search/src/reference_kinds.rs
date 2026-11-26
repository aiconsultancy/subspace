//! App-owned reference kinds, shared by every service in the process.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

const MAX_KIND_CHARS: usize = 64;
const BUILT_IN: [&str; 5] = [
    "repository",
    "branch",
    "commit",
    "github",
    "github-copilot-agents",
];
static APP_KINDS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Register an app-owned reference kind before indexing starts.
/// Names are 1-64 ASCII characters, start with a lower-case letter, and contain
/// only lower-case letters, digits and hyphens. Registration is idempotent and
/// lasts for the process; the built-in kinds are always accepted.
// qual:api - Registration belongs to app startup outside the Framework.
pub fn register_reference_kind(kind: &str) -> Result<(), String> {
    let valid = (1..=MAX_KIND_CHARS).contains(&kind.len())
        && kind.as_bytes()[0].is_ascii_lowercase()
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        return Err("Invalid reference kind".into());
    }
    APP_KINDS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(kind.to_owned());
    Ok(())
}

pub(crate) fn is_known(kind: &str) -> bool {
    BUILT_IN.contains(&kind)
        || APP_KINDS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(kind)
}
