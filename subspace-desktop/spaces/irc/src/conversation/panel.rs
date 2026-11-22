//! A conversation as a dock panel: its tab names the buffer and counts its
//! unread messages, and the dock tells it when it is shown and closed.

use gpui_kit::component::dock::{BasePanel, Panel, PanelControl, PanelEvent, TabGroup};
use gpui_kit::component::h_flex;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    WeakEntity, Window,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::{Tokens, badge};

use super::{Conversation, ConversationEvent};

/// The name a conversation panel is persisted under. Never change it.
pub(crate) const PANEL_NAME: &str = "irc.conversation";

/// The dock tab group holding a conversation, whose tab shows its unread
/// count, and what that tab last showed.
#[derive(Default)]
pub(super) struct Tab {
    pub group: Option<WeakEntity<TabGroup>>,
    shown: TabContent,
}

#[derive(Default, PartialEq, Eq)]
pub(super) struct TabContent {
    unread: usize,
    mention: bool,
    context: Option<String>,
}

/// What a buffer's tab says: its name, with the server buffer named after
/// its network.
pub(crate) fn tab_name(target: &Target, network: &str) -> String {
    match target {
        Target::Channel(name) | Target::Query(name) => name.clone(),
        Target::Server => network.to_owned(),
    }
}

impl Conversation {
    fn tab_label(&self, cx: &App) -> String {
        let state = &self.hub.read(cx).state;
        let network = state.network(&self.buffer.network).map_or_else(
            || self.buffer.network.0.clone(),
            |network| network.name.clone(),
        );
        tab_name(&self.buffer.target, &network)
    }

    /// Redraws the title when unread state or conversation context changes.
    pub(super) fn refresh_tab(&mut self, cx: &mut Context<Self>) {
        let content = self.tab_content(cx);
        if content == self.tab.shown {
            return;
        }
        self.tab.shown = content;
        if let Some(group) = self.tab.group.as_ref().and_then(WeakEntity::upgrade) {
            group.update(cx, |_, cx| cx.notify());
        }
    }

    fn tab_content(&self, cx: &App) -> TabContent {
        let (unread, mention) = self
            .hub
            .read(cx)
            .state
            .buffer(&self.buffer)
            .map_or((0, false), |held| (held.unread, held.highlights > 0));
        TabContent {
            unread,
            mention,
            context: self.header_facts(cx).map(|facts| facts.context()),
        }
    }
}

fn unread_items(count: usize) -> String {
    match count {
        1 => "1 unread item".to_owned(),
        _ => format!("{count} unread items"),
    }
}

fn tab_description(name: &str, unread: usize, mention: bool) -> String {
    match (unread, mention) {
        (0, _) => name.to_owned(),
        (_, true) => format!("{name}, {}, mentions you", unread_items(unread)),
        _ => format!("{name}, {}", unread_items(unread)),
    }
}

impl EventEmitter<PanelEvent> for Conversation {}

impl Focusable for Conversation {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for Conversation {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn on_added_to(&mut self, group: WeakEntity<TabGroup>, _: &mut Window, cx: &mut Context<Self>) {
        self.tab.group = Some(group);
        cx.notify();
    }

    fn on_removed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ConversationEvent::Closed);
    }
}

/// Keep the short visible name for native collapsed strips and Close labels.
/// The composed title carries unread counts and accessibility metadata.
impl Panel for Conversation {
    fn tab_name(&self, cx: &App) -> Option<gpui_kit::SharedString> {
        Some(self.tab_label(cx).into())
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let TabContent {
            unread,
            mention,
            context,
        } = self.tab_content(cx);
        let label = self.tab_label(cx);
        let mut description = tab_description(&label, unread, mention);
        if let Some(context) = &context {
            description.push_str(", ");
            description.push_str(context);
        }
        let (background, foreground) = if mention {
            (tokens.accent, tokens.bg)
        } else {
            (tokens.select, tokens.fg)
        };
        h_flex()
            .id("conversation-tab-title")
            .test_support()
            .role(gpui_kit::Role::Group)
            .aria_label(label.clone())
            .aria_description(description)
            .gap_1()
            .when_some(context, |title, context| {
                title.tooltip(move |window, cx| Tooltip::new(context.clone()).build(window, cx))
            })
            .when(unread > 0, |title| {
                title.child(subspace_framework::dashboard::tabs::unread_dot(&tokens))
            })
            .child(label)
            .when(unread > 0, |title| {
                title.child(badge::badge(unread.to_string(), background, foreground))
            })
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Target;

    use super::{tab_description, tab_name};

    #[test]
    fn unread_tab_names_explain_the_dot_and_mentions() {
        assert_eq!(tab_description("#ops", 0, false), "#ops");
        assert_eq!(tab_description("#ops", 3, false), "#ops, 3 unread items");
        assert_eq!(tab_description("#ops", 1, false), "#ops, 1 unread item");
        assert_eq!(
            tab_description("#ops", 3, true),
            "#ops, 3 unread items, mentions you"
        );
    }

    #[test]
    fn a_tab_names_the_buffer_or_the_server_s_network() {
        assert_eq!(tab_name(&Target::Channel("#ops".into()), "Lab"), "#ops");
        assert_eq!(tab_name(&Target::Query("mira".into()), "Lab"), "mira");
        assert_eq!(tab_name(&Target::Server, "Lab"), "Lab");
    }
}
