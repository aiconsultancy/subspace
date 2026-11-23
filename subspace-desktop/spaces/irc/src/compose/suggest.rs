//! What the word at the cursor could become: an `@member`, a `/command`, a
//! command's first argument or a `#channel`. In a fleet channel, its own
//! commands ([`super::ask`]) come before the IRC client's. Pure: [`Facts`]
//! are gathered from the hub's state, and the result is shown in the shared
//! completion list above the composer.

use std::ops::Range;

use subspace_client_irc::{BufferKey, COMMANDS, CommandHelp, FirstArgument, Member, Target};
use subspace_framework::dashboard::chat_input::completion::{CompletionItem, word_before};

use crate::fleet::{Rank, Roster};
use crate::state::{IrcState, is_message};

/// Lines read back for who spoke recently.
const RECENT_LINES: usize = 500;
/// The most people one list offers; typing narrows the rest.
const PEOPLE_LIMIT: usize = 50;

/// Someone a draft can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Person {
    pub nick: String,
    /// The highest channel prefix, such as `@`.
    pub prefix: Option<char>,
    pub away: bool,
    /// The fleet rank the Space knows for the nick.
    pub rank: Option<Rank>,
    /// The member's server account, which must be the nick for a rank.
    pub account: Option<String>,
}

impl Person {
    fn named(nick: &str) -> Self {
        Self {
            nick: nick.to_owned(),
            prefix: None,
            away: false,
            rank: None,
            account: None,
        }
    }

    fn member(member: &Member) -> Self {
        Self {
            nick: member.nick.clone(),
            prefix: member.prefixes.chars().next(),
            away: member.away,
            rank: None,
            account: member.account.clone(),
        }
    }
}

/// What completion knows about a buffer, each list in the order offered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    /// The buffer's members, or the other person of a private conversation:
    /// recent speakers first, then by name, never ourselves.
    pub people: Vec<Person>,
    /// Everyone on the network, in the same order, for command arguments.
    pub everyone: Vec<Person>,
    /// The network's channels, by name.
    pub channels: Vec<String>,
    /// The channels we are in, by name.
    pub joined: Vec<String>,
    /// The agent `/ask` addresses in this channel, when it is a fleet's
    /// ship channel or Starbase.
    pub ask: Option<String>,
}

/// One choice: the row shown, and the text that replaces the word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Suggestion {
    pub item: CompletionItem,
    pub insert: String,
}

/// The choices for the word in `range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Offer {
    pub range: Range<usize>,
    pub choices: Vec<Suggestion>,
}

/// What the word at the cursor asks to complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger<'a> {
    Command(&'a str),
    Argument(FirstArgument, &'a str),
    Mention(&'a str),
    Channel(&'a str),
}

/// IRC's traditional case folding (RFC 1459), which servers apply to nicks
/// and channel names: letters ignore case, and `[]\~` equal `{}|^`. It
/// matches at least as much as the server's own casemapping.
pub(crate) fn fold(text: &str) -> String {
    text.chars()
        .flat_map(|character| match character {
            '[' => '{'.to_lowercase(),
            ']' => '}'.to_lowercase(),
            '\\' => '|'.to_lowercase(),
            '~' => '^'.to_lowercase(),
            other => other.to_lowercase(),
        })
        .collect()
}

/// The choices for the word that ends at `cursor` in `text`, if any.
pub(crate) fn suggest(text: &str, cursor: usize, facts: &Facts) -> Option<Offer> {
    let range = word_before(text, cursor);
    let trigger = trigger(&text[..range.start], &text[range.clone()])?;
    let choices = choices(trigger, facts);
    (!choices.is_empty()).then_some(Offer { range, choices })
}

fn trigger<'a>(before: &str, word: &'a str) -> Option<Trigger<'a>> {
    if before.is_empty() && word.starts_with('/') && !word.starts_with("//") {
        return Some(Trigger::Command(&word[1..]));
    }
    if let Some(command) = command_before(before) {
        return Some(Trigger::Argument(command.first, word));
    }
    if let Some(query) = word.strip_prefix('@') {
        return Some(Trigger::Mention(query));
    }
    word.starts_with('#').then_some(Trigger::Channel(word))
}

/// The command whose first argument is being typed: `before` is exactly
/// `/name` and a space.
fn command_before(before: &str) -> Option<&'static CommandHelp> {
    let name = before.strip_prefix('/')?.strip_suffix(' ')?;
    if name.contains(char::is_whitespace) {
        return None;
    }
    let name = name.to_ascii_lowercase();
    COMMANDS.iter().find(|command| command.answers(&name))
}

