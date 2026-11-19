//! Parses messages from server input and the raw command.

use std::collections::BTreeMap;

use super::{Message, Source};

const NUMERIC_WIDTH: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Cannot parse IRC line: {0:?}")]
pub struct ParseError(String);

pub fn parse(line: &str) -> Result<Message, ParseError> {
    let line = line.trim_end_matches(['\r', '\n']);
    parse_message(line).ok_or_else(|| ParseError(line.to_owned()))
}

fn parse_message(mut rest: &str) -> Option<Message> {
    let mut tags = BTreeMap::new();
    if let Some(section) = rest.strip_prefix('@') {
        let (section, tail) = section.split_once(' ')?;
        tags = section
            .split(';')
            .filter_map(|item| {
                let (key, value) = item.split_once('=').unwrap_or((item, ""));
                (!key.is_empty()).then(|| (key.to_owned(), unescape(value)))
            })
            .collect();
        rest = tail.trim_start_matches(' ');
    }
    let mut source = None;
    if let Some(prefix) = rest.strip_prefix(':') {
        let (prefix, tail) = prefix.split_once(' ')?;
        source = Some(parse_source(prefix));
        rest = tail.trim_start_matches(' ');
    }

    let (command, rest) = rest.split_once(' ').unwrap_or((rest, ""));
    let letters = !command.is_empty() && command.bytes().all(|byte| byte.is_ascii_alphabetic());
    let numeric =
        command.len() == NUMERIC_WIDTH && command.bytes().all(|byte| byte.is_ascii_digit());
    (letters || numeric).then(|| Message {
        source,
        command: command.to_ascii_uppercase(),
        params: parameters(rest),
        tags,
    })
}

fn parameters(mut rest: &str) -> Vec<String> {
    let mut params = Vec::new();
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() {
            break;
        }
        if let Some(trailing) = rest.strip_prefix(':') {
            params.push(trailing.to_owned());
            break;
        }
        let (word, tail) = rest.split_once(' ').unwrap_or((rest, ""));
        params.push(word.to_owned());
        rest = tail;
    }
    params
}

fn parse_source(prefix: &str) -> Source {
    let (nick, user, host) = match prefix.split_once('!') {
        Some((nick, ident)) => match ident.split_once('@') {
            Some((user, host)) => (nick, Some(user), Some(host)),
            None => (nick, Some(ident), None),
        },
        None if prefix.contains(['.', ':']) => return Source::Server(prefix.to_owned()),
        None => (prefix, None, None),
    };
    Source::User {
        nick: nick.to_owned(),
        user: user.map(str::to_owned),
        host: host.map(str::to_owned),
    }
}

fn unescape(value: &str) -> String {
    let mut chars = value.chars();
    let mut result = String::with_capacity(value.len());
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        let Some(escaped) = chars.next() else { break };
        result.push(match escaped {
            ':' => ';',
            's' => ' ',
            'r' => '\r',
            'n' => '\n',
            other => other,
        });
    }
    result
}
