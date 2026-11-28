//! The inspector of a fleet channel. Its tabs show the channel, Thread and
//! People. Other conversations keep the member list under its title.
//!
//! What a ship or Starbase tab shows comes from the fleet's Fleet API as
//! the hub holds it ([`crate::live`]); while the fleet is not live the tab
//! says why in one sentence, in the fleet client's words, and never shows
//! a figure the API did not give.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _,
};
use subspace_client_fleet::FleetState;
use subspace_client_irc::{BufferKey, Target};
use subspace_framework::dashboard::workspace::{self, InspectorTab as ShellTab};
use subspace_framework::dashboard::{Tokens, feedback};

use crate::fleet::{ChannelKind, Roster};
use crate::live::{self, FleetLive};
use crate::members::MemberList;
use crate::{Irc, dock};

mod panel;
mod play;
mod reason;
mod ship;
mod starbase;
#[cfg(test)]
mod tests;
pub(crate) mod thread;
pub(crate) use panel::ShipPanel;

#[cfg(test)]
pub(crate) use reason::reason;

/// The inspector's open tab for a fleet channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InspectorTab {
    /// The channel, its ship or its fleet.
    #[default]
    Subject,
    /// The thread chosen in the conversation ([`crate::thread`]).
    Thread,
    People,
}

impl InspectorTab {
    const ALL: [Self; 3] = [Self::Subject, Self::Thread, Self::People];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or_default()
    }

    fn at(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// What a fleet channel's inspector is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Subject {
    /// A ship's channel, by its captain's roster nick.
    Ship {
        captain: String,
    },
    Starbase,
    Command,
    Audit,
}

impl Subject {
    fn of(kind: &ChannelKind) -> Self {
        match kind {
            ChannelKind::Ship { captain } => Self::Ship {
                captain: captain.clone(),
            },
            ChannelKind::Starbase => Self::Starbase,
            ChannelKind::Command => Self::Command,
            ChannelKind::Audit => Self::Audit,
        }
    }

    /// The subject tab's label.
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Ship { .. } => "Ship",
            Self::Starbase => "Starbase",
            Self::Command => "Command",
            Self::Audit => "Fleet audit",
        }
    }
}

/// The tabs of a fleet channel's inspector: its subject, its thread, then
/// its people with their count when it is known.
pub(crate) fn tabs(subject: &Subject, people: Option<usize>) -> Vec<ShellTab> {
    let people_tab = ShellTab::new("People");
    let people_tab = match people {
        Some(count) => people_tab.count(count.to_string()),
        None => people_tab,
    };
    vec![
        ShellTab::new(subject.label()),
        ShellTab::new("Thread"),
        people_tab,
    ]
}

/// The channel in front, its fleet's word and its roster, for a fleet
/// channel's inspector; `None` for every other conversation.
struct Front {
    subject: Subject,
    channel: String,
    buffer: BufferKey,
    fleet_id: Option<String>,
    roster: std::rc::Rc<Roster>,
    fleet: Option<FleetLive>,
}

fn front(board: &Irc, cx: &App) -> Option<Front> {
    let buffer = board.dock.focused.as_ref()?;
    let Target::Channel(channel) = &buffer.target else {
        return None;
    };
    let hub = board.hub.as_ref()?.read(cx);
    let roster = hub.roster(&buffer.network, cx);
    let subject = Subject::of(roster.channel(channel)?);
    let fleet_id = live::fleet_id(&roster, channel);
    let fleet = fleet_id.and_then(|id| hub.fleets.get(id)).cloned();
    Some(Front {
        subject,
        channel: channel.clone(),
        buffer: buffer.clone(),
        fleet_id: fleet_id.map(str::to_owned),
        roster,
        fleet,
    })
}

/// Records a complete read and schedules feedback if a refresh takes longer.
pub(crate) fn sync_ship(board: &mut Irc, cx: &mut Context<Irc>) {
    let ship = front(board, cx).filter(|front| matches!(front.subject, Subject::Ship { .. }));
    ShipPanel::sync(ship, board, cx);
}

/// Asks for the answers of what the Ship tab's **In play here** lists.
pub(crate) fn want_in_play(board: &Irc, cx: &mut Context<Irc>) {
    play::want(board, cx);
}

/// The inspector: a fleet channel and its people under tabs; else the focused channel's members under their count, or a hint
/// for a buffer that has none.
pub(super) fn inspector(board: &Irc, cx: &mut Context<Irc>) -> workspace::Inspector {
    tabbed(board, cx).unwrap_or_else(|| {
        let thread_open = board.inspector_tab == InspectorTab::Thread
            && board
                .dock
                .focused
                .as_ref()
                .is_some_and(|buffer| board.threads.shown(buffer).is_some());
        if thread_open {
            let inspector = workspace::Inspector::new("irc-inspector", "Thread")
                .on_close("Close inspector", cx.listener(close_tabbed))
                .child(thread::body(board, cx));
            match thread::footer(board, cx) {
                Some(footer) => inspector.footer(footer),
                None => inspector,
            }
        } else {
            members_only(board, cx)
        }
    })
}

