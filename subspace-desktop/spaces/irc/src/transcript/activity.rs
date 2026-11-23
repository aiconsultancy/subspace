//! The line above the composer: the status lines of agents in the
//! conversation, then who else is typing. A status is the agent's own text
//! for its whole nick, such as `Galactica is thinking…`, so the line shows
//! it as one plain line and never as being about a message.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::{BufferKey, Target};
use subspace_framework::dashboard::{Tokens, truncated};

use crate::state::{Buffer, IrcState, Statuses};

/// Between the parts of the line.
const SEPARATOR: &str = " · ";
/// Between the parts of its accessible name.
const SPOKEN_SEPARATOR: &str = "; ";

/// What the line shows, and its accessible name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Activity {
    pub text: String,
    pub label: String,
}

/// The line above the composer of conversation `key`, which keeps its
/// height while empty so the transcript does not jump. `None` when the hub
/// holds no such buffer.
pub(crate) fn line(state: &IrcState, key: &BufferKey, tokens: &Tokens) -> Option<AnyElement> {
    let held = state.buffer(key)?;
    Some(render(of(state, key, held), tokens))
}

/// What the line says in conversation `key`, whose buffer is `held`.
pub(crate) fn of(state: &IrcState, key: &BufferKey, held: &Buffer) -> Option<Activity> {
    let network = state.network(&key.network)?;
    let present = present(held, &key.target);
    let statuses = shown(&network.statuses, &present, &network.nick);
    let typing: Vec<&str> = held.typing.keys().map(String::as_str).collect();
    compose(&statuses, &typing)
}

/// Who is in the conversation: a channel's members, or a private
/// conversation's peer. Nobody for the server buffer.
fn present<'a>(held: &'a Buffer, target: &'a Target) -> Vec<&'a str> {
    match target {
        Target::Channel(_) => held
            .members
            .iter()
            .map(|member| member.nick.as_str())
            .collect(),
        Target::Query(peer) => vec![peer.as_str()],
        Target::Server => Vec::new(),
    }
}

/// The held statuses of nicks in `present`, as nick and text, never our
/// `own`. Nicks compare ignoring ASCII case: the status may carry another
/// spelling than the member list.
pub(crate) fn shown<'a>(
    statuses: &'a Statuses,
    present: &[&str],
    own: &str,
) -> Vec<(&'a str, &'a str)> {
    statuses
        .iter()
        .filter(|(nick, _)| {
            !nick.eq_ignore_ascii_case(own)
                && present.iter().any(|here| here.eq_ignore_ascii_case(nick))
        })
        .collect()
}

/// The statuses first, verbatim and joined with ` · `, then who else is
/// typing: a nick with a status shows its status only. `None` when there
/// is nothing to say.
pub(crate) fn compose(statuses: &[(&str, &str)], typing: &[&str]) -> Option<Activity> {
    let typing = typing_text(&without_status(typing, statuses));
    joined(statuses, typing)
}

/// The typing nicks that have no status.
fn without_status<'a>(typing: &[&'a str], statuses: &[(&str, &str)]) -> Vec<&'a str> {
    typing
        .iter()
        .copied()
        .filter(|nick| {
            !statuses
                .iter()
                .any(|(held, _)| held.eq_ignore_ascii_case(nick))
        })
        .collect()
}

/// The status texts and the typing text as one line, and the accessible
/// name that says whose each status is.
fn joined(statuses: &[(&str, &str)], typing: Option<String>) -> Option<Activity> {
    let texts: Vec<String> = statuses
        .iter()
        .map(|(_, text)| (*text).to_owned())
        .chain(typing.clone())
        .collect();
    let labels: Vec<String> = statuses
        .iter()
        .map(|(nick, text)| format!("Status of {nick}: {text}"))
        .chain(typing)
        .collect();
    (!texts.is_empty()).then(|| Activity {
        text: texts.join(SEPARATOR),
        label: labels.join(SPOKEN_SEPARATOR),
    })
}

/// `alice is typing…`, `alice and bob are typing…`, `3 people are typing…`.
pub(crate) fn typing_text(nicks: &[&str]) -> Option<String> {
    match nicks {
        [] => None,
        [one] => Some(format!("{one} is typing…")),
        [one, two] => Some(format!("{one} and {two} are typing…")),
        many => Some(format!("{} people are typing…", many.len())),
    }
}

