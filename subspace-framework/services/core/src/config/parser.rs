use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{Config, Notice, NoticeKind, Secret, is_repo, show};
use crate::features;

mod acp;
mod application;
mod mcp;
mod web;
mod web_boards;

pub(super) fn parse(
    text: &str,
    file: &std::path::Path,
    renames: &crate::former_ids::Renames,
) -> (Config, Vec<Notice>) {
    let mut parser = Parser {
        file,
        notices: Vec::new(),
    };
    let config = parser.config(text, renames);
    (config, parser.notices)
}

pub(super) struct Parser<'a> {
    file: &'a std::path::Path,
    notices: Vec<Notice>,
}

impl Parser<'_> {
    pub(super) fn warn(&mut self, key: impl Into<String>, message: impl Into<String>) {
        self.notices.push(Notice {
            kind: NoticeKind::Warning,
            file: self.file.into(),
            key: key.into(),
            message: message.into(),
        });
    }

    fn config(&mut self, text: &str, renames: &crate::former_ids::Renames) -> Config {
        let mut config = Config::default();
        let mut root = match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(root)) => root,
            Ok(_) => {
                self.warn("", "expected a JSON object; using the defaults");
                return config;
            }
            Err(error) => {
                self.warn("", format!("invalid JSON ({error}); using the defaults"));
                return config;
            }
        };
        crate::former_ids::rename_config(&mut root, renames);

        let chat = self.section(&root, "chat");
        config.chat.engine = self.chat_engine(chat);
        config.chat.acp = acp::parse(self, chat);
        let mcp = self.section(&root, "mcp");
        config.mcp = mcp::parse(self, mcp);
        let web = self.section(&root, "web");
        config.web = web::parse(self, web);
        let terminal = self.section(&root, "terminal");
        config.terminal = terminal.cloned().unwrap_or_default();
        config.board_order = self.board_order(root.get("boardOrder"));
        config.auto_refresh = self.auto_refresh(root.get("autoRefresh"));

        let github = self.section(&root, "github");
        config.github.token = self.token(github, "github");
        config.repos = self.tracked_repos(root.get("repos"));
        config.boards = self.boards(root.get("boards"));
        config.features = self.features(root.get("features"));
        let web_boards = self.section(&root, "webBoards");
        config.web_boards = web_boards::parse(self, web_boards);
        config
    }

    /// `boards`: board id to Boolean. An unusable entry leaves the product
    /// default in effect, so a typo never changes a board without a notice.
    fn boards(&mut self, value: Option<&Value>) -> BTreeMap<String, bool> {
        let mut boards = BTreeMap::new();
        let entries = match value {
            None | Some(Value::Null) => return boards,
            Some(Value::Object(entries)) => entries,
            Some(_) => {
                self.warn("boards", "expected an object; using the defaults");
                return boards;
            }
        };
        for (id, value) in entries {
            match (id.trim(), value) {
                ("", _) => self.warn("boards", "expected a board id; ignored"),
                (id, Value::Bool(enabled)) => {
                    boards.insert(id.to_owned(), *enabled);
                }
                (_, other) => self.warn(
                    format!("boards.{id}"),
                    format!(
                        "expected true or false, got {}; the product default applies",
                        show(other)
                    ),
                ),
            }
        }
        boards
    }

    fn features(&mut self, value: Option<&Value>) -> BTreeMap<String, bool> {
        let mut features = BTreeMap::new();
        let entries = match value {
            None | Some(Value::Null) => return features,
            Some(Value::Object(entries)) => entries,
            Some(_) => {
                self.warn("features", "expected an object; using the defaults");
                return features;
            }
        };
        for (id, value) in entries {
            match value {
                Value::Bool(enabled) if features::valid_id(id) => {
                    features.insert(id.clone(), *enabled);
                }
                Value::Bool(_) => self.warn(
                    format!("features.{id}"),
                    format!("expected a namespaced feature id, got {id:?}; ignored"),
                ),
                other => self.warn(
                    format!("features.{id}"),
                    format!("expected true or false, got {}; ignored", show(other)),
                ),
            }
        }
        features
    }

    pub(super) fn section<'v>(
        &mut self,
        root: &'v Map<String, Value>,
        key: &str,
    ) -> Option<&'v Map<String, Value>> {
        match root.get(key) {
            None | Some(Value::Null) => None,
            Some(Value::Object(section)) => Some(section),
            Some(_) => {
                self.warn(key, "expected an object; using the defaults");
                None
            }
        }
    }

    pub(super) fn string(
        &mut self,
        section: Option<&Map<String, Value>>,
        name: &str,
        key: &str,
    ) -> Option<String> {
        match section?.get(key)? {
            Value::Null => None,
            Value::String(value) => Some(value.trim().to_owned()).filter(|value| !value.is_empty()),
            _ => {
                self.warn(format!("{name}.{key}"), "expected a string; ignored");
                None
            }
        }
    }

    fn token(&mut self, section: Option<&Map<String, Value>>, name: &str) -> Option<Secret> {
        self.string(section, name, "token").map(Secret::new)
    }

    fn tracked_repos(&mut self, repos: Option<&Value>) -> Vec<String> {
        let mut tracked = Vec::new();
        for (index, entry) in self.array(repos, "repos").iter().enumerate() {
            match entry.as_str().filter(|repo| is_repo(repo)) {
                Some(repo)
                    if !tracked
                        .iter()
                        .any(|known: &String| known.eq_ignore_ascii_case(repo)) =>
                {
                    tracked.push(repo.to_owned())
                }
                Some(_) => {}
                None => self.warn(
                    format!("repos[{index}]"),
                    format!("ignored invalid repo {}", show(entry)),
                ),
            }
        }
        tracked
    }

    pub(super) fn array<'v>(&mut self, value: Option<&'v Value>, key: &str) -> &'v [Value] {
        match value {
            None | Some(Value::Null) => &[],
            Some(Value::Array(items)) => items,
            Some(_) => {
                self.warn(key, "expected an array; ignored");
                &[]
            }
        }
    }
}
