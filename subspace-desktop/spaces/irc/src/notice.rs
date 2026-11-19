//! What an IRC notification says: who and where, never what was said.
//! OS notifications show on locked screens and stay in the notification
//! history, so the message itself stays in the app.

use subspace_client_irc::Target;
use subspace_framework::dashboard::notifications::Notice;

use crate::state::Attention;

/// The notice for `attention` on the network named `network`: one sentence
/// for the toast and the OS title, and the network as the OS body.
pub(crate) fn notice(attention: &Attention, network: &str) -> Notice {
    Notice::described(
        headline(&attention.buffer.target, attention.from.as_deref()),
        network.to_owned(),
    )
}

fn headline(target: &Target, from: Option<&str>) -> String {
    match (target, from) {
        (Target::Query(nick), _) => format!("{nick} sent you a private message"),
        (Target::Channel(channel), Some(from)) => format!("{from} mentioned you in {channel}"),
        (Target::Channel(channel), None) => format!("You were mentioned in {channel}"),
        (Target::Server, Some(from)) => format!("{from} mentioned you"),
        (Target::Server, None) => "You were mentioned".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::BufferKey;

    use super::notice;
    use crate::fixtures::{channel, query};
    use crate::state::{Attention, AttentionKind};

    fn said(buffer: BufferKey, kind: AttentionKind, from: Option<&str>) -> (String, String) {
        let attention = Attention {
            buffer,
            kind,
            from: from.map(str::to_owned),
        };
        let notice = notice(&attention, "Local Ergo");
        let body = notice.system_body().cloned().unwrap_or_default();
        (notice.text().to_string(), body.to_string())
    }

    #[test]
    fn a_mention_names_the_sender_the_channel_and_the_network() {
        assert_eq!(
            said(
                channel("#subspace"),
                AttentionKind::Highlight,
                Some("alice")
            ),
            (
                "alice mentioned you in #subspace".into(),
                "Local Ergo".into()
            )
        );
        assert_eq!(
            said(channel("#subspace"), AttentionKind::Highlight, None).0,
            "You were mentioned in #subspace"
        );
    }

    #[test]
    fn a_private_message_names_the_other_person() {
        assert_eq!(
            said(query("carol"), AttentionKind::Query, Some("carol")).0,
            "carol sent you a private message"
        );
        assert_eq!(
            said(query("carol"), AttentionKind::Highlight, Some("carol")).0,
            "carol sent you a private message",
            "a tag in a private conversation reads the same"
        );
    }
}