fn choices(trigger: Trigger<'_>, facts: &Facts) -> Vec<Suggestion> {
    match trigger {
        Trigger::Command(query) => commands(query, facts.ask.as_deref()),
        Trigger::Mention(query) => people(&facts.people, query, "@"),
        Trigger::Channel(query) => channels(&facts.joined, query),
        Trigger::Argument(FirstArgument::Nick, query) => {
            people(&facts.everyone, query.trim_start_matches('@'), "")
        }
        Trigger::Argument(FirstArgument::Channel, query) => channels(&facts.channels, query),
        Trigger::Argument(FirstArgument::JoinedChannel, query) => channels(&facts.joined, query),
        Trigger::Argument(_, _) => Vec::new(),
    }
}

/// Commands whose name or alias starts with `query`: the channel's own
/// first, when it has an agent to `ask`, then the IRC client's in
/// catalogue order.
fn commands(query: &str, ask: Option<&str>) -> Vec<Suggestion> {
    let query = query.to_ascii_lowercase();
    let own = ask
        .filter(|_| super::ask::NAME.starts_with(&query))
        .map(ask_suggestion);
    let client = COMMANDS
        .iter()
        .filter(|command| {
            std::iter::once(&command.name)
                .chain(command.aliases)
                .any(|name| name.starts_with(&query))
        })
        .map(|command| Suggestion {
            item: CompletionItem::command(
                format!("/{}", command.name),
                command.args,
                command.description,
            )
            .spoken(format!("{}, {}", command.usage(), command.description)),
            insert: format!("/{} ", command.name),
        });
    own.into_iter().chain(client).collect()
}

/// `/ask`, which addresses `agent`, as a command of this channel.
fn ask_suggestion(agent: &str) -> Suggestion {
    let description = super::ask::description(agent);
    let name = super::ask::NAME;
    let args = super::ask::ARGS;
    Suggestion {
        item: CompletionItem::command(format!("/{name}"), args, description.clone()).spoken(
            format!("/{name} {args}, {description}, a command of this channel"),
        ),
        insert: format!("/{name} "),
    }
}

/// People whose nick starts with `query`, ignoring case, each inserted
/// after `sigil` with a space to type on.
fn people(people: &[Person], query: &str, sigil: &str) -> Vec<Suggestion> {
    let query = fold(query);
    people
        .iter()
        .filter(|person| fold(&person.nick).starts_with(&query))
        .take(PEOPLE_LIMIT)
        .map(|person| Suggestion {
            item: person_item(person),
            insert: format!("{sigil}{} ", person.nick),
        })
        .collect()
}

/// A person's row: the channel prefix and the fleet rank as the trailing
/// mark, as the member list notes them, such as `@ · captain`.
fn person_item(person: &Person) -> CompletionItem {
    let rank = match person.prefix {
        Some('~' | '&' | '@' | '%') => ", operator",
        Some('+') => ", voiced",
        _ => "",
    };
    let fleet = person
        .rank
        .map(|rank| format!(", {}", rank.note()))
        .unwrap_or_default();
    let away = if person.away { ", away" } else { "" };
    let prefix = person.prefix.map(String::from);
    let mark: Vec<String> = prefix
        .into_iter()
        .chain(person.rank.map(|rank| rank.note().to_owned()))
        .collect();
    CompletionItem::person(person.nick.clone(), mark.join(" · "), person.away)
        .spoken(format!("{}{rank}{fleet}{away}", person.nick))
}

