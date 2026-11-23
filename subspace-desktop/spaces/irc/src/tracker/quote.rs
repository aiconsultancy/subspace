//! What a message action writes: the message quoted as text, the issue or
//! decision drafted from it, and the choices its dialog offers. Pure.

use chrono::{DateTime, Utc};

use super::plan::clip;
use super::write::{DecisionDraft, IssueDraft, SUMMARY_CHARS, TITLE_CHARS};
use crate::state::Entry;

/// The most characters of the message a decision's evidence quotes.
const EXCERPT_CHARS: usize = 4_000;

/// Which action the dialog is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    MakeIssue,
    RecordDecision,
}

/// The message an action quotes: its author, time, plain text and where
/// it was said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Quoted {
    pub author: String,
    pub time: DateTime<Utc>,
    pub text: String,
    pub msgid: Option<String>,
    /// Such as `#aurora-lyra on Acme IRC`.
    pub place: String,
}

impl Quoted {
    /// `entry` as said in `place`, its formatting codes removed.
    pub(crate) fn of(entry: &Entry, place: String) -> Self {
        Self {
            author: entry.line.from.clone().unwrap_or_default(),
            time: entry.line.time,
            text: subspace_client_irc::format::strip(&entry.line.text),
            msgid: entry.line.id.clone(),
            place,
        }
    }

    /// Where the message can be found again: its place, time and ID.
    pub(crate) fn reference(&self) -> String {
        let time = self.time.format("%Y-%m-%d %H:%M UTC");
        match &self.msgid {
            Some(id) => format!(
                "IRC message {id} by {} in {}, {time}",
                self.author, self.place
            ),
            None => format!("IRC message by {} in {}, {time}", self.author, self.place),
        }
    }
}

/// The issue description for `quoted`: its words in a fenced block, so
/// nothing in them is read as Markdown, a link or a mention, then who said
/// it, where and when, as a code span for the same reason: a nick, a
/// channel's name and a message ID are other people's text too.
pub(crate) fn description(quoted: &Quoted) -> String {
    let longest = longest_run(&quoted.text, '`');
    let fence = "`".repeat(longest.max(2) + 1);
    format!(
        "{fence}text\n{}\n{fence}\n\nFrom {}.",
        quoted.text.trim_end(),
        code_span(&quoted.reference())
    )
}

/// `text` as one Markdown code span, so nothing in it is a link, a
/// mention or formatting: on one line, between backtick runs longer than
/// any in it, padded where it starts or ends with a backtick.
pub(crate) fn code_span(text: &str) -> String {
    let line = text.replace(['\n', '\r'], " ");
    let ticks = "`".repeat(longest_run(&line, '`') + 1);
    let pad = if line.starts_with('`') || line.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{line}{pad}{ticks}")
}

fn longest_run(text: &str, mark: char) -> usize {
    text.split(|character| character != mark)
        .map(str::len)
        .max()
        .unwrap_or(0)
}

/// The issue to create from `quoted` in `project` with `title`.
pub(crate) fn issue_draft(
    quoted: &Quoted,
    project: &str,
    title: &str,
) -> Result<IssueDraft, String> {
    let title = checked_title(title)?;
    Ok(IssueDraft {
        project: project.to_owned(),
        title,
        summary: clip(&quoted.text, SUMMARY_CHARS),
        description: description(quoted),
    })
}

/// The decision to record on `issue` from `quoted` with `title`.
pub(crate) fn decision_draft(
    quoted: &Quoted,
    issue: &str,
    title: &str,
    context: String,
) -> Result<DecisionDraft, String> {
    let title = checked_title(title)?;
    let excerpt = clip(
        &format!("{}: {}", quoted.author, quoted.text),
        EXCERPT_CHARS,
    );
    Ok(DecisionDraft {
        issue: issue.to_owned(),
        title,
        text: quoted.text.trim().to_owned(),
        context,
        message: Some((code_span(&quoted.reference()), excerpt)),
    })
}

fn checked_title(title: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > TITLE_CHARS {
        return Err(format!("Enter a title of 1 to {TITLE_CHARS} characters."));
    }
    Ok(title.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The project key of `identifier`, such as `NAV` for `NAV-2`.
pub(crate) fn project_of(identifier: &str) -> Option<&str> {
    identifier.rsplit_once('-').map(|(key, _)| key)
}

/// The choices a dialog offers and the one chosen first. For a decision:
/// the issues the message names, then those in play, defaulting to the
/// message's only issue. For an issue: the tracker's projects, defaulting
/// to the project of the message's only issue, else of the newest issue in
/// play, else the first.
pub(crate) fn choices(
    kind: Kind,
    named: &[String],
    in_play: &[String],
    projects: &[String],
) -> (Vec<String>, Option<String>) {
    match kind {
        Kind::RecordDecision => {
            let mut issues: Vec<String> = Vec::new();
            for identifier in named.iter().chain(in_play) {
                if !issues.contains(identifier) {
                    issues.push(identifier.clone());
                }
            }
            let chosen = match named {
                [only] => Some(only.clone()),
                _ => None,
            };
            (issues, chosen)
        }
        Kind::MakeIssue => {
            let preferred = match named {
                [only] => Some(only),
                _ => in_play.first(),
            }
            .and_then(|identifier| project_of(identifier))
            .filter(|key| projects.iter().any(|project| project == key));
            let chosen = preferred
                .map(str::to_owned)
                .or_else(|| projects.first().cloned());
            (projects.to_vec(), chosen)
        }
    }
}
