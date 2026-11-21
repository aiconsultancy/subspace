//! The session log the cloud detail panel shows: what is open, what the
//! cache holds, and how the entries read.
//!
//! The log comes from the undocumented Copilot API
//! ([`subspace_framework::client_github::copilot_agent_sessions`]). It is loaded only when the user
//! asks for it, kept for one session at a time, and shown as Markdown.

use gpui_kit::App;
use serde::{Deserialize, Serialize};
use subspace_framework::client_github::copilot_agent_sessions::{LogEntry, SessionLog};
use subspace_framework::dashboard::{PaletteCommand, Section, full_text, prefs};

use crate::dates;
use crate::keys::ShowLog;
use crate::ui::toggle_label;

pub(crate) mod load;
mod references;

pub(crate) use references::{TaskLinks, metadata};

/// The setting that turns the feature off; on unless it is stored as `0`.
pub(crate) const LOGS_KEY: &str = "github-copilot-agents.logs";

/// The cache file beside the state file. It holds the last log that was
/// shown, which is large, so it never goes into `state.json`.
pub(crate) const CACHE_FILE: &str = "github-copilot-agents-session-log.json";

/// The most entries the panel draws; a longer log says what it left out.
const MAX_SHOWN: usize = 400;
/// The most of one message the panel draws.
const MAX_TEXT: usize = 4_000;
/// Tool arguments and tool answers stay collapsed to one short line.
const MAX_COLLAPSED: usize = 160;

/// What the panel says when the log cannot be read or cannot be parsed.
pub(crate) const UNAVAILABLE: &str = "Log unavailable.";
pub(crate) const HEADING: &str = "Session log";

/// Where a loaded log stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LogState {
    Loading,
    Ready(SessionLog),
    Unavailable,
}

/// The session log the panel shows. A load for another row or another session
/// replaces it, while the on-disk cache retains several recent logs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenLog {
    /// The list key of the task whose panel asked for the log.
    pub(crate) task_key: String,
    pub(crate) session_id: String,
    /// The session id and its last update, which is what the cache is keyed
    /// by: a session that changed gets a new read.
    pub(crate) cache_key: String,
    pub(crate) state: LogState,
}

/// The cache file's one entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedLog {
    pub(crate) key: String,
    pub(crate) log: SessionLog,
}

const MAX_CACHED_LOGS: usize = 8;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn cache_key(session_id: &str, updated_at: &str) -> String {
    format!("{session_id}@{updated_at}")
}

/// The cache file's key for `key` as read by the work account `login`, so a
/// log one account read is never taken from the cache for another.
pub(crate) fn account_key(login: &str, key: &str) -> String {
    format!("{}/{key}", login.to_ascii_lowercase())
}

/// The cached log for `key`, when the file holds exactly that one.
pub(crate) fn cached(key: &str) -> Option<SessionLog> {
    let text = prefs::read_cache(CACHE_FILE)?;
    let entries = decode_entries(&text);
    entries
        .into_iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.log)
}

/// The cache file's text for one log; `None` when it cannot be written.
pub(crate) fn encode(key: &str, log: &SessionLog) -> Option<String> {
    encode_entries(key, log, prefs::read_cache(CACHE_FILE).as_deref())
}

pub(crate) fn scope(app: &str, login: &str) -> Option<full_text::Scope> {
    (!login.trim().is_empty()).then(|| full_text::Scope {
        app: app.into(),
        board: "github-copilot-agents".into(),
        endpoint: subspace_framework::client_github::API_URL.into(),
        tenant: "github".into(),
        account: login.to_ascii_lowercase(),
    })
}

pub(crate) fn document(
    task_key: &str,
    session_id: &str,
    log: &SessionLog,
    refreshed_at: i64,
) -> Option<full_text::Document> {
    Some(full_text::Document {
        id: session_id.to_owned(),
        title: format!("Agent session {session_id}"),
        body: log_markdown(log),
        metadata: "Copilot agent session log prompt tool call".into(),
        parent_id: None,
        kind: "Copilot Agents session log".into(),
        url: None,
        navigation: serde_json::to_string(&(task_key, session_id)).ok()?,
        refreshed_at,
        payload: serde_json::to_string(log).ok(),
    })
}

fn decode_entries(text: &str) -> Vec<CachedLog> {
    serde_json::from_str::<Vec<CachedLog>>(text)
        .or_else(|_| serde_json::from_str::<CachedLog>(text).map(|entry| vec![entry]))
        .unwrap_or_default()
}

fn encode_entries(key: &str, log: &SessionLog, previous: Option<&str>) -> Option<String> {
    let mut entries = decode_entries(previous.unwrap_or_default());
    entries.retain(|entry| entry.key != key);
    entries.insert(
        0,
        CachedLog {
            key: key.to_owned(),
            log: log.clone(),
        },
    );
    entries.truncate(MAX_CACHED_LOGS);
    while serde_json::to_vec(&entries).ok()?.len() > MAX_CACHE_BYTES && entries.len() > 1 {
        entries.pop();
    }
    serde_json::to_string(&entries).ok()
}

