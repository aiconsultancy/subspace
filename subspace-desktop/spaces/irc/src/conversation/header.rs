//! Context shared by the dock title, the board's toolbar heading and the
//! heading of a standalone conversation.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::{Tokens, avatar, truncated};

use super::Conversation;

/// What the header shows, read from the hub.
pub(super) struct HeaderFacts {
    pub title: String,
    pub network: String,
    /// The topic without formatting codes.
    pub topic: Option<String>,
}

impl HeaderFacts {
    /// The network and topic, for the dock tab's tooltip.
    pub(super) fn context(&self) -> String {
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => format!("{} · {topic}", self.network),
            None => self.network.clone(),
        }
    }

    /// The quiet text beside the name: the topic, or the network (for the
    /// server buffer, its status) while there is none.
    fn quiet(&self) -> String {
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => topic.to_owned(),
            None => self.network.clone(),
        }
    }

    fn spoken(&self) -> String {
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => format!("{}, {}, topic: {topic}", self.title, self.network),
            None => format!("{}, {}", self.title, self.network),
        }
    }
}

impl Conversation {
    pub(super) fn standalone_title(&self, cx: &App) -> Option<AnyElement> {
        if self.group.is_some() {
            return None;
        }
        self.heading(true, cx)
    }

    /// The heading for a host that shows it outside the conversation, as the
    /// board's toolbar does for the conversation in front. With `named`
    /// false the name is left to a control beside it, such as a menu that
    /// already says it.
    pub(crate) fn heading(&self, named: bool, cx: &App) -> Option<AnyElement> {
        let facts = self.header_facts(cx)?;
        Some(title(&self.buffer.target, &facts, named, cx))
    }
}

/// The leading part of the header row: avatar for a person and the name in
/// strong text when `named`, then the topic in quiet text on the same line.
/// The accessible name still says the network.
fn title(target: &Target, facts: &HeaderFacts, named: bool, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let person = match target {
        Target::Query(nick) if named => Some(nick.clone()),
        _ => None,
    };
    h_flex()
        .id("irc-header")
        .role(Role::Heading)
        .aria_label(facts.spoken())
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .children(person.map(|nick| avatar::member(nick, &tokens)))
        .children(named.then(|| {
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_size(tokens.row_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(facts.title.clone())
        }))
        .child(
            truncated::text("irc-header-context", facts.quiet())
                .flex_1()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::HeaderFacts;

    fn facts(topic: Option<&str>) -> HeaderFacts {
        HeaderFacts {
            title: "#subspace".to_owned(),
            network: "Local Ergo".to_owned(),
            topic: topic.map(str::to_owned),
        }
    }

    #[test]
    fn beside_the_name_the_topic_shows_alone_and_the_network_stands_in_for_none() {
        assert_eq!(facts(Some("Release day")).quiet(), "Release day");
        assert_eq!(facts(Some("")).quiet(), "Local Ergo");
        assert_eq!(facts(None).quiet(), "Local Ergo");
        assert_eq!(
            facts(Some("Release day")).context(),
            "Local Ergo · Release day",
            "the tab's tooltip still names the network"
        );
    }
}
