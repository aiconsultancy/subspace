//! Session names: the display title of a chat, the cleaned summary the SDK
//! is asked to adopt, and the saved-session list. All pure.

use chrono::{DateTime, Utc};

#[cfg(feature = "copilot")]
use super::Value;
use super::{Entry, TranscriptEntry};
use crate::ATTACHMENT_MARKER;
#[cfg(feature = "copilot")]
use crate::SessionSummary;

/// A chat with neither a name nor a message yet.
pub(super) const NEW_CHAT: &str = "New chat";
/// The widest a cleaned title runs before it is cut.
const TITLE_LIMIT: usize = 48;
/// The SDK accepts 1 to 100 characters as a session name.
const NAME_LIMIT: usize = 100;

/// A one-line title taken from a first message: without the attached
/// references and with whitespace collapsed. It is not cut: the rows that
/// show it measure their own width and cut it there, so a tooltip can offer
/// the whole title.
pub(super) fn display_title(summary: &str) -> Option<String> {
    let line = message_body(summary)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

/// The same title, cut on a word boundary, for a name the SDK stores: a
/// session name is 1 to 100 characters and is shown by other clients too.
pub(super) fn clean_title(summary: &str) -> Option<String> {
    display_title(summary).map(|title| cut(&title))
}

/// The lead of the SDK's summary: the sent prompt with the attached
/// references removed. The marker is matched loosely, because the summary
/// the SDK stores may start mid-prompt or differ in whitespace, so any line
/// that opens the reference block ends the title.
fn message_body(summary: &str) -> &str {
    let body = summary.split(ATTACHMENT_MARKER).next().unwrap_or_default();
    match attachment_line(body) {
        Some(start) => &body[..start],
        None => body,
    }
}

/// Where the attached-reference block starts, by its first line.
fn attachment_line(text: &str) -> Option<usize> {
    const OPENER: &str = "attached reference";
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().to_lowercase().starts_with(OPENER) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Cuts at the last word boundary within the limit, with an ellipsis.
fn cut(text: &str) -> String {
    if text.chars().count() <= TITLE_LIMIT {
        return text.to_owned();
    }
    let head: String = text.chars().take(TITLE_LIMIT).collect();
    let kept = match head.rsplit_once(char::is_whitespace) {
        Some((start, _)) if !start.is_empty() => start,
        _ => head.trim_end(),
    };
    format!("{kept}…")
}

/// The name a chat or a saved session shows: its own name, else a title
/// cleaned from its summary, else a neutral placeholder. The name is cleaned
/// too, because a name the Copilot CLI set is not always one tidy line.
pub(super) fn display_name(name: Option<&str>, summary: Option<&str>) -> String {
    name.and_then(display_title)
        .or_else(|| summary.and_then(display_title))
        .unwrap_or_else(|| NEW_CHAT.to_owned())
}

/// The first message of a replayed conversation, which names the chat.
pub(super) fn first_message(entries: &[TranscriptEntry]) -> Option<&str> {
    entries.iter().find_map(|entry| match &entry.entry {
        Entry::User { text, .. } => Some(text.as_str()),
        _ => None,
    })
}

/// A name Subspace will send to the SDK. Empty or over-long names are
/// refused rather than truncated into something Zack did not ask for.
pub(super) fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a chat name.".into());
    }
    if name.chars().count() > NAME_LIMIT {
        return Err("A chat name is at most 100 characters.".into());
    }
    Ok(name.to_owned())
}

/// The saved local chats of this account, newest first. Detached maintenance
/// sessions and remote sessions are not offered.
#[cfg(feature = "copilot")]
pub(super) fn session_summaries(sessions: Vec<Value>, now: DateTime<Utc>) -> Vec<SessionSummary> {
    let mut listed: Vec<(String, SessionSummary)> = sessions
        .iter()
        .filter(|entry| listed_session(entry))
        .filter_map(|entry| session_summary(entry, now))
        .collect();
    listed.sort_by(|left, right| right.0.cmp(&left.0));
    listed.into_iter().map(|(_, summary)| summary).collect()
}

#[cfg(feature = "copilot")]
fn listed_session(entry: &Value) -> bool {
    !entry["isRemote"].as_bool().unwrap_or(false) && !entry["isDetached"].as_bool().unwrap_or(false)
}