/// Whether the session log feature is on. It is, unless it was turned off.
pub(crate) fn logs_on(cx: &App) -> bool {
    prefs::get(cx, LOGS_KEY).as_deref() != Some("0")
}

pub(crate) fn set_logs(cx: &mut App, on: bool) {
    prefs::set(cx, LOGS_KEY, if on { "1" } else { "0" });
}

/// The palette's log commands: showing or hiding the open task's log, and
/// the setting that turns the feature off.
pub(crate) fn log_commands(shown: bool, cx: &App) -> Vec<PaletteCommand> {
    let on = logs_on(cx);
    vec![
        PaletteCommand::new(show_label(shown), ShowLog)
            .keywords("session log copilot transcript tool calls")
            .shortcut("l"),
        PaletteCommand::callback(toggle_label("Session logs", on), move |_, cx| {
            set_logs(cx, !on);
        })
        .keywords("copilot agents session log copilot transcript setting"),
    ]
}

/// What the log command offers: the change, not the state.
fn show_label(shown: bool) -> &'static str {
    if shown {
        "Hide the session log"
    } else {
        "Show the session log"
    }
}

/// `text` at most `max` characters, with a mark when it was cut.
pub(crate) fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}\u{2026}", &text[..at]),
        None => text.to_owned(),
    }
}

/// A collapsed one-line view of a long value.
fn collapsed(text: &str) -> String {
    let line = text.lines().find(|line| !line.trim().is_empty());
    clip(line.unwrap_or_default().trim(), MAX_COLLAPSED)
}

/// The time a log entry carries, as a local stamp with a space after it.
fn at(created: Option<i64>) -> String {
    match created.map(dates::unix_stamp).filter(|at| !at.is_empty()) {
        Some(stamp) => format!("`{stamp}`  "),
        None => String::new(),
    }
}

/// One entry as Markdown. Tool calls and tool answers are rows with the
/// tool's name; what they carry stays collapsed to one line.
fn entry_markdown(entry: &LogEntry) -> String {
    match entry {
        LogEntry::Assistant { created, text } => {
            format!("{}{}", at(*created), clip(text.trim(), MAX_TEXT))
        }
        LogEntry::ToolCall {
            created,
            name,
            arguments,
            ..
        } => format!(
            "- {}**{name}** `{}`",
            at(*created),
            collapsed(arguments).replace('`', "'")
        ),
        LogEntry::ToolResult { name, text } => format!(
            "  - {}answered {} characters: `{}`",
            if name.is_empty() {
                String::new()
            } else {
                format!("**{name}** ")
            },
            text.chars().count(),
            collapsed(text).replace('`', "'")
        ),
        LogEntry::User { text } => format!("**User**  {}", clip(text.trim(), MAX_TEXT)),
    }
}

/// What a long log says about the entries the panel left out.
fn left_out(total: usize) -> Option<String> {
    (total > MAX_SHOWN).then(|| format!("_{} more entries are not shown._", total - MAX_SHOWN))
}

/// The log as the panel's Markdown body.
fn log_markdown(log: &SessionLog) -> String {
    if log.entries.is_empty() {
        return "This session logged nothing.".to_owned();
    }
    let mut blocks: Vec<String> = log
        .entries
        .iter()
        .take(MAX_SHOWN)
        .map(entry_markdown)
        .collect();
    blocks.extend(left_out(log.entries.len()));
    if log.truncated {
        blocks.push("_The log was cut at 2 MB; this is its start only._".to_owned());
    }
    blocks.join("\n\n")
}

