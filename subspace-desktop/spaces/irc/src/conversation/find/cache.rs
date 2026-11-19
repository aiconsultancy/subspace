//! Cache prepared documents by stable row identity; incoming lines parse once.
use crate::{
    lines::Item,
    markup::Mentions,
    state::{Buffer, Entry},
    transcript::{self, Row},
};
use gpui_kit::App;
use std::{
    collections::{HashMap, HashSet},
    ops::RangeInclusive,
};
use subspace_framework::dashboard::{Tokens, Tone, find_in_page::Document};
const TONES: [Tone; 11] = [
    Tone::Fg,
    Tone::Strong,
    Tone::Muted,
    Tone::Accent,
    Tone::Red,
    Tone::Orange,
    Tone::Yellow,
    Tone::Green,
    Tone::Cyan,
    Tone::Blue,
    Tone::Magenta,
];
#[derive(Default)]
pub(crate) struct Cache {
    mentions: Mentions,
    palette: Vec<u32>,
    rows: HashMap<String, Cached>,
    pub(super) indices: HashMap<std::sync::Arc<str>, usize>,
    #[cfg(test)]
    pub(crate) computations: usize,
    #[cfg(test)]
    pub(crate) calls: usize,
}
struct Cached {
    key: std::sync::Arc<str>,
    source: Vec<Entry>,
    document: Document,
}
impl Cache {
    pub(super) fn documents(
        &mut self,
        rows: &[Row],
        held: &Buffer,
        (mentions, hub): (&Mentions, Option<&crate::hub::Hub>),
        cx: &App,
    ) -> Vec<(usize, Document)> {
        #[cfg(test)]
        {
            self.calls += 1;
        }
        self.refresh_context(mentions, &Tokens::current(cx));
        self.indices.clear();
        let mut live = HashSet::new();
        let mut documents = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let Some((id, range)) = source(*row, held) else {
                continue;
            };
            live.insert(id.clone());
            let shown = displayed(held, range.clone(), hub);
            let current = self
                .rows
                .get(&id)
                .is_some_and(|cached| cached.source == shown);
            if !current {
                let Some(document) = transcript::find_document(*row, held, (mentions, hub), cx)
                else {
                    continue;
                };
                #[cfg(test)]
                {
                    self.computations += 1;
                }
                self.rows.insert(
                    id.clone(),
                    Cached {
                        key: document.key().into(),
                        source: shown,
                        document,
                    },
                );
            }
            if let Some(cached) = self.rows.get(&id) {
                self.indices.insert(cached.key.clone(), index);
                documents.push((index, cached.document.clone()));
            }
        }
        self.rows.retain(|id, _| live.contains(id));
        documents
    }
    fn refresh_context(&mut self, mentions: &Mentions, tokens: &Tokens) {
        let palette = TONES
            .iter()
            .map(|tone| u32::from(tokens.tone(*tone).to_rgb()))
            .collect::<Vec<_>>();
        if self.palette != palette {
            self.rows.clear();
            self.palette = palette;
        } else if self.mentions != *mentions {
            if let Some(changed) = self.mentions.changed_nicks(mentions) {
                self.rows.retain(|_, cached| {
                    !cached.source.iter().any(|entry| {
                        let text = entry.line.text.to_lowercase();
                        changed.iter().any(|nick| text.contains(nick))
                    })
                });
            } else {
                self.rows.clear();
            }
        }
        self.mentions = mentions.clone();
    }
}
/// The entries in `range` as their rows show them.
fn displayed(
    held: &Buffer,
    range: RangeInclusive<usize>,
    hub: Option<&crate::hub::Hub>,
) -> Vec<Entry> {
    held.entries
        .range(range)
        .map(|entry| transcript::displayed(entry, hub).unwrap_or_else(|| entry.clone()))
        .collect()
}
fn source(row: Row, held: &Buffer) -> Option<(String, RangeInclusive<usize>)> {
    let Row::Item(item) = row else {
        return None;
    };
    let range = match item {
        Item::Message { at, .. } | Item::Line(at) => at..=at,
        Item::Membership { first, last } => first..=last,
        Item::Day(_) => return None,
    };
    if range.start() > range.end() || *range.end() >= held.entries.len() {
        return None;
    }
    Some((crate::lines::item_id(item, &held.entries), range))
}

#[cfg(test)]
mod tests {
    use super::Cache;
    use crate::{
        fixtures::{channel, line},
        lines::Item,
        markup::Mentions,
        state::{Buffer, Entry},
        transcript::Row,
    };
    use gpui_kit::TestAppContext;
    use subspace_client_irc::LineKind;
    #[gpui_kit::test]
    fn unchanged_rows_are_not_reparsed_and_removed_rows_leave_the_cache(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let room = channel("#cache");
        let mut buffer = Buffer::default();
        buffer.entries.extend((0..100).map(|n| Entry {
            seq: n,
            line: line(
                &room,
                LineKind::Privmsg,
                "fixture-bot",
                &format!("message {n}"),
                n + 1,
            ),
            highlight: false,
        }));
        let rows: Vec<_> = (0..100)
            .map(|at| {
                Row::Item(Item::Message {
                    at,
                    continues: false,
                })
            })
            .collect();
        let mut cache = Cache::default();
        cx.update(|cx| {
            assert_eq!(
                cache
                    .documents(&rows, &buffer, (&Mentions::default(), None), cx)
                    .len(),
                100
            );
            assert_eq!(cache.computations, 100);
            cache.documents(&rows, &buffer, (&Mentions::default(), None), cx);
            assert_eq!(cache.computations, 100);
            buffer.entries[50].line.text.push_str(" edited");
            let documents = cache.documents(&rows, &buffer, (&Mentions::default(), None), cx);
            assert!(documents[50].1.markdown.contains("edited"));
            assert_eq!(cache.computations, 101);
            cache.documents(&rows[..2], &buffer, (&Mentions::default(), None), cx);
            assert_eq!(cache.rows.len(), 2);
            assert_eq!(cache.indices.len(), 2);
            assert!(
                cache
                    .documents(
                        &[Row::Item(Item::Line(999))],
                        &buffer,
                        (&Mentions::default(), None),
                        cx
                    )
                    .is_empty()
            );
        });
    }
    #[gpui_kit::test]
    fn nick_changes_reparse_only_rows_that_can_mention_the_changed_name(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let room = channel("#members");
        let mut buffer = Buffer::default();
        buffer.entries.extend((0..100).map(|n| Entry {
            seq: n,
            line: line(
                &room,
                LineKind::Privmsg,
                "fixture-bot",
                if n == 50 {
                    "hello @Alice"
                } else {
                    "ordinary text"
                },
                n + 1,
            ),
            highlight: false,
        }));
        let rows: Vec<_> = (0..100)
            .map(|at| {
                Row::Item(Item::Message {
                    at,
                    continues: false,
                })
            })
            .collect();
        let mut cache = Cache::default();
        cx.update(|cx| {
            cache.documents(&rows, &buffer, (&Mentions::default(), None), cx);
            assert_eq!(cache.computations, 100);
            cache.documents(&rows, &buffer, (&Mentions::new(["Alice"]), None), cx);
            assert_eq!(cache.computations, 101);
            cache.documents(&rows, &buffer, (&Mentions::new(["Alice", "Bob"]), None), cx);
            assert_eq!(
                cache.computations, 101,
                "an unmentioned join cannot reparse history"
            );
            cache.documents(&rows, &buffer, (&Mentions::new(["Bob"]), None), cx);
            assert_eq!(
                cache.computations, 102,
                "removing a mentioned nick refreshes only its row"
            );
        });
    }
}
