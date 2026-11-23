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

/// The person part of `id` when it is exactly an account network id:
/// `@account.`, then 16 lower-case hexadecimal digits, a dot and 16 more.
fn owner(id: &str) -> Option<&str> {
    let (person, server) = id.strip_prefix(ACCOUNT_PREFIX)?.split_once('.')?;
    (is_digest(person) && is_digest(server)).then_some(person)
}

fn is_digest(part: &str) -> bool {
    part.len() == DIGEST_DIGITS
        && part
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The network a stored record belongs to, from its structured fields: a
/// line's navigation, or an ending's buffer. Never from its ids or text.
fn network_of(document: &Document) -> Option<NetworkId> {
    crate::store::navigation(document)
        .map(|navigation| navigation.network)
        .or_else(|| crate::store::ended(document).map(|ended| ended.buffer.network))
}

/// Whether search must hide a stored `document` now: it belongs to an
/// account network of a person other than `current`, the person signed in
/// (`None` while nobody is).
pub(crate) fn hidden(document: &Document, current: Option<&str>) -> bool {
    network_of(document)
        .as_ref()
        .and_then(|network| owner(&network.0))
        .is_some_and(|owner| Some(owner) != current)
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::{BufferKey, NetworkId, Target};
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

    /// A stored line of `network`'s `channel`, with its ids and parent as
    /// the store writes them.
    fn line(network: &str, channel: &str) -> Document {
        let navigation = serde_json::json!({
            "network": NetworkId(network.into()),
            "target": Target::Channel(channel.into()),
            "msgid": null,
            "time": chrono::Utc::now(),
            "from": "lyra",
        });
        Document {
            id: format!("msgid:{network}:{channel}"),
            title: String::new(),
            body: "launch".into(),
            metadata: String::new(),
            parent_id: Some(format!("buffer:{network}:c:{channel}")),
            kind: "message".into(),
            url: None,
            navigation: navigation.to_string(),
            refreshed_at: 0,
            payload: None,
        }
    }

    #[test]
    fn search_hides_another_person_s_account_lines_only() {
        let ada = network_id("user-ada", &endpoint("irc.example.com", true));
        let mine = digest("user-ada");
        assert!(!hidden(&line(&ada.0, "#nav"), Some(&mine)));
        assert!(hidden(&line(&ada.0, "#nav"), Some(&digest("user-bob"))));
        assert!(hidden(&line(&ada.0, "#nav"), None), "nobody signed in");
        assert!(
            !hidden(&line("local", "#nav"), None),
            "a saved network's lines always show"
        );
        assert!(!hidden(&line("@work", "#nav"), Some(&mine)));
        assert!(
            !hidden(&line("@account.x.y", "#nav"), None),
            "a hand-made id is never classified as an account's"
        );
        let ended = crate::store::Ended {
            buffer: BufferKey {
                network: ada.clone(),
                target: Target::Channel("#nav".into()),
            },
            request: "request-1".into(),
            outcome: None,
            at: None,
        };
        let ending = Document {
            // The kind the store gives a request's ending.
            kind: "Channels request ending".into(),
            navigation: String::new(),
            payload: serde_json::to_string(&ended).ok(),
            ..line("local", "#nav")
        };
        assert!(hidden(&ending, None), "an ending by its buffer's network");
    }

    #[test]
    fn only_the_record_s_network_decides_never_its_names() {
        let ada = network_id("user-ada", &endpoint("irc.example.com", true));
        let named = line("local", &format!("#{}", ada.0));
        assert!(
            !hidden(&named, None),
            "a saved channel named like an account network shows"
        );
        for id in [
            "@account.0123456789abcdeé.0123456789abcdef",
            "@account.0123456789abcdef.0123456789abcdeé",
            "@account.0123456789abcdef.0123456789abcdef\u{e9}",
            "@account.é",
            "@account.0123456789abcdef.",
            "@account.0123456789ABCDEF.0123456789abcdef",
            "@account.0123456789abcdef.0123456789abcdef.0",
        ] {
            assert!(!hidden(&line(id, "#nav"), None), "{id}");
            let text = format!("#{id}é");
            assert!(!hidden(&line("local", &text), None), "{text}");
        }
    }
}
