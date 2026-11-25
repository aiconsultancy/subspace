//! Exporting one saved chat as Markdown: the transcript as the user saw it,
//! with attached references folded to their labels and one line per tool
//! call. The formatting is pure; only the read and the write are not.

#[cfg(feature = "copilot")]
use super::{
    Client, EventLogReadRequest, EventsReadDirection, Session, SessionEvent, SessionId,
    SessionsReadPersistedEventsRequest, history_entries,
};
use super::{Entry, Path, ToolStatus, TranscriptEntry, state};
use crate::{ATTACHMENT_MARKER, ToolCall};

/// Events read per page, and the most pages one export reads, so a very long
/// chat cannot hold the runtime.
#[cfg(feature = "copilot")]
const PAGE: i64 = 200;
#[cfg(feature = "copilot")]
const MAX_PAGES: usize = 50;

/// The saved events of a chat that is not open, read without resuming it.
#[cfg(feature = "copilot")]
pub(super) async fn session_history(
    client: &Client,
    session: SessionId,
) -> Result<Vec<TranscriptEntry>, String> {
    let mut events = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let page = client
            .rpc()
            .sessions()
            .read_persisted_events(SessionsReadPersistedEventsRequest {
                cursor: cursor.take(),
                direction: Some(EventsReadDirection::Forward),
                max: Some(PAGE),
                session_id: session.clone(),
            })
            .await
            .map_err(|_| "Cannot read that chat's saved transcript.".to_string())?;
        events.extend(page.events);
        if !page.has_more {
            break;
        }
        cursor = Some(page.cursor);
    }
    Ok(history_entries(&events))
}

/// The events of an open chat, read from its journal rather than from
/// `session.getMessages`: that call returns the messages alone, so a replay
/// built from it loses every tool call the conversation made.
#[cfg(feature = "copilot")]
pub(super) async fn open_session_events(session: &Session) -> Result<Vec<SessionEvent>, String> {
    let mut events = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let page = session
            .rpc()
            .event_log()
            .read(EventLogReadRequest {
                cursor: cursor.take(),
                direction: Some(EventsReadDirection::Forward),
                // History backfill discards ephemerals anyway, so the read is
                // bounded by the durable log instead of racing a busy session.
                include_ephemeral: Some(false),
                max: Some(PAGE),
                ..Default::default()
            })
            .await
            .map_err(|_| "Cannot read this chat's transcript.".to_string())?;
        events.extend(page.events);
        if !page.has_more {
            break;
        }
        cursor = Some(page.cursor);
    }
    Ok(events)
}

/// The transcript as Markdown, led by the chat's name.
#[cfg(feature = "copilot")]
pub(super) fn markdown(title: &str, entries: &[TranscriptEntry]) -> String {
    markdown_with_agent(title, entries, "Copilot")
}

pub(super) fn markdown_with_agent(
    title: &str,
    entries: &[TranscriptEntry],
    assistant: &str,
) -> String {
    let mut text = format!("# {}\n", title.trim());
    for entry in entries {
        text.push('\n');
        text.push_str(&block(&entry.entry, assistant));
        text.push('\n');
    }
    text
}

fn block(entry: &Entry, assistant: &str) -> String {
    match entry {
        Entry::User { text, .. } => format!("## You\n\n{}\n", folded(text)),
        Entry::Assistant(text) => format!("## {assistant}\n\n{}\n", text.trim_end()),
        Entry::Info(text) => format!("> {}\n", text.trim_end()),
        Entry::Thinking { seconds, .. } => match seconds {
            Some(seconds) => format!("_Thought for {seconds}s_\n"),
            None => "_Thought_\n".to_owned(),
        },
        Entry::Tool(call) => format!("- {}\n", tool_line(call)),
    }
}

/// One line for a tool call: its title, then its arguments when they say
/// something the title does not, and a note when the call never finished.
pub(super) fn tool_line(call: &ToolCall) -> String {
    let mut line = format!("`{}`", call.title.trim());
    let detail = call.shown_detail();
    if !detail.is_empty() {
        line.push(' ');
        line.push_str(detail);
    }
    if call.status == ToolStatus::Aborted {
        line.push_str(" (did not finish)");
    }
    line
}

/// A sent message with its attached references folded to their labels, so an
/// export carries no copied service content.
fn folded(text: &str) -> String {
    let mut blocks = text.split(ATTACHMENT_MARKER);
    let mut folded = blocks.next().unwrap_or_default().trim_end().to_owned();
    for attachment in blocks {
        let label = attachment.lines().next().unwrap_or_default().trim();
        folded.push_str(&format!("\n\n_Attached: {label}_"));
    }
    folded
}

