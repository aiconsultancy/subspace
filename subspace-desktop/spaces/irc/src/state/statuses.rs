//! Nick-wide status lines, such as `Galactica is thinking…`, that agents
//! publish through `draft/metadata-2`. One network holds them, never a
//! buffer, and they are never stored: a status describes the nick, not a
//! message, and lasts only while the connection does.

use std::collections::BTreeMap;

use subspace_client_irc::status_text;

/// Each nick's status on one network, under the spelling the runtime sent.
/// The Space does not know the network's case mapping, so nicks compare
/// ignoring ASCII case, as [`crate::members::note`] does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Statuses(BTreeMap<String, String>);

impl Statuses {
    /// Holds `nick`'s status, replacing one held under any spelling of it.
    /// The text is untrusted: it is cleaned again here, to one plain line of
    /// at most 80 characters, and `None` or nothing left after cleaning
    /// clears the status.
    pub fn set(&mut self, nick: &str, status: Option<&str>) {
        self.0.retain(|held, _| !held.eq_ignore_ascii_case(nick));
        if let Some(text) = status.and_then(status_text) {
            self.0.insert(nick.to_owned(), text);
        }
    }

    /// Forgets `nick`'s status, under any spelling.
    pub fn remove(&mut self, nick: &str) {
        self.0.retain(|held, _| !held.eq_ignore_ascii_case(nick));
    }

    /// Forgets every status, as when the connection ends.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// `nick`'s status, under any spelling.
    pub fn get(&self, nick: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(held, _)| held.eq_ignore_ascii_case(nick))
            .map(|(_, text)| text.as_str())
    }

    /// Every held status as nick and text, ordered by nick.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(nick, text)| (nick.as_str(), text.as_str()))
    }
}