/// Channels whose name starts with `query`, with or without its `#`.
fn channels(names: &[String], query: &str) -> Vec<Suggestion> {
    let query = fold(query);
    names
        .iter()
        .filter(|name| {
            let name = fold(name);
            name.starts_with(&query) || name.trim_start_matches(['#', '&']).starts_with(&query)
        })
        .map(|name| Suggestion {
            item: CompletionItem::new(name.clone()),
            insert: format!("{name} "),
        })
        .collect()
}

impl Facts {
    /// What `state` holds for completing a draft in `buffer`.
    pub fn gather(state: &IrcState, buffer: &BufferKey) -> Self {
        let own = fold(state.nick(&buffer.network).unwrap_or_default());
        let recent = recent_speakers(state, buffer, &own);
        let (channels, joined) = channel_names(state, buffer);
        Self {
            people: ordered(buffer_people(state, buffer), &recent, &own),
            everyone: ordered(network_people(state, buffer), &recent, &own),
            channels,
            joined,
            ask: None,
        }
    }

    /// The same facts with the agent `/ask` addresses, when there is one.
    pub fn asking(mut self, agent: Option<&str>) -> Self {
        self.ask = agent.map(str::to_owned);
        self
    }

    /// The same facts with each person's fleet rank from `roster`.
    pub fn ranked(mut self, roster: &Roster) -> Self {
        for person in self.people.iter_mut().chain(self.everyone.iter_mut()) {
            person.rank = roster.verified(&person.nick, person.account.as_deref());
        }
        self
    }
}

/// Who spoke in `buffer`, newest first, each once, never ourselves.
fn recent_speakers(state: &IrcState, buffer: &BufferKey, own: &str) -> Vec<String> {
    let mut speakers: Vec<String> = Vec::new();
    let Some(held) = state.buffer(buffer) else {
        return speakers;
    };
    let lines = held.entries.iter().rev().take(RECENT_LINES);
    for nick in lines
        .filter(|entry| is_message(entry.line.kind))
        .filter_map(|entry| entry.line.from.as_deref())
    {
        let folded = fold(nick);
        if folded != own && !speakers.contains(&folded) {
            speakers.push(folded);
        }
    }
    speakers
}

fn buffer_people(state: &IrcState, buffer: &BufferKey) -> Vec<Person> {
    match &buffer.target {
        Target::Query(nick) => vec![Person::named(nick)],
        _ => state
            .buffer(buffer)
            .map(|held| held.members.iter().map(Person::member).collect())
            .unwrap_or_default(),
    }
}

/// Everyone in the network's channels and private conversations.
fn network_people(state: &IrcState, buffer: &BufferKey) -> Vec<Person> {
    let mut people: Vec<Person> = Vec::new();
    let held = state
        .buffers()
        .filter(|(key, _)| key.network == buffer.network);
    for (key, held) in held {
        let found = match &key.target {
            Target::Query(nick) => vec![Person::named(nick)],
            _ => held.members.iter().map(Person::member).collect(),
        };
        for person in found {
            let folded = fold(&person.nick);
            if !people.iter().any(|known| fold(&known.nick) == folded) {
                people.push(person);
            }
        }
    }
    people
}

/// `people` without ourselves: recent speakers first, newest first, then
/// everyone else by name.
fn ordered(mut people: Vec<Person>, recent: &[String], own: &str) -> Vec<Person> {
    people.retain(|person| fold(&person.nick) != own);
    people.sort_by_cached_key(|person| {
        let folded = fold(&person.nick);
        let spoke = recent.iter().position(|nick| *nick == folded);
        (spoke.unwrap_or(usize::MAX), folded)
    });
    people
}

/// The network's channels, and those we are in, by name.
fn channel_names(state: &IrcState, buffer: &BufferKey) -> (Vec<String>, Vec<String>) {
    let mut channels = Vec::new();
    let mut joined = Vec::new();
    for (key, held) in state.buffers() {
        let Target::Channel(name) = &key.target else {
            continue;
        };
        if key.network != buffer.network {
            continue;
        }
        channels.push(name.clone());
        if held.joined {
            joined.push(name.clone());
        }
    }
    channels.sort_by_key(|name| fold(name));
    joined.sort_by_key(|name| fold(name));
    (channels, joined)
}

#[cfg(test)]
#[path = "suggest_tests.rs"]
mod tests;
