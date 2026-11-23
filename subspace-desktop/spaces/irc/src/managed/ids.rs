//! Account network ids: `@account.<person>.<server>`, 16 lower-case
//! hexadecimal digits each of SHA-256 of the API's user ID and of the
//! canonical endpoint. Only this exact form is an account network's; a saved
//! network may have any other id, including others that start with `@`, and
//! one that starts with `@account.` is refused when the settings load. The
//! store keeps every account network's lines; search shows them only while
//! their person is signed in.

use sha2::{Digest as _, Sha256};
use subspace_client_irc::NetworkId;
use subspace_framework::dashboard::full_text::Document;

use crate::initiatives::Endpoint;

/// The start of every account network's id, kept from saved networks.
const ACCOUNT_PREFIX: &str = "@account.";
/// Hexadecimal digits kept of each digest in an id.
const DIGEST_DIGITS: usize = 16;

/// The account network's id for the person with the API's user ID
/// `person` on the server at `endpoint`: the same on every run, different
/// for every other person and server.
pub(crate) fn network_id(person: &str, endpoint: &Endpoint) -> NetworkId {
    let server = format!("{}|{}|{}", endpoint.host, endpoint.port, endpoint.tls);
    NetworkId(format!(
        "{ACCOUNT_PREFIX}{}.{}",
        digest(person),
        digest(&server)
    ))
}

/// The person part of account network ids, from the API's user ID.
pub(crate) fn digest(text: &str) -> String {
    let mut digest = hex::encode(Sha256::digest(text.as_bytes()));
    digest.truncate(DIGEST_DIGITS);
    digest
}

/// Whether a saved network's `id` would be taken for an account network's.
pub(crate) fn reserved(id: &str) -> bool {
    id.starts_with(ACCOUNT_PREFIX)
}

/// The person part of the first account network id in `text`.
fn owner_in(text: &str) -> Option<&str> {
    let start = text.find(ACCOUNT_PREFIX)? + ACCOUNT_PREFIX.len();
    let rest = text.get(start..)?;
    let (person, server) = (rest.get(..DIGEST_DIGITS)?, rest.get(DIGEST_DIGITS + 1..)?);
    let hex = |part: &str| {
        part.len() >= DIGEST_DIGITS
            && part[..DIGEST_DIGITS]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    (rest.as_bytes().get(DIGEST_DIGITS) == Some(&b'.') && hex(person) && hex(server))
        .then_some(person)
}

/// Whether search must hide a stored `document` now: it belongs to an
/// account network of a person other than `current`, the person signed in
/// (`None` while nobody is).
pub(crate) fn hidden(document: &Document, current: Option<&str>) -> bool {
    [document.parent_id.as_deref(), Some(document.id.as_str())]
        .into_iter()
        .flatten()
        .find_map(owner_in)
        .is_some_and(|owner| Some(owner) != current)
}

#[cfg(test)]
mod tests {
    use subspace_framework::dashboard::full_text::Document;

    use super::{digest, hidden, network_id, reserved};
    use crate::initiatives::Endpoint;

    fn endpoint(host: &str, tls: bool) -> Endpoint {
        Endpoint {
            host: host.into(),
            port: 6697,
            tls,
        }
    }

    #[test]
    fn an_account_network_s_id_is_its_person_s_and_server_s_alone() {
        let ada = network_id("user-ada", &endpoint("irc.example.com", true));
        assert_eq!(
            ada,
            network_id("user-ada", &endpoint("irc.example.com", true))
        );
        assert_ne!(
            ada,
            network_id("user-bob", &endpoint("irc.example.com", true))
        );
        assert_ne!(
            ada,
            network_id("user-ada", &endpoint("irc.example.org", true))
        );
        assert_ne!(
            ada,
            network_id("user-ada", &endpoint("irc.example.com", false))
        );
        assert!(reserved(&ada.0));
    }

    #[test]
    fn only_the_account_prefix_is_kept_from_saved_networks() {
        for saved in [
            "@work",
            "@acc",
            "@accounts",
            "local",
            "ada-irc-example-com-6697",
        ] {
            assert!(!reserved(saved), "{saved}");
        }
        assert!(reserved("@account.x.y"));
    }

    #[test]
    fn search_hides_another_person_s_account_lines_only() {
        let ada = network_id("user-ada", &endpoint("irc.example.com", true));
        let line = |network: &str| Document {
            id: format!("line:{network}"),
            title: String::new(),
            body: "launch".into(),
            metadata: String::new(),
            parent_id: Some(format!("buffer:{network}:channel:#nav")),
            kind: "message".into(),
            url: None,
            navigation: String::new(),
            refreshed_at: 0,
            payload: None,
        };
        let mine = digest("user-ada");
        assert!(!hidden(&line(&ada.0), Some(&mine)));
        assert!(hidden(&line(&ada.0), Some(&digest("user-bob"))));
        assert!(hidden(&line(&ada.0), None), "nobody signed in");
        assert!(
            !hidden(&line("local"), None),
            "a saved network's lines always show"
        );
        assert!(!hidden(&line("@work"), Some(&mine)));
        assert!(
            !hidden(&line("@account.x.y"), None),
            "a hand-made id is never classified as an account's"
        );
    }
}
