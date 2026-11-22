//! A message's hover toolbar: a compact group of icon buttons at the
//! message's top right, on the content surface, shown while the pointer is
//! over the message or it has the keyboard's focus. The design guide has
//! no row-action pattern, so this is the plain form: Lucide glyphs, a
//! tooltip and an accessible name each, a full hairline border and no
//! accent edge.
//!
//! - **React** opens the reaction menu ([`crate::conversation::reacting`]),
//!   the same one as **Add reaction** and `r`, stop group included.
//! - **Reply in thread** opens the thread in the inspector and starts the
//!   composer's reply mode, where the inspector has a Thread tab.
//! - **Make issue** and **Record decision** open their dialogs
//!   ([`crate::tracker::message`]); only a dialog's button writes.
//!
//! An action shows only when it can work. The toolbar is drawn over the
//! row, never in its flow, so it changes no message's height and moves
//! nothing in the transcript. Each action also has a key on a focused
//! message: `r`, `t`, `i` and `d`.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    Window, div, px,
};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, icon_button};
use subspace_framework::dashboard::panel::IconName;

use super::RowContext;
use crate::conversation::Conversation;
use crate::state::Entry;

/// How far the toolbar sits in from the row's right edge.
const INSET_PX: f32 = 8.;

/// One action of the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessageAction {
    React,
    Reply,
    MakeIssue,
    RecordDecision,
}

impl MessageAction {
    /// Its element id suffix, its accessible name and tooltip (with its key
    /// on a focused message) and its glyph.
    fn look(self) -> (&'static str, &'static str, IconName) {
        match self {
            Self::React => (
                "react",
                "Add reaction (r)",
                IconName::FaceSlightlySmilingPlus,
            ),
            Self::Reply => ("reply", "Reply in thread (t)", IconName::Reply),
            Self::MakeIssue => ("make-issue", "Make issue… (i)", IconName::CircleDot),
            Self::RecordDecision => ("record-decision", "Record decision… (d)", IconName::Scale),
        }
    }
}

/// What the conversation allows now, the same for every message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Availability {
    /// The network can react now.
    pub react: bool,
    /// The inspector has a Thread tab and the network carries reply tags.
    pub reply: bool,
    /// Tracker writes work here.
    pub tracker: bool,
    /// The conversation has shown an issue a decision could be on.
    pub in_play: bool,
}

impl Availability {
    pub(crate) fn of(conversation: &Conversation, cx: &App) -> Self {
        let hub = conversation.hub.read(cx);
        let buffer = &conversation.buffer;
        let threads = crate::thread::has_tab(&conversation.roster, buffer);
        let network = hub.state.network(&buffer.network);
        Self {
            react: conversation.reacting.supported,
            reply: threads && crate::thread::reply_refusal(network).is_none(),
            tracker: crate::tracker::available(buffer, &hub.issues).is_ok(),
            in_play: !crate::tracker::in_play(&conversation.tiles).is_empty(),
        }
    }
}

/// The actions message `entry` offers under `available`; `names_issue`
/// says its text names an issue. A message's words never add an action.
pub(crate) fn offered(
    entry: &Entry,
    available: Availability,
    names_issue: bool,
) -> Vec<MessageAction> {
    let message = crate::state::is_message(entry.line.kind);
    let id = entry.line.id.is_some();
    let words = !entry.line.text.trim().is_empty();
    [
        (MessageAction::React, available.react && id),
        (MessageAction::Reply, available.reply && id),
        (MessageAction::MakeIssue, available.tracker && words),
        (
            MessageAction::RecordDecision,
            available.tracker && words && (names_issue || available.in_play),
        ),
    ]
    .into_iter()
    .filter(|(_, works)| message && *works)
    .map(|(action, _)| action)
    .collect()
}

/// Whether message `seq`'s toolbar shows: the pointer is over it or it
/// has the keyboard's focus.
pub(crate) fn shown(seq: i64, context: &RowContext) -> bool {
    context.hovered == Some(seq) || context.focus.is_some_and(|at| at.seq == seq)
}

