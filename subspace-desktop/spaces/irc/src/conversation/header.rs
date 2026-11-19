//! Context shared by the dock title and the heading of a standalone conversation.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::{Tokens, avatar, heading_font, truncated};

use super::Conversation;

/// The name's size before density scaling: a step over body text, under
/// the shared panel title.
const TITLE_SIZE_PX: f32 = 15.;

/// What the header shows, read from the hub.
pub(super) struct HeaderFacts {
    pub title: String,
    pub network: String,
    /// The topic without formatting codes.
    pub topic: Option<String>,
}

impl HeaderFacts {
    pub(super) fn context(&self) -> String {
        match self.topic.as_deref().filter(|topic| !topic.is_empty()) {
            Some(topic) => format!("{} · {topic}", self.network),
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
        let facts = self.header_facts(cx)?;
        Some(title(&self.buffer.target, &facts, cx))
    }
}

/// The leading part of the header row: avatar for a person, name, then the
/// network and topic.
fn title(target: &Target, facts: &HeaderFacts, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let person = match target {
        Target::Query(nick) => Some(nick.clone()),
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
        .child(
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .font_family(heading_font(cx))
                .text_size(tokens.heading_size(TITLE_SIZE_PX))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(facts.title.clone()),
        )
        .child(
            truncated::text("irc-header-context", facts.context())
                .flex_1()
                .text_size(tokens.secondary_size())
                .text_color(tokens.muted),
        )
        .into_any_element()
}