/// The member list under its title, without tabs.
fn members_only(board: &Irc, cx: &mut Context<Irc>) -> workspace::Inspector {
    let members = dock::focused_members(board);
    let count = members.as_ref().map(|list| list.read(cx).count(cx));
    let body = match members {
        Some(list) => list.into_any_element(),
        None => no_channel(cx),
    };
    workspace::Inspector::new("irc-inspector", super::details::members_title(count))
        .on_close(
            "Hide members",
            cx.listener(|board, _, _, cx| dock::toggle_members(board, cx)),
        )
        .child(body)
}

/// What the inspector says for the server buffer and private conversations.
fn no_channel(cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let hint = "Open a channel to see its members.";
    feedback::empty(&tokens, "irc-members-none", "No channel", hint).into_any_element()
}

/// A fleet channel's inspector with its tabs; `None` for any other
/// conversation, which keeps the member list alone.
fn tabbed(board: &Irc, cx: &mut Context<Irc>) -> Option<workspace::Inspector> {
    let front = front(board, cx)?;
    let members = dock::focused_members(board);
    let count = members.as_ref().map(|list| list.read(cx).count(cx));
    let board_entity = cx.entity().downgrade();
    let body = match board.inspector_tab {
        InspectorTab::People => people(members, cx),
        InspectorTab::Thread => thread::body(board, cx),
        InspectorTab::Subject => subject_body(board, &front, cx),
    };
    let footer = match board.inspector_tab {
        InspectorTab::Thread => thread::footer(board, cx),
        _ => None,
    };
    let title = format!("{} details", front.channel);
    let inspector = workspace::Inspector::new("irc-inspector", title)
        .tabs(
            tabs(&front.subject, count),
            board.inspector_tab.index(),
            move |index, _, _, cx| {
                _ = board_entity.update(cx, |board, cx| {
                    board.inspector_tab = InspectorTab::at(index);
                    // Shown now: ask for what In play here lists.
                    want_in_play(board, cx);
                    cx.notify();
                });
            },
        )
        .on_close("Close inspector", cx.listener(close_tabbed))
        .child(body);
    Some(match footer {
        Some(footer) => inspector.footer(footer),
        None => inspector,
    })
}

fn close_tabbed(
    board: &mut Irc,
    _: &gpui_kit::ClickEvent,
    window: &mut gpui_kit::Window,
    cx: &mut Context<Irc>,
) {
    dock::toggle_members(board, cx);
    if let Some(hosted) = board
        .dock
        .focused
        .as_ref()
        .and_then(|buffer| board.dock.open.get(buffer))
    {
        hosted
            .entity
            .update(cx, |view, cx| view.focus_composer(window, cx));
    }
}

/// The People tab: the channel's member list, as without tabs.
fn people(members: Option<Entity<MemberList>>, cx: &App) -> AnyElement {
    match members {
        Some(list) => list.into_any_element(),
        None => {
            let tokens = Tokens::current(cx);
            feedback::empty(&tokens, "irc-members-none", "No members", "").into_any_element()
        }
    }
}

/// The channel tab. Like every tab, it starts
/// at the inspector body's own text edge, with no inset of its own.
fn subject_body(board: &Irc, front: &Front, cx: &App) -> AnyElement {
    let content = match &front.subject {
        Subject::Ship { captain } => ship::body(board, front, captain, cx),
        Subject::Starbase => starbase::body(front, cx),
        Subject::Command | Subject::Audit => channel_details(board, front, cx),
    };
    v_flex()
        .id("irc-inspector-subject")
        .test_support()
        .w_full()
        .min_w_0()
        .gap_4()
        .child(content)
        .into_any_element()
}

fn channel_details(board: &Irc, front: &Front, cx: &App) -> AnyElement {
    let topic = board
        .hub
        .as_ref()
        .zip(board.dock.focused.as_ref())
        .and_then(|(hub, buffer)| hub.read(cx).state.buffer(buffer))
        .and_then(|held| held.topic.as_ref())
        .map(|topic| subspace_client_irc::format::strip(&topic.text))
        .filter(|topic| !topic.trim().is_empty());
    let tokens = Tokens::current(cx);
    workspace::section(front.channel.clone(), cx)
        .child(feedback::empty_line(
            &tokens,
            "irc-inspector-topic",
            topic.as_deref().unwrap_or("No channel topic."),
        ))
        .into_any_element()
}

/// The state of `fleet` as the inspector reads it: a fleet the board does
/// not follow, or that the service does not know, is not configured.
fn state_of(fleet: Option<&FleetLive>) -> FleetState {
    fleet.map_or(FleetState::NotConfigured, |fleet| fleet.state.clone())
}
