//! Network ids: stable once saved, derived from the name for new networks.

use super::draft::NetworkText;

/// The id of a network whose name and host give no letters or digits.
const FALLBACK_ID: &str = "network";

/// A lower-case id from a network's name: letters and digits, words joined
/// by `-`.
pub(crate) fn slug(name: &str) -> String {
    let words: Vec<String> = name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    words.join("-")
}

/// `base`, else `base-2`, `base-3`… so no two networks share an id.
pub(crate) fn unique_id(base: &str, taken: &[String]) -> String {
    let base = if base.is_empty() { FALLBACK_ID } else { base };
    let mut candidate = base.to_owned();
    let mut suffix = 2;
    while taken.iter().any(|id| id == &candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

/// The id a new network gets from its name, else its host.
pub(crate) fn new_id(text: &NetworkText, taken: &[String]) -> String {
    let from_name = slug(&text.name);
    let base = if from_name.is_empty() {
        slug(&text.host)
    } else {
        from_name
    };
    unique_id(&base, taken)
}
