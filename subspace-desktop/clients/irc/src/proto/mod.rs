//! Structured IRC messages shared by the wire codec and session.

use std::collections::BTreeMap;

pub mod ctcp;
mod parse;
mod serialise;
#[cfg(test)]
mod tests;

pub use parse::parse;
pub use serialise::serialise;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Server(String),
    User {
        nick: String,
        user: Option<String>,
        host: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub source: Option<Source>,
    pub command: String,
    pub params: Vec<String>,
    pub(crate) tags: BTreeMap<String, String>,
}

impl Message {
    pub fn new<S: Into<String>>(command: &str, params: impl IntoIterator<Item = S>) -> Self {
        Self {
            source: None,
            command: command.to_ascii_uppercase(),
            params: params.into_iter().map(Into::into).collect(),
            tags: BTreeMap::new(),
        }
    }

    pub fn with_tag(mut self, key: &str, value: &str) -> Self {
        self.tags.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn param(&self, index: usize) -> &str {
        self.params.get(index).map_or("", String::as_str)
    }

    pub fn trailing(&self) -> &str {
        self.params.last().map_or("", String::as_str)
    }

    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags.get(key).map(String::as_str)
    }

    pub fn nick(&self) -> Option<&str> {
        match &self.source {
            Some(Source::User { nick, .. }) => Some(nick),
            _ => None,
        }
    }
}