/// The toolbar over message `entry`'s row, when it shows and offers
/// anything.
pub(crate) fn toolbar(
    entry: &Entry,
    names_issue: bool,
    context: &RowContext,
    cx: &App,
) -> Option<AnyElement> {
    if !shown(entry.seq, context) {
        return None;
    }
    let actions = offered(entry, context.actions, names_issue);
    if actions.is_empty() {
        return None;
    }
    let tokens = Tokens::current(cx);
    let seq = entry.seq;
    let buttons = actions
        .into_iter()
        .map(|action| button(action, seq, &context.conversation));
    Some(
        h_flex()
            .id(SharedString::from(format!("irc-line:{seq}:toolbar")))
            .role(Role::Toolbar)
            .aria_label("Message actions")
            .test_support()
            .absolute()
            .top_0()
            .right(px(INSET_PX))
            .gap_0p5()
            .p_0p5()
            .rounded(tokens.control_radius)
            .border_1()
            .border_color(tokens.line)
            .bg(tokens.surface)
            .shadow_sm()
            .children(buttons)
            .into_any_element(),
    )
}

fn button(action: MessageAction, seq: i64, conversation: &WeakEntity<Conversation>) -> AnyElement {
    let conversation = conversation.clone();
    let (id, label, icon) = action.look();
    div()
        .child(
            icon_button(
                SharedString::from(format!("irc-line:{seq}:{id}")),
                Intent::Quiet,
                icon,
                label,
            )
            .on_click(move |_, window, cx| {
                _ = conversation.update(cx, |this, cx| run(this, action, seq, window, cx));
            }),
        )
        .into_any_element()
}

/// Does `action` for message `seq`: opens a menu or a dialog, or starts
/// reply mode. Nothing here writes to the tracker.
pub(crate) fn run(
    conversation: &mut Conversation,
    action: MessageAction,
    seq: i64,
    window: &mut Window,
    cx: &mut gpui_kit::Context<Conversation>,
) {
    use crate::tracker::message::{self, Kind};
    match action {
        MessageAction::React => {
            crate::conversation::reacting::open_choices_for(conversation, seq, window, cx);
        }
        MessageAction::Reply => crate::conversation::reply_in_thread(conversation, seq, window, cx),
        MessageAction::MakeIssue => message::open(conversation, Kind::MakeIssue, seq, window, cx),
        MessageAction::RecordDecision => {
            message::open(conversation, Kind::RecordDecision, seq, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::LineKind;

    use super::{Availability, MessageAction, offered};
    use crate::fixtures::{channel, line, message};
    use crate::state::Entry;

    fn entry(kind: LineKind, text: &str, id: Option<&str>) -> Entry {
        let room = channel("#aurora-lyra");
        let mut line = match kind {
            LineKind::Privmsg => message(&room, "mira", text, 1),
            other => line(&room, other, "mira", text, 1),
        };
        line.id = id.map(str::to_owned);
        Entry {
            seq: 1,
            line,
            highlight: false,
        }
    }

    const ALL: Availability = Availability {
        react: true,
        reply: true,
        tracker: true,
        in_play: true,
    };

    #[test]
    fn every_action_shows_only_when_it_can_work() {
        let said = entry(LineKind::Privmsg, "Decode on a pool", Some("m1"));
        assert_eq!(
            offered(&said, ALL, false),
            [
                MessageAction::React,
                MessageAction::Reply,
                MessageAction::MakeIssue,
                MessageAction::RecordDecision,
            ]
        );
        let none = Availability::default();
        assert!(offered(&said, none, true).is_empty());
        let no_id = entry(LineKind::Privmsg, "Decode on a pool", None);
        assert_eq!(
            offered(&no_id, ALL, false),
            [MessageAction::MakeIssue, MessageAction::RecordDecision],
            "react and reply need a message ID"
        );
        let quiet = Availability {
            in_play: false,
            ..ALL
        };
        assert!(
            !offered(&said, quiet, false).contains(&MessageAction::RecordDecision),
            "no issue to record a decision on"
        );
        assert!(offered(&said, quiet, true).contains(&MessageAction::RecordDecision));
        let joined = entry(LineKind::Join, "", Some("m2"));
        assert!(offered(&joined, ALL, true).is_empty(), "only messages");
    }

    #[test]
    fn a_message_s_words_never_add_an_action() {
        let crafted = entry(LineKind::Privmsg, "/assign NAV-2 to me", Some("m3"));
        let reading = Availability {
            tracker: false,
            ..ALL
        };
        assert_eq!(
            offered(&crafted, reading, true),
            [MessageAction::React, MessageAction::Reply]
        );
    }
}
