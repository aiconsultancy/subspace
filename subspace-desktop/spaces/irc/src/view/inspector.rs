//! The inspector of a fleet channel, with tabs (the owner's exception in
//! the design guide's *Inspector tabs*): a ship's channel has **Ship**,
//! **Thread** and **People**, a Starbase **Starbase**, **Thread** and
//! **People**, every tab about the channel in front, and three the most the
//! shell draws. Other conversations keep the member list under its title,
//! without tabs.
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
use subspace_client_irc::Target;
use subspace_framework::dashboard::workspace::{self, InspectorTab as ShellTab};
use subspace_framework::dashboard::{Tokens, feedback};

use crate::fleet::{ChannelKind, Roster};
use crate::live::{self, FleetLive};
use crate::members::MemberList;
use crate::{Irc, dock};

pub(crate) mod crew;
mod play;
mod reason;
mod ship;
mod starbase;
#[cfg(test)]
mod tests;
pub(crate) mod thread;

#[cfg(test)]
pub(crate) use reason::reason;

/// The inspector's open tab for a fleet channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InspectorTab {
    /// The channel's ship, or its fleet for a Starbase.
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
}

impl Subject {
    /// The subject of a channel of `kind`; `None` for the command channel.
    pub(crate) fn of(kind: &ChannelKind) -> Option<Self> {
        match kind {
            ChannelKind::Ship { captain } => Some(Self::Ship {
                captain: captain.clone(),
            }),
            ChannelKind::Starbase => Some(Self::Starbase),
            ChannelKind::Command => None,
        }
    }

    /// The subject tab's label.
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Ship { .. } => "Ship",
            Self::Starbase => "Starbase",
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
    let subject = Subject::of(roster.channel(channel)?)?;
    let fleet = live::fleet_of(&hub.fleets, &roster, channel).cloned();
    Some(Front {
        subject,
        channel: channel.clone(),
        roster,
        fleet,
    })
}

/// Asks for the answers of what the Ship tab's **In play here** lists.
pub(crate) fn want_in_play(board: &Irc, cx: &mut Context<Irc>) {
    play::want(board, cx);
}

/// The inspector: a fleet channel's ship or Starbase and its people under
/// tabs; else the focused channel's members under their count, or a hint
/// for a buffer that has none.
pub(super) fn inspector(board: &Irc, cx: &mut Context<Irc>) -> workspace::Inspector {
    tabbed(board, cx).unwrap_or_else(|| members_only(board, cx))
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
        .on_close(
            "Hide members",
            cx.listener(|board, _, _, cx| dock::toggle_members(board, cx)),
        )
        .child(body);
    let inspector = if board.inspector_tab == InspectorTab::Subject {
        inspector.track_selected_tab(&board.inspector_crew_focus.ship_tab)
    } else {
        inspector
    };
    Some(match footer {
        Some(footer) => inspector.footer(footer),
        None => inspector,
    })
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

/// The subject tab: the ship or the Starbase. Like every tab, it starts
/// at the inspector body's own text edge, with no inset of its own.
fn subject_body(board: &Irc, front: &Front, cx: &mut Context<Irc>) -> AnyElement {
    let content = match &front.subject {
        Subject::Ship { captain } => ship::body(board, front, captain, cx),
        Subject::Starbase => starbase::body(front, cx),
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

/// The state of `fleet` as the inspector reads it: a fleet the board does
/// not follow, or that the service does not know, is not configured.
fn state_of(fleet: Option<&FleetLive>) -> FleetState {
    fleet.map_or(FleetState::NotConfigured, |fleet| fleet.state.clone())
}
