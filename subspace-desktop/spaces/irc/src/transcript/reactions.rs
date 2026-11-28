//! A message's reactions, as a row of small pills under it: each reaction
//! and its count, in the order the client's book gives. Ours are told
//! apart by the pill's surface (latched) and its semibold text, never by a
//! line on one edge. A tooltip and the accessible name say who reacted.
//!
//! Selecting a pill adds or takes back our reaction. The row ends with
//! **Add reaction**, a menu of a short fixed set of reactions
//! ([`crate::reactions::choices`]) and, on a running request to a verified
//! agent or that agent's reply, the stop group under `Stop <ship>`. Every
//! selection goes through [`reacting::toggle`] and its one rule
//! ([`crate::reactions::gate`]): a new ✋ or 🛑 of ours is sent only from
//! that labelled menu item, never from a pill or the `+N` menu, whether or
//! not the app recognises the message as an agent's; elsewhere selecting
//! someone else's stop reaction says where the deliberate stop is.
//!
//! Without reaction support, or on a message without a `msgid`, the pills
//! stay as read-only text and there is nothing to add. A reply that only
//! appears in the inspector keeps its reaction controls there.
//!
//! Every name and value comes from the network: values are bounded by the
//! client and cleaned for display ([`shown_value`]), names are cleaned and
//! cut ([`crate::thread_status::agent_name`]), and both are rendered as
//! plain text. What is sent back is always the exact value held.

use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyElement, FontWeight, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity,
    div,
};
use subspace_client_irc::reactions::ReactionSummary;
use subspace_client_irc::reactions::agent_stop::is_agent_stop_reaction;
use subspace_framework::dashboard::Tone;
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::anchored;
use subspace_framework::dashboard::badge::Pill;
use subspace_framework::dashboard::panel::IconName;

use super::RowContext;
use crate::conversation::{Conversation, reacting};
use crate::reactions::choices::{Choice, Choices, choices};
use crate::reactions::stop::AgentMessage;
use crate::state::Entry;
use crate::thread_status::{agent_name, invisible};

mod thread;

pub(crate) use thread::thread_reply;

/// Pills shown before the rest go behind `+N`.
pub(crate) const SHOWN_PILLS: usize = 10;
/// People named in a pill's description before `and N others`.
const NAMED: usize = 8;

/// What a pill's click does, when it does anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PillAction {
    /// Add ours, or take ours back.
    Toggle { add: bool },
    /// A stop reaction we have not made: say where the deliberate stop is
    /// instead of sending one.
    Explain,
    /// Read-only: no support, no `msgid`, or the Thread tab.
    None,
}

/// What selecting `summary`'s pill does on a message that `can_react`. It
/// only shapes the control; [`reacting::toggle`] decides what is sent.
pub(crate) fn pill_action(summary: &ReactionSummary, can_react: bool) -> PillAction {
    let mine = summary.mine();
    match can_react {
        false => PillAction::None,
        true if is_agent_stop_reaction(&summary.reaction) && !mine => PillAction::Explain,
        true => PillAction::Toggle { add: !mine },
    }
}

/// A reaction's value as it is shown: without spaces, control characters
/// or the zero-width, joining-control and direction characters that would
/// let it hide text or read as other words (`🛑 from you`, or text turned
/// round by U+202E). The zero-width joiner stays, as emoji sequences need
/// it.
pub(crate) fn shown_value(reaction: &str) -> String {
    reaction
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .filter(|c| *c == '\u{200d}' || !invisible(*c))
        .collect()
}