/// One quiet caption line, cut with an ellipsis and the whole text in a
/// tooltip when it does not fit.
fn render(activity: Option<Activity>, tokens: &Tokens) -> AnyElement {
    let (text, label) = activity
        .map(|activity| (activity.text, activity.label))
        .unzip();
    div()
        .id("irc-activity")
        .when_some(label, |line, label| {
            line.role(Role::Label).aria_label(label)
        })
        .test_support()
        .flex_shrink_0()
        .h(tokens.row_height)
        .px_2()
        .min_w_0()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .children(text.map(|text| {
            truncated::styled(
                "irc-activity-text",
                text.clone(),
                text.clone().test_text(text),
            )
            .w_full()
            .text_size(tokens.caption_size())
            .text_color(tokens.muted)
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Activity, compose, shown, typing_text};
    use crate::state::Statuses;

    fn statuses(held: &[(&str, &str)]) -> Statuses {
        let mut statuses = Statuses::default();
        for &(nick, text) in held {
            statuses.set(nick, Some(text));
        }
        statuses
    }

    fn text(activity: Option<Activity>) -> Option<String> {
        activity.map(|activity| activity.text)
    }

    #[test]
    fn typing_alone_reads_as_before() {
        assert_eq!(typing_text(&[]), None);
        assert_eq!(
            text(compose(&[], &["alice"])).as_deref(),
            Some("alice is typing…")
        );
        assert_eq!(
            text(compose(&[], &["alice", "bob"])).as_deref(),
            Some("alice and bob are typing…")
        );
        assert_eq!(
            text(compose(&[], &["alice", "bob", "carol"])).as_deref(),
            Some("3 people are typing…")
        );
        assert_eq!(compose(&[], &[]), None, "nothing to say");
    }

    #[test]
    fn a_status_comes_first_and_a_person_typing_still_shows_beside_it() {
        let activity = compose(&[("galactica", "Galactica is thinking…")], &["alice"]);
        assert_eq!(
            activity,
            Some(Activity {
                text: "Galactica is thinking… · alice is typing…".into(),
                label: "Status of galactica: Galactica is thinking…; alice is typing…".into(),
            })
        );
    }

    #[test]
    fn an_agent_with_a_status_that_is_also_typing_shows_its_status_only() {
        let held = [
            ("discovery", "Discovery is noodling on it…"),
            ("galactica", "Galactica is thinking…"),
        ];
        assert_eq!(
            text(compose(&held, &["Galactica", "discovery"])).as_deref(),
            Some("Discovery is noodling on it… · Galactica is thinking…"),
            "two statuses, and nobody else typing"
        );
        assert_eq!(
            text(compose(&held, &["galactica", "alice", "bob"])).as_deref(),
            Some(
                "Discovery is noodling on it… · Galactica is thinking… · alice and bob are typing…"
            )
        );
    }

    #[test]
    fn only_the_conversation_s_nicks_show_and_never_our_own() {
        let held = statuses(&[
            ("Galactica", "Galactica is thinking…"),
            ("discovery", "Discovery is noodling on it…"),
            ("subspace", "Our own status"),
        ]);
        assert_eq!(
            shown(&held, &["galactica", "alice", "SubSpace"], "subspace"),
            vec![("Galactica", "Galactica is thinking…")],
            "a member in another spelling shows; one not here and our own do not"
        );
        assert_eq!(
            shown(&held, &["discovery"], "subspace"),
            vec![("discovery", "Discovery is noodling on it…")],
            "a private conversation's peer"
        );
        assert!(
            shown(&held, &[], "subspace").is_empty(),
            "the server buffer"
        );
    }

    #[test]
    fn the_shown_text_is_the_status_verbatim_as_plain_text() {
        let literal = "**Thinking** about [docs](https://example.org) @alice #ops";
        let held = statuses(&[("galactica", literal)]);
        let visible = shown(&held, &["galactica"], "subspace");
        assert_eq!(
            text(compose(&visible, &[])).as_deref(),
            Some(literal),
            "Markdown, links and mentions stay literal text"
        );
    }
}