/// A file name for the save dialog: the chat's name, without path
/// separators and without the ellipsis a cut display name carries.
pub fn export_file_name(title: &str) -> String {
    let safe: String = title
        .trim_end()
        .trim_end_matches('…')
        .trim_end()
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect();
    let safe = safe.trim().trim_matches('.').trim();
    if safe.is_empty() {
        return "chat.md".to_owned();
    }
    format!("{safe}.md")
}

pub(super) fn write_export(path: &Path, text: &str) -> Result<(), String> {
    state::write_atomic(path, text).map_err(|_| "Cannot write the export file.".into())
}

#[cfg(test)]
mod tests {
    use super::{export_file_name, markdown_with_agent, tool_line};
    use crate::{ATTACHMENT_MARKER, Entry, ToolCall, ToolStatus, TranscriptEntry};

    fn markdown(title: &str, entries: &[TranscriptEntry]) -> String {
        markdown_with_agent(title, entries, "Copilot")
    }

    fn entry(id: &str, entry: Entry) -> TranscriptEntry {
        TranscriptEntry {
            id: id.into(),
            entry,
        }
    }

    #[test]
    fn an_export_reads_as_the_conversation_did() {
        let entries = vec![
            entry(
                "1",
                Entry::User {
                    text: "Check the risks".into(),
                    steering: false,
                },
            ),
            entry(
                "2",
                Entry::Thinking {
                    text: "reasoning".into(),
                    seconds: Some(4),
                },
            ),
            entry(
                "3",
                Entry::Tool(ToolCall {
                    call_id: "c1".into(),
                    title: "Read services".into(),
                    detail: "github_search query=is:open".into(),
                    status: ToolStatus::Done,
                    result: "3 risks".into(),
                }),
            ),
            entry("4", Entry::Assistant("Three risks need attention.".into())),
            entry("5", Entry::Info("The skill command finished.".into())),
        ];
        let text = markdown("Release risks", &entries);
        assert!(text.starts_with("# Release risks\n"));
        assert!(text.contains("## You\n\nCheck the risks\n"));
        assert!(text.contains("## Copilot\n\nThree risks need attention.\n"));
        assert!(text.contains("_Thought for 4s_"));
        assert!(text.contains("- `Read services` github_search query=is:open"));
        assert!(text.contains("> The skill command finished."));
        assert!(
            !text.contains("3 risks"),
            "a tool result is not replayed into the export"
        );
    }

    #[test]
    fn attached_references_are_folded_to_their_labels() {
        let message = format!("Review this{ATTACHMENT_MARKER}GH: octo/app#12\nState: Open");
        let text = markdown(
            "Review",
            &[entry(
                "1",
                Entry::User {
                    text: message,
                    steering: false,
                },
            )],
        );
        assert!(text.contains("Review this"));
        assert!(text.contains("_Attached: GH: octo/app#12_"));
        assert!(
            !text.contains("State: Open"),
            "the attached content stays out of the export"
        );
    }

    #[test]
    fn an_export_file_name_is_safe_on_every_platform() {
        assert_eq!(export_file_name("Release risks"), "Release risks.md");
        assert_eq!(export_file_name("a/b:c*d?"), "a-b-c-d-.md");
        assert_eq!(export_file_name("   "), "chat.md");
        assert_eq!(export_file_name(".."), "chat.md");
    }

    /// A display name is cut for the row it sits in; the file keeps the
    /// words, not the ellipsis that cut them.
    #[test]
    fn an_export_file_name_drops_the_display_ellipsis() {
        assert_eq!(
            export_file_name("give me a change log style list of the…"),
            "give me a change log style list of the.md"
        );
        assert_eq!(export_file_name("…"), "chat.md");
    }

    #[test]
    fn a_tool_line_says_each_thing_once() {
        let call = |title: &str, detail: &str, status| ToolCall {
            call_id: "c1".into(),
            title: title.into(),
            detail: detail.into(),
            status,
            result: String::new(),
        };
        assert_eq!(
            tool_line(&call("Read services", "github_search", ToolStatus::Done)),
            "`Read services` github_search"
        );
        assert_eq!(
            tool_line(&call("SELECT 1", "query=SELECT 1", ToolStatus::Done)),
            "`SELECT 1`",
            "the arguments are not repeated after the title"
        );
        assert_eq!(
            tool_line(&call("Read services", "", ToolStatus::Aborted)),
            "`Read services` (did not finish)"
        );
    }

    #[test]
    fn an_empty_chat_still_exports_its_name() {
        assert_eq!(markdown("New chat", &[]), "# New chat\n");
    }
}
