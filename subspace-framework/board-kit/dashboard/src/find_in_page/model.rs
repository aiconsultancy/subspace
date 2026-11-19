//! Literal, case-insensitive matches in document order and a wrapping cursor.

use std::ops::Range;
use std::sync::Arc;

use regex::RegexBuilder;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) document: Arc<str>,
    pub(crate) range: Range<usize>,
}

#[derive(Default)]
pub(crate) struct FindCursor {
    hits: Vec<Match>,
    active: Option<usize>,
}

impl FindCursor {
    pub(crate) fn replace(&mut self, hits: Vec<Match>, retain: bool) {
        let previous = previous_match(self, retain);
        self.active = replacement_ix(&hits, previous.as_ref());
        self.hits = hits;
    }

    pub(crate) fn step(&mut self, backwards: bool) {
        self.active = stepped_ix(self.hits.len(), self.active, backwards);
    }

    pub(crate) fn clear(&mut self) {
        self.hits.clear();
        self.active = None;
    }

    pub(crate) fn hits(&self) -> &[Match] {
        &self.hits
    }

    pub(crate) fn active(&self) -> Option<&Match> {
        self.active.and_then(|ix| self.hits.get(ix))
    }

    pub(crate) fn position(&self) -> (usize, usize) {
        (self.active.map_or(0, |ix| ix + 1), self.hits.len())
    }
}

pub(crate) fn find<'a>(
    query: &str,
    documents: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Vec<Match>, regex::Error> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()?;
    Ok(documents
        .into_iter()
        .flat_map(|(document, text)| {
            let document: Arc<str> = document.into();
            pattern.find_iter(text).map(move |hit| Match {
                document: document.clone(),
                range: hit.range(),
            })
        })
        .collect())
}

fn previous_match(cursor: &FindCursor, retain: bool) -> Option<Match> {
    retain
        .then_some(cursor.active)
        .flatten()
        .and_then(|ix| cursor.hits.get(ix))
        .cloned()
}

fn replacement_ix(hits: &[Match], previous: Option<&Match>) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    Some(
        previous
            .and_then(|previous| hits.iter().position(|hit| hit == previous))
            .unwrap_or(0),
    )
}

fn stepped_ix(len: usize, current: Option<usize>, backwards: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match (current, backwards) {
        (None, false) => Some(0),
        (None, true) => Some(len - 1),
        (Some(0), true) => Some(len - 1),
        (Some(ix), true) => Some(ix - 1),
        (Some(ix), false) => Some((ix + 1) % len),
    }
}

#[cfg(test)]
mod tests {
    use super::{FindCursor, Match, find};

    #[test]
    fn finds_literal_unicode_text_in_document_order_with_byte_ranges() {
        let hits = find("σ", [("title", "Σ ς"), ("comment", "€σ")]).expect("literal query");
        assert_eq!(
            hits,
            [
                Match {
                    document: "title".into(),
                    range: 0..2
                },
                Match {
                    document: "title".into(),
                    range: 3..5
                },
                Match {
                    document: "comment".into(),
                    range: 3..5
                },
            ]
        );
        assert_eq!(find("[a]", [("body", "[A] a")]).unwrap().len(), 1);
        assert!(find("", [("body", "anything")]).unwrap().is_empty());
    }

    #[test]
    fn moves_in_both_directions_wraps_and_clears_when_closed() {
        let mut cursor = FindCursor::default();
        cursor.replace(find("one", [("body", "one ONE one")]).unwrap(), false);
        assert_eq!(cursor.position(), (1, 3));
        cursor.step(true);
        assert_eq!(cursor.position(), (3, 3));
        cursor.step(false);
        assert_eq!(cursor.position(), (1, 3));
        cursor.step(false);
        assert_eq!(cursor.active().unwrap().range, 4..7);
        cursor.clear();
        cursor.step(true);
        assert_eq!(cursor.position(), (0, 0));
        assert!(cursor.hits().is_empty() && cursor.active().is_none());
    }

    #[test]
    fn retains_the_current_hit_on_refresh_and_resets_for_a_new_query() {
        let mut cursor = FindCursor::default();
        cursor.replace(find("one", [("comment", "one one")]).unwrap(), false);
        cursor.step(false);
        cursor.replace(
            find("one", [("title", "one"), ("comment", "one one")]).unwrap(),
            true,
        );
        assert_eq!(cursor.position(), (3, 3));
        cursor.replace(
            find("one", [("title", "one"), ("comment", "one one")]).unwrap(),
            false,
        );
        assert_eq!(cursor.position(), (1, 3));
        cursor.replace(Vec::new(), true);
        assert_eq!(cursor.position(), (0, 0));
    }
}
