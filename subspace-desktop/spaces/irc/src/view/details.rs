//! What a buffer's row and the server buffer's header say about it, and
//! the words of the status bar and the member inspector's title.

use subspace_client_irc::Target;
use subspace_framework::dashboard::panel::IconName;

use crate::rows;
use crate::state::Network;

/// The service the status bar names first.
const SERVICE: &str = "IRC";

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

/// The status bar's words: the service, the `total` of networks and how many
/// of them are `connected`, then a hint such as the connection an initiative
/// still needs. Nothing while there is neither a network nor a hint.
pub(crate) fn status_line(total: usize, connected: usize, hint: Option<&str>) -> Option<String> {
    let counts = (total > 0).then(|| {
        let noun = if total == 1 { "network" } else { "networks" };
        format!("{total} {noun} · {connected} connected")
    });
    if counts.is_none() && hint.is_none() {
        return None;
    }
    let parts = [Some(SERVICE), counts.as_deref(), hint];
    Some(parts.into_iter().flatten().collect::<Vec<_>>().join(" · "))
}

/// The member inspector's title: `Members`, with the count for a channel.
pub(super) fn members_title(count: Option<usize>) -> String {
    match count {
        Some(count) => format!("Members · {count}"),
        None => "Members".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Target;
    use subspace_framework::dashboard::panel::IconName;

    use super::{kind, kind_icon, members_title, status_line};

    #[test]
    fn the_status_bar_names_the_service_the_networks_and_any_hint() {
        assert_eq!(
            status_line(1, 1, None).as_deref(),
            Some("IRC · 1 network · 1 connected")
        );
        assert_eq!(
            status_line(3, 0, Some("Checking initiatives…")).as_deref(),
            Some("IRC · 3 networks · 0 connected · Checking initiatives…")
        );
        let hint = "Add one connection to irc.example.com for initiatives";
        assert_eq!(
            status_line(0, 0, Some(hint)).as_deref(),
            Some("IRC · Add one connection to irc.example.com for initiatives"),
            "the hint shows before any network is configured"
        );
        assert_eq!(status_line(0, 0, None), None);
    }

    #[test]
    fn the_member_inspector_counts_a_channel_s_members() {
        assert_eq!(members_title(Some(4)), "Members · 4");
        assert_eq!(members_title(Some(0)), "Members · 0");
        assert_eq!(members_title(None), "Members", "not a channel");
    }

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