/// `alice, bob and you`, at most [`NAMED`] names, then how many others.
pub(crate) fn who(summary: &ReactionSummary) -> String {
    let mut names: Vec<String> = summary
        .reactors
        .iter()
        .filter(|reactor| !reactor.own)
        .take(NAMED)
        .map(|reactor| agent_name(&reactor.nick))
        .collect();
    let others = summary.count() - names.len() - usize::from(summary.mine());
    if summary.mine() {
        names.push("you".to_owned());
    }
    if others > 0 {
        names.push(format!(
            "{others} other{}",
            if others == 1 { "" } else { "s" }
        ));
    }
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// A pill's tooltip and accessible name: who reacted, and on an agent
/// message what a stop reaction asked, never that the agent stopped.
pub(crate) fn describe(summary: &ReactionSummary, agent: Option<&AgentMessage>) -> String {
    let base = format!("{} from {}", shown_value(&summary.reaction), who(summary));
    match agent.filter(|_| is_agent_stop_reaction(&summary.reaction)) {
        Some(agent) => format!(
            "{base}. It asked {} to stop; the status line shows whether it did.",
            agent.ship
        ),
        None => base,
    }
}

/// The reactions row under a transcript message, if it has reactions.
pub(crate) fn row(entry: &Entry, context: &RowContext) -> Option<AnyElement> {
    let summaries = context.reactions.get(&entry.seq)?;
    let agent = context.stops.get(&entry.seq);
    let msgid = entry.line.id.clone().filter(|_| context.can_react);
    let (shown, hidden) = summaries.split_at(summaries.len().min(SHOWN_PILLS));
    let pills = shown.iter().enumerate().map(|(index, summary)| {
        let id = format!("irc-line:{}:reaction:{index}", entry.seq);
        pill(id, summary, agent, msgid.as_deref(), context)
    });
    let overflow = (!hidden.is_empty()).then(|| {
        overflow(
            (entry.seq, agent),
            hidden,
            msgid.as_deref(),
            &context.conversation,
        )
    });
    let add = msgid.as_ref().map(|msgid| {
        let ship = agent
            .filter(|agent| agent.running)
            .map(|agent| agent.ship.as_str());
        add_control(
            entry.seq,
            choices(summaries, ship),
            msgid,
            &context.conversation,
        )
    });
    Some(frame(
        entry.seq,
        "irc-line",
        pills.chain(overflow).chain(add),
    ))
}

/// The same pills read-only, for the inspector's Thread tab.
pub(crate) fn read_only(entry: &Entry, context: &RowContext) -> Option<AnyElement> {
    let summaries = context.reactions.get(&entry.seq)?;
    let agent = context.stops.get(&entry.seq);
    let (shown, hidden) = summaries.split_at(summaries.len().min(SHOWN_PILLS));
    let pills = shown.iter().enumerate().map(|(index, summary)| {
        let id = format!("irc-thread-line:{}:reaction:{index}", entry.seq);
        text_pill(id, summary, &describe(summary, agent))
    });
    let more = (!hidden.is_empty()).then(|| {
        let words = hidden
            .iter()
            .map(|summary| describe(summary, agent))
            .collect::<Vec<_>>();
        let id = format!("irc-thread-line:{}:reactions-more", entry.seq);
        tipped(
            id,
            words.join("\n"),
            Pill::new("more", format!("+{}", hidden.len())),
        )
    });
    Some(frame(entry.seq, "irc-thread-line", pills.chain(more)))
}

fn frame(seq: i64, prefix: &str, children: impl Iterator<Item = AnyElement>) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("{prefix}:{seq}:reactions")))
        .test_support()
        .role(Role::Group)
        .aria_label("Reactions")
        .flex_wrap()
        .gap_1()
        .pt_0p5()
        .children(children)
        .into_any_element()
}

/// One pill: a latched button when selecting it does something, else
/// read-only text.
fn pill(
    id: String,
    summary: &ReactionSummary,
    agent: Option<&AgentMessage>,
    msgid: Option<&str>,
    context: &RowContext,
) -> AnyElement {
    let words = describe(summary, agent);
    let action = pill_action(summary, msgid.is_some());
    let Some(msgid) = msgid.filter(|_| action != PillAction::None) else {
        return text_pill(id, summary, &words);
    };
    let label = format!("{} {}", shown_value(&summary.reaction), summary.count());
    let button = action_button(SharedString::from(id), Intent::Neutral, None, label)
        .latched(summary.mine())
        .tooltip(words.clone())
        .accessibility_label(words);
    let button = if summary.mine() {
        button.font_weight(FontWeight::SEMIBOLD)
    } else {
        button
    };
    let (conversation, msgid) = (context.conversation.clone(), msgid.to_owned());
    let (reaction, add) = (summary.reaction.clone(), !summary.mine());
    button
        .on_click(move |_, _, cx| {
            let (msgid, reaction) = (msgid.clone(), reaction.clone());
            _ = conversation.update(cx, |this, cx| {
                reacting::toggle(this, (msgid, reaction), add, None, cx);
            });
        })
        .into_any_element()
}