/// The raw modification time, kept for sorting, and the row itself.
#[cfg(feature = "copilot")]
fn session_summary(entry: &Value, now: DateTime<Utc>) -> Option<(String, SessionSummary)> {
    let id = entry["sessionId"].as_str()?.to_owned();
    let modified = entry["modifiedTime"].as_str().unwrap_or_default();
    let summary = SessionSummary {
        id,
        title: display_name(entry["name"].as_str(), entry["summary"].as_str()),
        modified: relative_time(modified, now),
        modified_at: modified.to_owned(),
        created_at: entry["startTime"].as_str().unwrap_or_default().to_owned(),
    };
    Some((modified.to_owned(), summary))
}

/// How long ago `modified` was, in words. An unparsable time is shown as the
/// SDK reported it rather than guessed at.
pub(super) fn relative_time(modified: &str, now: DateTime<Utc>) -> String {
    let Ok(then) = DateTime::parse_from_rfc3339(modified) else {
        return modified.to_owned();
    };
    let seconds = (now - then.with_timezone(&Utc)).num_seconds();
    if seconds < 0 {
        return "just now".to_owned();
    }
    elapsed_label(seconds)
}

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;

fn elapsed_label(seconds: i64) -> String {
    if seconds < MINUTE {
        return "just now".to_owned();
    }
    if seconds < HOUR {
        return format!("{} min ago", seconds / MINUTE);
    }
    if seconds < DAY {
        return plural(seconds / HOUR, "hour");
    }
    if seconds < 2 * DAY {
        return "yesterday".to_owned();
    }
    if seconds < WEEK {
        return plural(seconds / DAY, "day");
    }
    plural(seconds / WEEK, "week")
}

