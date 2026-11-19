//! The socket-independent state machine for one IRC connection.

mod autojoin;
mod batch;
mod caps;
mod channel_info;
mod channels;
mod commands;
mod describe;
mod dispatch;
mod history;
mod input;
mod isupport;
mod lines;
mod listing;
mod member_state;
mod membership;
mod messages;
mod metadata;
mod numerics;
mod registration;
mod send;
mod send_all;
mod spelling;
mod split;

#[cfg(test)]
mod cleanroom_acceptance;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::{DateTime, Utc};

use crate::model::{Anchor, BufferKey, Command, Event, NetworkConfig, NetworkId, Target};
use crate::proto::Message;

pub use input::{COMMANDS, CommandHelp, FirstArgument};
pub use send::{
    MAX_INLINE_BYTES, check_draft, check_inline_text, connection_batches, draft_line_limit,
    message_budget,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carry {
    pub rejoin: Vec<String>,
    pub spellings: Vec<String>,
    pub left: Vec<String>,
    pub anchors: Vec<Anchor>,
    autojoin: autojoin::Autojoin,
}

impl Carry {
    pub fn forget(&mut self, command: &Command) {
        match command {
            Command::Anchors { anchors, .. } => {
                for anchor in anchors {
                    history::remember(&mut self.anchors, anchor.clone());
                }
            }
            Command::Forget { buffer }
                if let Target::Channel(name) = &buffer.target
                    && !self.left.iter().any(|left| left.eq_ignore_ascii_case(name)) =>
            {
                self.left.push(name.clone());
            }
            _ => {}
        }
    }
}

pub(crate) fn configure_autojoin(
    carry: &mut Carry,
    config: &NetworkConfig,
    command: &Command,
) -> Vec<Event> {
    let Command::SetAutojoin { channels, .. } = command else {
        return Vec::new();
    };
    let mapping = carry.autojoin.mapping();
    let fold = |name: &str| mapping.fold(name);
    let mut retired = carry.autojoin.set(config, channels.clone(), mapping);
    carry
        .rejoin
        .retain(|name| !carry.autojoin.is_retired(name, &fold));
    for name in &mut retired {
        let folded = fold(name);
        if let Some(spelling) = carry
            .spellings
            .iter()
            .find(|spelling| fold(spelling) == folded)
        {
            name.clone_from(spelling);
        }
    }
    autojoin::retire_events(config, &retired)
}

#[derive(Debug, Default, PartialEq)]
pub struct Output {
    pub send: Vec<Message>,
    pub events: Vec<Event>,
    pub written: Vec<Event>,
    pub unwritten: Vec<Event>,
    pub quit: bool,
    pub(crate) fatal: Option<String>,
}

impl Output {
    fn send(&mut self, message: Message) {
        self.send.push(message);
    }

    fn event(&mut self, event: Event) {
        self.events.push(event);
    }
}

pub struct Session {
    config: NetworkConfig,
    reg: registration::Registration,
    caps: caps::Caps,
    isupport: isupport::Isupport,
    channels: BTreeMap<String, channels::Channel>,
    queries: BTreeSet<String>,
    history: history::Requests,
    anchors: Vec<Anchor>,
    listing: listing::Listing,
    statuses: metadata::Statuses,
    rejoin: Vec<String>,
    autojoin: autojoin::Autojoin,
    left: Vec<String>,
    spellings: spelling::Spellings,
    sent_batches: u64,
    now: DateTime<Utc>,
    batches: VecDeque<batch::Batch>,
    ctcp_replies: VecDeque<(DateTime<Utc>, String)>,
}

impl Session {
    pub fn new(config: NetworkConfig, carry: Carry) -> Self {
        let mut session = Self {
            reg: registration::Registration::new(config.nickname.clone()),
            config,
            caps: caps::Caps::default(),
            isupport: isupport::Isupport::default(),
            channels: BTreeMap::new(),
            queries: BTreeSet::new(),
            history: history::Requests::default(),
            anchors: carry.anchors,
            listing: listing::Listing::default(),
            statuses: metadata::Statuses::default(),
            rejoin: carry.rejoin,
            autojoin: carry.autojoin,
            left: carry.left,
            spellings: spelling::Spellings::default(),
            sent_batches: 0,
            now: Utc::now(),
            batches: VecDeque::new(),
            ctcp_replies: VecDeque::new(),
        };
        session.refold(carry.spellings);
        session
    }

    pub fn into_carry(mut self) -> Carry {
        let mapping = self.isupport.casemapping;
        if self.registered() {
            self.autojoin.remember_mapping(mapping);
        }
        let rejoin = self
            .channel_names()
            .into_iter()
            .filter(|name| !self.autojoin.is_retired(name, &|name| mapping.fold(name)))
            .collect();
        Carry {
            rejoin,
            spellings: self.spellings.names(),
            left: self.left,
            anchors: self.anchors,
            autojoin: self.autojoin,
        }
    }

    pub fn abandon_history(&mut self) -> Vec<Event> {
        let mut events = history::abandon(self);
        self.canonicalise(&mut events);
        events
    }

    pub fn abandon(&mut self) -> Vec<Event> {
        let mut events = self.abandon_history();
        events.extend(metadata::clear_all(self));
        events
    }

    pub fn tick(&mut self, now: DateTime<Utc>) -> Output {
        self.now = now;
        let mut out = Output::default();
        caps::expire_sasl(self, &mut out);
        metadata::sync_due(self, &mut out);
        out
    }

    pub fn start(&mut self, now: DateTime<Utc>) -> Output {
        self.now = now;
        self.caps.open();
        Output {
            send: registration::start(&self.config),
            ..Output::default()
        }
    }

    pub fn receive(&mut self, message: Message, now: DateTime<Utc>) -> Output {
        self.now = now;
        let mut out = Output::default();
        batch::receive(self, message, &mut out);
        self.finish(out)
    }

    pub fn command(&mut self, command: Command, now: DateTime<Utc>) -> Output {
        self.now = now;
        let manual = !matches!(command, Command::SetAutojoin { .. });
        let mut out = Output::default();
        commands::handle(self, command, &mut out);
        if manual {
            let mapping = self.isupport.casemapping;
            for message in &out.send {
                self.autojoin
                    .record_manual(message, &|name| mapping.fold(name));
            }
        }
        self.finish(out)
    }

    fn finish(&mut self, mut out: Output) -> Output {
        self.canonicalise(&mut out.events);
        self.canonicalise(&mut out.written);
        self.canonicalise(&mut out.unwritten);
        history::track(&mut self.anchors, &out.events);
        out
    }

    pub fn written(&mut self, events: &[Event]) {
        history::track(&mut self.anchors, events);
    }

    pub fn channel_names(&self) -> Vec<String> {
        self.channels
            .values()
            .map(|channel| channel.name.clone())
            .collect()
    }

    pub fn network(&self) -> NetworkId {
        self.config.id.clone()
    }

    pub fn registered(&self) -> bool {
        self.reg.registered
    }

    fn fold(&self, name: &str) -> String {
        self.isupport.fold(name)
    }

    fn is_own(&self, nick: &str) -> bool {
        self.fold(nick) == self.fold(&self.reg.nick)
    }

    fn buffer(&self, target: Target) -> BufferKey {
        BufferKey {
            network: self.network(),
            target,
        }
    }

    fn has_cap(&self, cap: &str) -> bool {
        self.caps.enabled.contains(cap)
    }

    fn refold(&mut self, names: Vec<String>) {
        self.spellings
            .refold(names, |name| self.isupport.fold(name));
    }

    fn canonicalise(&mut self, events: &mut [Event]) {
        for buffer in events.iter_mut().flat_map(spelling::buffers_mut) {
            if let Some(name) = spelling::name_mut(&mut buffer.target) {
                *name = self.spellings.canonical(self.isupport.fold(name), name);
            }
        }
    }
}
