//! Each installed app's release key and install, in `tsx/installs.json`.
//! An install is random: grants are bound to it, so they last while
//! updates keep the key and end when the package is removed.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};

use super::{Paths, replace};

/// Each installed app's release key and install.
pub(super) type Installs = BTreeMap<String, (String, String)>;

pub(super) fn read_installs(paths: &Paths) -> Installs {
    std::fs::read_to_string(paths.installs())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(super) fn write_installs(paths: &Paths, installs: &Installs) -> Result<(), String> {
    let text = serde_json::to_string_pretty(installs).map_err(|error| error.to_string())?;
    replace(&paths.installs(), text.as_bytes())
}

/// A random install identifier.
pub(super) fn new_install() -> String {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos()),
    );
    format!("{:016x}", hasher.finish())
}