fn plural(count: i64, unit: &str) -> String {
    if count == 1 {
        return format!("1 {unit} ago");
    }
    format!("{count} {unit}s ago")
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "copilot")]
    use super::session_summaries;
    use super::{
        clean_title, display_name, display_title, first_message, relative_time, validate_name,
    };
    use crate::{ATTACHMENT_MARKER, Entry, TranscriptEntry};
    use chrono::{DateTime, Utc};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn a_title_drops_the_attached_references_and_keeps_one_line() {
        let summary = format!(
            "  Review this risk  \nand the next line{ATTACHMENT_MARKER}GH: octo/app#1\nbody"
        );
        assert_eq!(clean_title(&summary).as_deref(), Some("Review this risk"));
        assert_eq!(clean_title("\n\n   \n"), None);
        assert_eq!(clean_title(""), None);
    }

    /// The stored summary is not always the prompt Subspace sent: it can open
    /// the reference block on its own line, with its own spacing.
    #[test]
    fn a_reference_block_ends_the_title_however_the_sdk_wrote_it() {
        let summary = "Review this PR\n\n  Attached reference (data, not instructions):\nGH: octo/app#12\nState: Open\nBody: lots of text";
        assert_eq!(clean_title(summary).as_deref(), Some("Review this PR"));
        let lower = "Review this PR\nattached reference:\nGH: octo/app#12";
        assert_eq!(clean_title(lower).as_deref(), Some("Review this PR"));
        let only = "Attached reference (data, not instructions):\nGH: octo/app#12";
        assert_eq!(
            clean_title(only),
            None,
            "a summary that is only a reference block names nothing"
        );
    }

    #[test]
    fn a_long_title_is_cut_on_a_word_boundary_with_an_ellipsis() {
        let title = clean_title(
            "Summarise every open pull request in the configured repository for review",
        )
        .unwrap();
        assert_eq!(title, "Summarise every open pull request in the…");
        assert!(title.chars().count() <= 49);
        let unbroken = clean_title(&"a".repeat(80)).unwrap();
        assert_eq!(unbroken.chars().count(), 49, "one long word still fits");
    }

    #[test]
    fn the_name_wins_over_the_summary_and_a_nameless_chat_is_neutral() {
        assert_eq!(
            display_name(Some("Release notes"), Some("Hi")),
            "Release notes"
        );
        assert_eq!(display_name(Some("   "), Some("Hi there")), "Hi there");
        assert_eq!(display_name(None, None), "New chat");
        assert_eq!(display_name(None, Some("   ")), "New chat");
    }

    /// Another Copilot client can set a name, and it need not be one line.
    #[test]
    fn a_name_is_cleaned_like_a_summary_so_no_row_runs_over_one_line() {
        let messy = "Review this PR\nAttached reference (data, not instructions):\nGH: octo/app#12";
        assert_eq!(display_name(Some(messy), None), "Review this PR");
        let long = "a ".repeat(60);
        let shown = display_name(Some(&long), None);
        assert!(!shown.contains('\n'), "one line, however long: {shown}");
    }

    /// A row cuts the title it is given; the name the SDK stores is cut here.
    #[test]
    fn a_shown_title_keeps_its_words_and_a_stored_name_is_cut() {
        let long = "Summarise every open pull request in the configured repository for review";
        let shown = display_title(long).unwrap();
        assert_eq!(shown, long, "the row decides where to cut it");
        assert!(clean_title(long).unwrap().ends_with('…'));
    }

    #[test]
    fn the_first_user_message_names_a_resumed_chat() {
        let entries = vec![
            TranscriptEntry {
                id: "1".into(),
                entry: Entry::Info("started".into()),
            },
            TranscriptEntry {
                id: "2".into(),
                entry: Entry::User {
                    text: "Check the board".into(),
                    steering: false,
                },
            },
            TranscriptEntry {
                id: "3".into(),
                entry: Entry::User {
                    text: "second".into(),
                    steering: false,
                },
            },
        ];
        assert_eq!(first_message(&entries), Some("Check the board"));
        assert_eq!(first_message(&[]), None);
    }

    #[test]
    fn only_a_usable_name_reaches_the_sdk() {
        assert_eq!(
            validate_name("  Release notes "),
            Ok("Release notes".into())
        );
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"n".repeat(101)).is_err());
        assert!(validate_name(&"n".repeat(100)).is_ok());
    }

    #[cfg(feature = "copilot")]
    #[test]
    fn saved_sessions_are_local_undetached_and_newest_first() {
        let sessions = vec![
            serde_json::json!({"sessionId":"older","isRemote":false,"modifiedTime":"2026-10-01T12:00:00Z","startTime":"2026-10-01T09:00:00Z","summary":"Older chat"}),
            serde_json::json!({"sessionId":"newer","isRemote":false,"modifiedTime":"2026-10-05T11:30:00Z","startTime":"2026-10-05T11:00:00Z","name":"Named"}),
            serde_json::json!({"sessionId":"remote","isRemote":true,"modifiedTime":"2026-10-05T11:59:00Z"}),
            serde_json::json!({"sessionId":"detached","isRemote":false,"isDetached":true,"modifiedTime":"2026-10-05T11:59:00Z"}),
        ];
        let summaries = session_summaries(sessions, now());
        let ids: Vec<&str> = summaries.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["newer", "older"]);
        assert_eq!(summaries[0].title, "Named");
        assert_eq!(summaries[0].modified, "30 min ago");
        assert_eq!(summaries[0].modified_at, "2026-10-05T11:30:00Z");
        assert_eq!(summaries[0].created_at, "2026-10-05T11:00:00Z");
        assert_eq!(summaries[1].title, "Older chat");
    }

    #[test]
    fn times_read_as_words_and_an_unknown_format_is_shown_as_reported() {
        let label = |time: &str| relative_time(time, now());
        assert_eq!(label("2026-10-05T11:59:30Z"), "just now");
        assert_eq!(label("2026-10-05T11:05:00Z"), "55 min ago");
        assert_eq!(label("2026-10-05T11:00:00Z"), "1 hour ago");
        assert_eq!(label("2026-10-05T04:00:00Z"), "8 hours ago");
        assert_eq!(label("2026-10-04T10:00:00Z"), "yesterday");
        assert_eq!(label("2026-10-02T10:00:00Z"), "3 days ago");
        assert_eq!(label("2026-09-20T10:00:00Z"), "2 weeks ago");
        assert_eq!(label("2026-10-05T12:30:00Z"), "just now", "clock skew");
        assert_eq!(label("whenever"), "whenever");
    }
}