/// A read-only pill with its description as tooltip and name; ours in
/// the accent's text.
fn text_pill(id: String, summary: &ReactionSummary, words: &str) -> AnyElement {
    let label = format!("{} {}", shown_value(&summary.reaction), summary.count());
    let pill =
        Pill::new(SharedString::from(format!("{id}:pill")), label).aria_label(words.to_owned());
    let pill = if summary.mine() {
        pill.hue(Tone::Accent)
    } else {
        pill
    };
    tipped(id, words.to_owned(), pill)
}

fn tipped(id: String, words: String, pill: Pill) -> AnyElement {
    let tip = SharedString::from(words);
    div()
        .id(SharedString::from(id))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .child(pill)
        .into_any_element()
}

/// `+N`: a menu of the reactions past [`SHOWN_PILLS`], each selected as
/// its pill would be, through [`reacting::toggle`], so a stop reaction in
/// it is explained, never sent.
fn overflow(
    (seq, agent): (i64, Option<&AgentMessage>),
    hidden: &[ReactionSummary],
    msgid: Option<&str>,
    conversation: &WeakEntity<Conversation>,
) -> AnyElement {
    let words: Vec<String> = hidden
        .iter()
        .map(|summary| describe(summary, agent))
        .collect();
    let label = format!("+{}", hidden.len());
    let button = action_button(
        SharedString::from(format!("irc-line:{seq}:reactions-more")),
        Intent::Quiet,
        None,
        label,
    )
    .accessibility_label(format!("{} more reactions", hidden.len()))
    .tooltip(words.join("\n"));
    let entries: Vec<Choice> = hidden
        .iter()
        .map(|summary| Choice {
            reaction: summary.reaction.clone(),
            name: "",
            mine: summary.mine(),
            own: summary
                .mine()
                .then(|| summary.reaction.clone())
                .into_iter()
                .collect(),
        })
        .collect();
    let (conversation, msgid) = (conversation.clone(), msgid.map(str::to_owned));
    anchored::dropdown_menu(button, move |menu, _, _| {
        entries.iter().fold(menu, |menu, choice| {
            let item = PopupMenuItem::new(shown_value(&choice.reaction)).checked(choice.mine);
            match &msgid {
                Some(msgid) => menu.item(on_choose(item, choice, msgid, None, &conversation)),
                None => menu.item(item.disabled(true)),
            }
        })
    })
    .into_any_element()
}

/// **Add reaction** at the end of the row.
fn add_control(
    seq: i64,
    choices: Choices,
    msgid: &str,
    conversation: &WeakEntity<Conversation>,
) -> AnyElement {
    let button = icon_button(
        SharedString::from(format!("irc-line:{seq}:add-reaction")),
        Intent::Quiet,
        IconName::FaceSlightlySmilingPlus,
        "Add reaction",
    );
    let (conversation, msgid) = (conversation.clone(), msgid.to_owned());
    anchored::dropdown_menu(button, move |menu, _, _| {
        menu_items(menu, &choices, &msgid, &conversation)
    })
    .into_any_element()
}

/// The reaction choices as menu items: the common set, then the stop
/// group apart under its plain label. A checked item is ours; choosing it
/// takes ours back.
pub(crate) fn menu_items(
    menu: PopupMenu,
    choices: &Choices,
    msgid: &str,
    conversation: &WeakEntity<Conversation>,
) -> PopupMenu {
    let menu = choices.common.iter().fold(menu, |menu, choice| {
        let item = PopupMenuItem::new(choice.label()).checked(choice.mine);
        menu.item(on_choose(item, choice, msgid, None, conversation))
    });
    let Some(group) = &choices.stop else {
        return menu;
    };
    let menu = menu.separator().label(group.label());
    group.choices.iter().fold(menu, |menu, choice| {
        let item = PopupMenuItem::new(choice.label()).checked(choice.mine);
        let ship = Some(group.ship.clone());
        menu.item(on_choose(item, choice, msgid, ship, conversation))
    })
}

fn on_choose(
    item: PopupMenuItem,
    choice: &Choice,
    msgid: &str,
    ship: Option<String>,
    conversation: &WeakEntity<Conversation>,
) -> PopupMenuItem {
    let conversation = conversation.clone();
    let (msgid, sends) = (msgid.to_owned(), choice.sends());
    item.on_click(move |_, _, cx| {
        _ = conversation.update(cx, |this, cx| {
            for (reaction, add) in &sends {
                let pair = (msgid.clone(), reaction.clone());
                reacting::toggle(this, pair, *add, ship.clone(), cx);
            }
        });
    })
}
