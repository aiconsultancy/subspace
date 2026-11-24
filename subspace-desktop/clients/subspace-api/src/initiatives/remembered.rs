//! The initiative a person last chose for each account and organisation,
//! kept between runs in the app's state file through the `prefs` global,
//! which saves it with the app's other choices. Each entry names its place
//! by a digest of the API, the verified user and the organisation, and holds
//! only the chosen initiative's ID.

use std::collections::BTreeMap;

use gpui_kit::App;
use subspace_framework::dashboard::prefs;

/// The state file key. Its value maps each place to an initiative ID.
const KEY: &str = "initiatives.selected";
/// The digest bytes that name a place.
const PLACE_BYTES: usize = 16;

/// Names the API at `origin`, its verified `user` and, for NETT, the
/// `organisation`, without keeping any of them.
pub(super) fn place(origin: &str, user: &str, organisation: Option<&str>) -> String {
    use sha2::Digest as _;
    let mut digest = sha2::Sha256::new();
    for part in [origin, user, organisation.unwrap_or_default()] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    digest.finalize()[..PLACE_BYTES]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The initiative last chosen in `place`, if any.
pub(super) fn get(place: &str, cx: &App) -> Option<String> {
    choices(cx).remove(place)
}

/// Remembers `id` as the initiative chosen in `place`. The state file is
/// only marked changed when the choice is new.
pub(super) fn set(place: &str, id: &str, cx: &mut App) {
    let text = with_choice(choices(cx), place, id);
    prefs::set(cx, KEY, text);
}

/// `choices` with `id` chosen in `place`, as the state file keeps them.
fn with_choice(mut choices: BTreeMap<String, String>, place: &str, id: &str) -> String {
    choices.insert(place.to_owned(), id.to_owned());
    serde_json::to_string(&choices).unwrap_or_default()
}

/// Every remembered choice; anything unreadable reads as none.
fn choices(cx: &App) -> BTreeMap<String, String> {
    prefs::get(cx, KEY)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::place;

    #[test]
    fn a_place_names_the_api_the_user_and_the_organisation_apart() {
        let here = place("https://api.example.com/api", "user-ada", None);
        assert_eq!(here.len(), 32);
        assert!(!here.contains("user-ada"), "no identifier is kept");
        assert_eq!(here, place("https://api.example.com/api", "user-ada", None));
        assert_ne!(here, place("https://api.example.org/api", "user-ada", None));
        assert_ne!(here, place("https://api.example.com/api", "user-bob", None));
        assert_ne!(
            place("https://api.example.com/api", "user-ada", Some("org-1")),
            place("https://api.example.com/api", "user-ada", Some("org-2"))
        );
        // Each part is length-prefixed, so moving text between parts changes
        // the place.
        assert_ne!(place("ab", "c", None), place("a", "bc", None));
    }
}
