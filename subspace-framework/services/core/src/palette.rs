//! Command palette logic: command matching, match highlighting and the
//! recent items list.

use crate::search::{fold, tokenize};

const FIRST_LABEL_WORD_PREFIX_SCORE: u32 = 10;
const LABEL_WORD_PREFIX_SCORE: u32 = 8;
const KEYWORD_PREFIX_SCORE: u32 = 5;
const LABEL_SUBSTRING_SCORE: u32 = 3;

/// How well `query` matches a command's label and keywords; 0 is no match.
///
/// Every query word must match: a prefix of the label's first word scores
/// highest, then a prefix of another label word, then a keyword prefix, then
/// a substring of the label. A query that fails falls back to a subsequence of
/// the label.
pub fn command_score(label: &str, keywords: &str, query: &str) -> u32 {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return 1;
    }
    let label = label.to_lowercase();
    let label_words: Vec<&str> = label
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '@' | '`' | '?')))
        .filter(|word| !word.is_empty())
        .collect();
    let keywords = keywords.to_lowercase();

    let mut total = 0;
    for word in &words {
        let best = score_word(&label_words, &keywords, &label, word);
        if best == 0 {
            total = 0;
            break;
        }
        total += best;
    }
    if total > 0 {
        return total;
    }
    let needle: String = words.concat();
    u32::from(is_subsequence(&needle, &label))
}

fn score_word(label_words: &[&str], keywords: &str, label: &str, word: &str) -> u32 {
    if label_words.first().is_some_and(|w| w.starts_with(word)) {
        FIRST_LABEL_WORD_PREFIX_SCORE
    } else if label_words.iter().any(|w| w.starts_with(word)) {
        LABEL_WORD_PREFIX_SCORE
    } else if keywords.split_whitespace().any(|w| w.starts_with(word)) {
        KEYWORD_PREFIX_SCORE
    } else if label.contains(word) {
        LABEL_SUBSTRING_SCORE
    } else {
        0
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut wanted = needle.chars().peekable();
    for c in haystack.chars() {
        if wanted.peek() == Some(&c) {
            wanted.next();
        }
    }
    !needle.is_empty() && wanted.peek().is_none()
}

/// The positions of the commands that match `query`, best first; ties keep
/// the given order. `fields` gives a command's label and keywords.
pub fn filter_commands<T>(
    commands: &[T],
    query: &str,
    fields: impl Fn(&T) -> (&str, &str),
) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> = commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let (label, keywords) = fields(command);
            (index, command_score(label, keywords, query))
        })
        .filter(|(_, score)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(index, _)| index).collect()
}

/// A piece of text, marked where it matches the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub hit: bool,
}

/// Splits `text` into plain and matched runs: a word is matched where it
/// starts with a query word.
pub fn highlight_runs(text: &str, query: &str) -> Vec<Run> {
    let words = tokenize(query);
    if words.is_empty() {
        return vec![Run {
            text: text.to_string(),
            hit: false,
        }];
    }
    let mut runs: Vec<Run> = Vec::new();
    let mut push = |part: &str, hit: bool| {
        if part.is_empty() {
            return;
        }
        match runs.last_mut() {
            Some(last) if last.hit == hit => last.text.push_str(part),
            _ => runs.push(Run {
                text: part.to_string(),
                hit,
            }),
        }
    };
    for part in split_words(text) {
        let folded = fold(part);
        let length = words
            .iter()
            .filter(|word| folded.starts_with(word.as_str()))
            .map(|word| word.chars().count())
            .max()
            .unwrap_or(0);
        let chars = part.chars().count();
        // Accents fold to the same length for common Latin text; else mark
        // the whole word.
        let cut = if folded.chars().count() == chars {
            length
        } else if length > 0 {
            chars
        } else {
            0
        };
        let at = part.char_indices().nth(cut).map_or(part.len(), |(i, _)| i);
        push(&part[..at], true);
        push(&part[at..], false);
    }
    runs
}

/// `text` cut into alternating runs of letters and digits and of anything
/// else, in order.
fn split_words(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut word = None;
    for (i, c) in text.char_indices() {
        let is_word = c.is_alphanumeric();
        if word.is_some_and(|w| w != is_word) {
            parts.push(&text[start..i]);
            start = i;
        }
        word = Some(is_word);
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

/// Most recent first, no duplicates, at most `max`.
pub fn push_recent<T: PartialEq + Clone>(list: &[T], item: T, max: usize) -> Vec<T> {
    std::iter::once(item.clone())
        .chain(list.iter().filter(|x| **x != item).cloned())
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Run, command_score, filter_commands, highlight_runs, push_recent};

    const COMMANDS: &[(&str, &str)] = &[
        ("Switch to board layout", "view layout v"),
        ("Show everyone (all)", "filter people me team m"),
        ("Change state…", "status s"),
        ("Change sprint…", "iteration"),
        ("Wrap titles", "truncate lines w"),
    ];

    fn labels(query: &str) -> Vec<&'static str> {
        filter_commands(COMMANDS, query, |(label, keywords)| (label, keywords))
            .into_iter()
            .map(|i| COMMANDS[i].0)
            .collect()
    }

    fn run(text: &str, hit: bool) -> Run {
        Run {
            text: text.into(),
            hit,
        }
    }

    #[test]
    fn a_label_word_prefix_beats_a_keyword_match() {
        assert_eq!(labels("board")[0], "Switch to board layout");
        assert_eq!(labels("change"), ["Change state…", "Change sprint…"]);
        assert_eq!(labels("iter"), ["Change sprint…"]);
    }

    #[test]
    fn command_score_keeps_match_precedence_and_weights() {
        assert_eq!(command_score("Board view", "board-keyword", "boa"), 10);
        assert_eq!(command_score("Switch board", "switch-keyword", "boa"), 8);
        assert_eq!(command_score("Switch view", "board-keyword", "boa"), 5);
        assert_eq!(command_score("Dashboard", "", "boa"), 3);
    }

    #[test]
    fn every_word_must_match_and_a_subsequence_is_the_fallback() {
        assert_eq!(labels("change sp"), ["Change sprint…"]);
        assert_eq!(labels("chsta"), ["Change state…"]);
        assert_eq!(command_score("Reload", "", "xyz"), 0);
        assert_eq!(labels("").len(), COMMANDS.len());
    }

    #[test]
    fn highlight_marks_word_prefixes_that_match_the_query() {
        assert_eq!(
            highlight_runs("Plan crew onboarding", "onbo crew"),
            [
                run("Plan ", false),
                run("crew", true),
                run(" ", false),
                run("onbo", true),
                run("arding", false),
            ]
        );
        assert_eq!(highlight_runs("plain", ""), [run("plain", false)]);
        assert_eq!(
            highlight_runs("Café crème", "cafe"),
            [run("Café", true), run(" crème", false)]
        );
    }

    #[test]
    fn recent_items_are_newest_first_unique_and_capped() {
        assert_eq!(push_recent(&[3, 2, 1], 2, 8), [2, 3, 1]);
        assert_eq!(push_recent(&[1, 2, 3], 4, 3), [4, 1, 2]);
    }
}
