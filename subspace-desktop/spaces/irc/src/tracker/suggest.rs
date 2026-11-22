//! Completing a tracker command: the commands themselves among the
//! composer's, then each argument (issues this conversation has shown,
//! newest first, then the tracker's project keys; whom to assign; `off`),
//! and, once the draft is a whole command that can run, a first row that
//! says what Return will do. Pure.

use subspace_framework::dashboard::chat_input::completion::CompletionItem;

use super::parse::{Name, arguments_before, typed};
use super::plan::{Answer, Facts, clip, plan};
use crate::compose::suggest::Suggestion;

/// The most issues one list offers; typing narrows the rest.
const ISSUE_LIMIT: usize = 20;
/// The most characters of a title an issue's row shows.
const TITLE_CHARS: usize = 48;

/// The tracker commands whose name starts with `query`.
pub(crate) fn commands(query: &str, facts: &Facts) -> Vec<Suggestion> {
    Name::ALL
        .into_iter()
        .filter(|name| name.word().starts_with(query))
        .map(|name| {
            let description = name.description(facts.ship.as_deref());
            Suggestion::insert(
                CompletionItem::command(
                    format!("/{}", name.word()),
                    name.args(),
                    description.clone(),
                )
                .spoken(format!(
                    "/{} {}, {description}, writes to the tracker",
                    name.word(),
                    name.args()
                )),
                format!("/{} ", name.word()),
            )
        })
        .collect()
}

/// The choices for the argument word `word` after `before`, when `before`
/// is a tracker command, and the row that runs `text` when it is whole.
pub(crate) fn arguments(
    text: &str,
    (before, word): (&str, &str),
    facts: &Facts,
) -> Option<Vec<Suggestion>> {
    let (name, done) = arguments_before(before)?;
    let run = (before.len() + word.len() == text.len())
        .then(|| preview(text, facts))
        .flatten();
    let choices = match (name, done.as_slice()) {
        (_, []) => issues(word, facts),
        (Name::Assign, [_]) => whom(word, facts, true),
        (Name::Assign, [_, to]) if to.eq_ignore_ascii_case("to") => whom(word, facts, false),
        (Name::Ready, [_]) => off(word),
        _ => Vec::new(),
    };
    Some(run.into_iter().chain(choices).collect())
}

/// The row that runs `text`, when it is a whole command that can run now.
fn preview(text: &str, facts: &Facts) -> Option<Suggestion> {
    let typed = typed(text)?;
    let plan = plan(&typed, facts).ok()?;
    let known = match facts.answer(plan.issue()) {
        Answer::Known(known) => Some(known),
        _ => None,
    };
    let words = plan.preview(known);
    let label = format!("/{}", typed.name.word());
    Some(Suggestion::run(
        CompletionItem::command(label, "", words.clone()).spoken(words),
    ))
}

/// Issues for the first argument: those the conversation has shown, then
/// a project key to type an identifier on.
fn issues(word: &str, facts: &Facts) -> Vec<Suggestion> {
    let query = word.to_ascii_uppercase();
    let shown = facts
        .in_play
        .iter()
        .filter(|identifier| identifier.starts_with(&query))
        .take(ISSUE_LIMIT)
        .map(|identifier| {
            let title = match facts.answer(identifier) {
                Answer::Known(known) => clip(&known.title, TITLE_CHARS),
                _ => String::new(),
            };
            Suggestion::insert(
                CompletionItem::command(identifier.clone(), "", title.clone())
                    .spoken(format!("{identifier}, {title}")),
                format!("{identifier} "),
            )
        });
    let keys = facts
        .projects
        .iter()
        .map(|key| format!("{key}-"))
        .filter(|prefix| prefix.starts_with(&query) && !query.contains('-'))
        .map(|prefix| {
            Suggestion::insert(
                CompletionItem::command(prefix.clone(), "", "An issue in this project")
                    .spoken(format!("{prefix}, an issue in this project")),
                prefix,
            )
        });
    shown.chain(keys).collect()
}

/// Whom to assign: `to <agent>` and `to me` after the issue (`with_to`),
/// or the name alone after `to`. The ship's captain comes first.
fn whom(word: &str, facts: &Facts, with_to: bool) -> Vec<Suggestion> {
    let query = word.to_ascii_lowercase();
    let mut names: Vec<(String, &str)> = Vec::new();
    if let Some(captain) = &facts.captain {
        names.push((captain.nick.clone(), "This ship's captain"));
    }
    for agent in &facts.agents {
        if !names
            .iter()
            .any(|(nick, _)| nick.eq_ignore_ascii_case(&agent.nick))
        {
            names.push((agent.nick.clone(), "A fleet agent"));
        }
    }
    names.push(("me".to_owned(), "You"));
    names
        .into_iter()
        .map(|(nick, note)| {
            let shown = if with_to { format!("to {nick}") } else { nick };
            (shown, note)
        })
        .filter(|(shown, _)| shown.to_ascii_lowercase().starts_with(&query))
        .map(|(shown, note)| {
            Suggestion::insert(
                CompletionItem::command(shown.clone(), "", note).spoken(format!("{shown}, {note}")),
                format!("{shown} "),
            )
        })
        .collect()
}

fn off(word: &str) -> Vec<Suggestion> {
    "off"
        .starts_with(&word.to_ascii_lowercase())
        .then(|| {
            Suggestion::insert(
                CompletionItem::command("off", "", "Clear the Ready for agents flag")
                    .spoken("off, clear the Ready for agents flag"),
                "off".to_owned(),
            )
        })
        .into_iter()
        .collect()
}
