//! Initiative autojoin follows the selected channel set. Manual joins and
//! their reconnect state remain independent of that set.

use std::collections::BTreeSet;

use super::isupport::Casemapping;
use super::{Output, Session};
use crate::model::{BufferKey, Event, NetworkConfig, Target};
use crate::proto::Message;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Autojoin {
    initiative: Option<Vec<String>>,
    manual: BTreeSet<String>,
    retired: BTreeSet<String>,
    last_mapping: Option<Casemapping>,
}

impl Autojoin {
    pub(super) fn mapping(&self) -> Casemapping {
        self.last_mapping.unwrap_or(Casemapping::Ascii)
    }

    pub(super) fn remember_mapping(&mut self, mapping: Casemapping) {
        self.last_mapping = Some(mapping);
    }

    pub(super) fn entries<'a>(
        &'a self,
        local: &'a [String],
        fold: &impl Fn(&str) -> String,
    ) -> Vec<&'a String> {
        let Some(names) = &self.initiative else {
            return local.iter().collect();
        };
        names
            .iter()
            .map(|name| local_entry(local, name, fold))
            .collect()
    }

    /// Replaces the selected set and returns old-only, non-manual channels.
    pub(super) fn set(
        &mut self,
        config: &NetworkConfig,
        channels: Option<Vec<String>>,
        mapping: Casemapping,
    ) -> Vec<String> {
        self.remember_mapping(mapping);
        let fold = |name: &str| mapping.fold(name);
        let previous = self
            .entries(&config.channels, &fold)
            .into_iter()
            .map(|entry| name(entry).to_owned())
            .collect::<Vec<_>>();
        self.initiative = channels;
        let next = self.keys(&config.channels, &fold);
        let manual = self
            .manual
            .iter()
            .map(|name| fold(name))
            .collect::<BTreeSet<_>>();
        let removed: Vec<String> = previous
            .into_iter()
            .filter(|channel| {
                let key = fold(channel);
                !next.contains(&key) && !manual.contains(&key)
            })
            .collect();
        self.retired
            .retain(|channel| !next.contains(&fold(channel)));
        self.retired
            .extend(removed.iter().map(|channel| channel.to_ascii_lowercase()));
        removed
    }

    fn keys(&self, local: &[String], fold: &impl Fn(&str) -> String) -> BTreeSet<String> {
        self.entries(local, fold)
            .into_iter()
            .map(|entry| fold(name(entry)))
            .collect()
    }

    pub(super) fn is_retired(&self, channel: &str, fold: &impl Fn(&str) -> String) -> bool {
        let key = fold(channel);
        self.retired.iter().any(|name| fold(name) == key)
    }

    pub(super) fn record_manual(&mut self, message: &Message, fold: &impl Fn(&str) -> String) {
        let joining = match message.command.as_str() {
            "JOIN" => true,
            "PART" => false,
            _ => return,
        };
        for channel in message.param(0).split(',') {
            let key = fold(channel);
            self.manual.retain(|name| fold(name) != key);
            if joining {
                self.retired.retain(|name| fold(name) != key);
                self.manual.insert(channel.to_ascii_lowercase());
            }
        }
    }
}

pub(super) fn name(entry: &str) -> &str {
    entry.split_whitespace().next().unwrap_or_default()
}

fn local_entry<'a>(
    local: &'a [String],
    channel: &'a String,
    fold: &impl Fn(&str) -> String,
) -> &'a String {
    let key = fold(channel);
    local
        .iter()
        .find(|entry| fold(name(entry)) == key)
        .unwrap_or(channel)
}

pub(super) fn replace(session: &mut Session, channels: Option<Vec<String>>, out: &mut Output) {
    let mapping = if session.reg.registered {
        session.isupport.casemapping
    } else {
        session.autojoin.mapping()
    };
    let fold = |name: &str| mapping.fold(name);
    let mut before = session.autojoin.keys(&session.config.channels, &fold);
    before.extend(
        session
            .channels
            .values()
            .filter(|channel| !session.autojoin.is_retired(&channel.name, &fold))
            .map(|channel| fold(&channel.name)),
    );
    let retired = session.autojoin.set(&session.config, channels, mapping);
    session
        .rejoin
        .retain(|channel| !session.autojoin.is_retired(channel, &fold));
    out.events.extend(retire_events(&session.config, &retired));
    if session.registered() {
        out.send.extend(
            retired
                .iter()
                .map(|channel| Message::new("PART", [channel])),
        );
        out.send
            .extend(super::registration::additions(session, &before));
    }
}

pub(super) fn retire_events(config: &NetworkConfig, retired: &[String]) -> Vec<Event> {
    retired
        .iter()
        .map(|channel| {
            Event::AutojoinRetired(BufferKey {
                network: config.id.clone(),
                target: Target::Channel(channel.clone()),
            })
        })
        .collect()
}