/// The panel section for the open log, or nothing when it belongs to
/// another row.
pub(crate) fn log_section(open: &OpenLog, task_key: &str) -> Option<Section> {
    if open.task_key != task_key {
        return None;
    }
    Some(match &open.state {
        LogState::Loading => Section::text(HEADING, "Loading the session log\u{2026}"),
        LogState::Unavailable => Section::text(HEADING, UNAVAILABLE),
        LogState::Ready(log) => Section::markdown(HEADING, log_markdown(log)),
    })
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agent_sessions::{LogEntry, SessionLog};
    use subspace_framework::dashboard::Body;

    use super::{HEADING, LogState, OpenLog, UNAVAILABLE, account_key, cache_key, log_section};

    #[test]
    fn a_cached_log_is_kept_apart_for_each_work_account() {
        let key = cache_key("s1", "2026-10-01T09:00:00Z");
        assert_ne!(
            account_key("octo-dev", &key),
            account_key("octo-other", &key)
        );
        assert_eq!(account_key("Octo-Dev", &key), account_key("octo-dev", &key));
        assert_ne!(
            account_key("octo-dev", &key),
            key,
            "an unbound entry never matches"
        );
    }

    fn log(entries: Vec<LogEntry>, truncated: bool) -> SessionLog {
        SessionLog { entries, truncated }
    }

    fn open(state: LogState) -> OpenLog {
        OpenLog {
            task_key: "cloud:t1".into(),
            session_id: "s1".into(),
            cache_key: cache_key("s1", "2026-10-01T09:00:00Z"),
            state,
        }
    }

    fn body(section: &subspace_framework::dashboard::Section) -> String {
        match &section.body {
            Body::Markdown(text) | Body::Text(text) => text.to_string(),
            other => panic!("a log section is text or Markdown, not {other:?}"),
        }
    }

    #[test]
    fn a_log_shows_text_tool_rows_and_collapsed_answers() {
        let entries = vec![
            LogEntry::User {
                text: "fix it".into(),
            },
            LogEntry::Assistant {
                created: Some(1_790_000_000),
                text: "Looking at it.".into(),
            },
            LogEntry::ToolCall {
                created: Some(1_790_000_002),
                id: "call_1".into(),
                name: "bash".into(),
                arguments: "{\"command\":\"ls\"}".into(),
            },
            LogEntry::ToolResult {
                name: "bash".into(),
                text: "README.md\nsrc\nmany\nmore\nlines\n".into(),
            },
        ];
        let section = log_section(&open(LogState::Ready(log(entries, false))), "cloud:t1").unwrap();
        let text = body(&section);
        assert_eq!(section.heading, HEADING);
        assert!(text.contains("**User**  fix it"));
        assert!(text.contains("Looking at it."));
        assert!(text.contains("- `"), "a tool call is a row: {text}");
        assert!(text.contains("**bash**"));
        assert!(
            text.contains("**bash** answered 30 characters: `README.md`"),
            "a tool answer collapses to one line: {text}"
        );
        assert!(
            !text.contains("many"),
            "the rest of the answer stays hidden"
        );
    }

    #[test]
    fn a_cut_log_and_an_empty_one_say_so() {
        let cut = log(
            vec![LogEntry::User {
                text: "fix it".into(),
            }],
            true,
        );
        let text = body(&log_section(&open(LogState::Ready(cut)), "cloud:t1").unwrap());
        assert!(text.contains("cut at 2 MB"));
        let empty =
            body(&log_section(&open(LogState::Ready(log(Vec::new(), false))), "cloud:t1").unwrap());
        assert_eq!(empty, "This session logged nothing.");
    }

    #[test]
    fn a_long_log_names_what_it_left_out() {
        let entries = (0..super::MAX_SHOWN + 5)
            .map(|index| LogEntry::User {
                text: format!("line {index}"),
            })
            .collect();
        let text =
            body(&log_section(&open(LogState::Ready(log(entries, false))), "cloud:t1").unwrap());
        assert!(text.contains("5 more entries are not shown."));
    }

    #[test]
    fn loading_and_failure_read_plainly_and_another_row_gets_nothing() {
        assert_eq!(
            body(&log_section(&open(LogState::Unavailable), "cloud:t1").unwrap()),
            UNAVAILABLE
        );
        assert!(
            body(&log_section(&open(LogState::Loading), "cloud:t1").unwrap()).contains("Loading")
        );
        assert!(log_section(&open(LogState::Loading), "cloud:other").is_none());
    }

    #[test]
    fn the_log_command_offers_the_change_not_the_state() {
        assert_eq!(super::show_label(false), "Show the session log");
        assert_eq!(super::show_label(true), "Hide the session log");
    }

    #[test]
    fn the_cache_key_names_the_session_and_its_last_update() {
        assert_eq!(
            cache_key("s1", "2026-10-01T09:00:00Z"),
            "s1@2026-10-01T09:00:00Z"
        );
        let log = log(Vec::new(), false);
        let text = super::encode("s1@t", &log).unwrap();
        let back: Vec<super::CachedLog> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].key, "s1@t");
        assert_eq!(back[0].log, log);
    }

    #[test]
    fn retained_logs_are_keyed_and_bounded() {
        let mut text = None;
        for index in 0..super::MAX_CACHED_LOGS + 2 {
            text = super::encode_entries(
                &format!("s{index}"),
                &log(
                    vec![LogEntry::User {
                        text: index.to_string(),
                    }],
                    false,
                ),
                text.as_deref(),
            );
        }
        let entries = super::decode_entries(text.as_deref().unwrap_or_default());
        assert_eq!(entries.len(), super::MAX_CACHED_LOGS);
        assert!(entries.iter().any(|entry| entry.key == "s9"));
        assert!(!entries.iter().any(|entry| entry.key == "s0"));
    }

    #[test]
    fn old_single_entry_cache_is_read_and_upgraded() {
        let old = serde_json::to_string(&super::CachedLog {
            key: "old".into(),
            log: log(Vec::new(), false),
        })
        .unwrap();
        assert_eq!(super::decode_entries(&old).len(), 1);
        let upgraded = super::encode_entries("new", &log(Vec::new(), false), Some(&old)).unwrap();
        assert_eq!(super::decode_entries(&upgraded).len(), 2);
    }

    #[test]
    fn indexed_logs_keep_prompt_text_and_fenced_navigation() {
        let log = log(
            vec![LogEntry::User {
                text: "find the cache bug".into(),
            }],
            false,
        );
        let document = super::document("cloud:t1", "s1", &log, 10).unwrap();
        assert!(document.body.contains("find the cache bug"));
        assert_eq!(document.id, "s1");
        assert_eq!(document.navigation, r#"["cloud:t1","s1"]"#);
    }
}
