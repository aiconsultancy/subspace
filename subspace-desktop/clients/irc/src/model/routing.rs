//! The network a command goes to and an event comes from, so a client can
//! route or fence them by network.

use super::{Command, Event, NetworkId};

impl Command {
    /// The network the command is for; `None` for [`Command::Configure`],
    /// which concerns them all.
    pub fn network(&self) -> Option<&NetworkId> {
        match self {
            Command::Connect(network)
            | Command::Disconnect { network, .. }
            | Command::SetAutojoin { network, .. }
            | Command::Join { network, .. }
            | Command::Query { network, .. }
            | Command::Anchors { network, .. }
            | Command::ListChannels { network } => Some(network),
            Command::Input { buffer, .. }
            | Command::Say { buffer, .. }
            | Command::SendAll { buffer, .. }
            | Command::Part { buffer, .. }
            | Command::Forget { buffer }
            | Command::LoadHistory { buffer, .. }
            | Command::LoadAround { buffer, .. }
            | Command::MarkRead { buffer, .. }
            | Command::Typing { buffer, .. }
            | Command::Reply { buffer, .. }
            | Command::React { buffer, .. }
            | Command::Unreact { buffer, .. } => Some(&buffer.network),
            Command::Configure(_) => None,
        }
    }
}

impl Event {
    /// The network the event comes from.
    pub fn network(&self) -> &NetworkId {
        match self {
            Event::Status { network, .. }
            | Event::Capabilities { network, .. }
            | Event::OwnNick { network, .. }
            | Event::NickStatus { network, .. }
            | Event::ChannelList { network, .. } => network,
            Event::Line(line) | Event::Replayed(line) => &line.buffer.network,
            Event::AutojoinRetired(buffer)
            | Event::History { buffer, .. }
            | Event::Backfill { buffer, .. }
            | Event::Joined { buffer }
            | Event::Left { buffer }
            | Event::Topic { buffer, .. }
            | Event::Members { buffer, .. }
            | Event::MemberJoined { buffer, .. }
            | Event::MemberLeft { buffer, .. }
            | Event::MemberChanged { buffer, .. }
            | Event::Typing { buffer, .. }
            | Event::ReadMarker { buffer, .. }
            | Event::Sent { buffer, .. } => &buffer.network,
        }
    }
}
