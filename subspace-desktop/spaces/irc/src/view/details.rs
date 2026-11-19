//! What a buffer's row and the server buffer's header say about it.

use subspace_client_irc::Target;
use subspace_framework::dashboard::panel::IconName;

use crate::rows;
use crate::state::Network;

pub(super) fn kind(target: &Target) -> &'static str {
    match target {
        Target::Server => "Server",
        Target::Channel(_) => "Channel",
        Target::Query(_) => "Private messages",
    }
}

pub(super) fn kind_icon(target: &Target) -> IconName {
    match target {
        Target::Server => IconName::Server,
        Target::Channel(_) => IconName::Hash,
        Target::Query(_) => IconName::User,
    }
}

/// The server buffer's header line: status, nick and capabilities.
pub(crate) fn server_summary(network: &Network) -> String {
    let caps = if network.caps.is_empty() {
        "none yet".to_owned()
    } else {
        network.caps.join(", ")
    };
    format!(
        "{} · {} · Capabilities: {caps}",
        status_detail(&network.status),
        network.nick
    )
}

/// The status with its reason, for the server panel.
pub(crate) fn status_detail(status: &subspace_client_irc::ConnectionStatus) -> String {
    use subspace_client_irc::ConnectionStatus;
    match status {
        ConnectionStatus::Failed { error } | ConnectionStatus::Reconnecting { error, .. }
            if !error.is_empty() =>
        {
            format!("{} · {error}", rows::status_label(status))
        }
        _ => rows::status_label(status),
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Target;
    use subspace_framework::dashboard::panel::IconName;

    use super::{kind, kind_icon};

    #[test]
    fn each_buffer_kind_has_its_word_and_icon() {
        let targets = [
            Target::Server,
            Target::Channel("#subspace".into()),
            Target::Query("mira".into()),
        ];
        let words: Vec<_> = targets.iter().map(kind).collect();
        assert_eq!(words, ["Server", "Channel", "Private messages"]);
        let icons: Vec<_> = targets.iter().map(kind_icon).collect();
        assert_eq!(icons, [IconName::Server, IconName::Hash, IconName::User]);
    }
}
