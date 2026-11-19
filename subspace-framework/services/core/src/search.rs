//! Item search over a dashboard's loaded documents (BM25F). Fields are
//! scored separately and weighted;
//! the last query word matches as a prefix while typing; documents that match
//! more query words rank higher; a numeric query matches document numbers by
//! prefix. No network and no dependency on the UI.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use unicode_normalization::UnicodeNormalization as _;

const K1: f64 = 1.2;
const B: f64 = 0.75;
const FIELD_COUNT: usize = 3;
const IDF_SMOOTHING: f64 = 0.5;
const EXACT_NUMBER_BONUS: f64 = 100.0;
const PREFIX_NUMBER_BONUS: f64 = 10.0;
/// Weights of the `title`, `parent` and `meta` fields, in that order.
const FIELD_WEIGHTS: [f64; FIELD_COUNT] = [1.0, 0.35, 0.5];
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "the", "of", "to", "in", "for", "with", "on", "or", "by", "is",
];

/// The number of results the palette shows (REQ-05).
pub const MAX_RESULTS: usize = 8;

/// Lower case with accents removed: NFKD, then combining marks dropped.
pub fn fold(text: &str) -> String {
    text.nfkd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Words of `text`: folded, split at anything but letters and digits, stop
/// words removed.
pub fn tokenize(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty() && !STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

/// One searchable thing. Dashboards map their items to documents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchDoc {
    /// Unique within the index; ties rank by id, numbers compared as numbers.
    pub id: String,
    /// Matched by a numeric query (`1258950`, `#123`), such as a work item ID
    /// or a pull request number.
    pub number: Option<u64>,
    pub title: String,
    /// The parent's title: makes children findable, below the parent itself.
    pub parent: String,
    /// Type, state, people, labels.
    pub meta: String,
}

// qual:allow(dry, boilerplate) reason: "Three small native setters preserve the public SearchDoc builder API; generating them would add a dependency without reducing meaningful complexity."
impl SearchDoc {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn number(mut self, number: u64) -> Self {
        self.number = Some(number);
        self
    }

    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = parent.into();
        self
    }

    pub fn meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = meta.into();
        self
    }

    fn fields(&self) -> [&str; 3] {
        [&self.title, &self.parent, &self.meta]
    }
}

/// A match: the document's position in the index and its score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: f64,
}

struct Indexed {
    terms: [HashMap<String, u32>; FIELD_COUNT],
    lengths: [usize; FIELD_COUNT],
}

/// The index over a fixed set of documents. Build it again when they change.
pub struct SearchIndex {
    docs: Vec<SearchDoc>,
    indexed: Vec<Indexed>,
    average: [f64; FIELD_COUNT],
    frequency: [HashMap<String, u32>; FIELD_COUNT],
    vocabulary: BTreeSet<String>,
}

impl SearchIndex {
    pub fn new(docs: Vec<SearchDoc>) -> Self {
        let indexed: Vec<Indexed> = docs
            .iter()
            .map(|doc| {
                let tokens = doc.fields().map(tokenize);
                Indexed {
                    lengths: [tokens[0].len(), tokens[1].len(), tokens[2].len()],
                    terms: tokens.map(|tokens| {
                        let mut counts = HashMap::new();
                        for token in tokens {
                            *counts.entry(token).or_insert(0) += 1;
                        }
                        counts
                    }),
                }
            })
            .collect();

        let mut average = [1.0; FIELD_COUNT];
        let mut frequency: [HashMap<String, u32>; FIELD_COUNT] = Default::default();
        let mut vocabulary = BTreeSet::new();
        for field in 0..FIELD_COUNT {
            let total: usize = indexed.iter().map(|doc| doc.lengths[field]).sum();
            let mean = total as f64 / indexed.len().max(1) as f64;
            average[field] = if mean > 0.0 { mean } else { 1.0 };
            for doc in &indexed {
                for term in doc.terms[field].keys() {
                    *frequency[field].entry(term.clone()).or_insert(0) += 1;
                    vocabulary.insert(term.clone());
                }
            }
        }
        Self {
            docs,
            indexed,
            average,
            frequency,
            vocabulary,
        }
    }

    pub fn docs(&self) -> &[SearchDoc] {
        &self.docs
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    fn idf(&self, field: usize, term: &str) -> f64 {
        let n = f64::from(self.frequency[field].get(term).copied().unwrap_or(0));
        let count = self.docs.len() as f64;
        (1.0 + (count - n + IDF_SMOOTHING) / (n + IDF_SMOOTHING)).ln()
    }

    fn term_score(&self, doc: &Indexed, term: &str) -> f64 {
        let mut score = 0.0;
        for (field, weight) in FIELD_WEIGHTS.iter().enumerate() {
            let Some(&tf) = doc.terms[field].get(term) else {
                continue;
            };
            let tf = f64::from(tf);
            let norm = tf + K1 * (1.0 - B + B * doc.lengths[field] as f64 / self.average[field]);
            score += weight * self.idf(field, term) * (tf * (K1 + 1.0)) / norm;
        }
        score
    }

    /// A query word stands for itself, or (the last word while typing, or a
    /// word that is no term) for the terms it prefixes.
    fn expand(&self, word: &str, is_last: bool) -> Vec<String> {
        let exact = self.vocabulary.contains(word);
        if exact && !is_last {
            return vec![word.to_string()];
        }
        let prefixed: Vec<String> = self
            .vocabulary
            .range(word.to_string()..)
            .take_while(|term| term.starts_with(word))
            .cloned()
            .collect();
        match (prefixed.is_empty(), exact) {
            (false, _) => prefixed,
            (true, true) => vec![word.to_string()],
            (true, false) => Vec::new(),
        }
    }

    fn score_document(
        &self,
        index: usize,
        expansions: &[Vec<String>],
        word_count: usize,
        id_query: Option<&str>,
    ) -> Option<Hit> {
        let doc = &self.indexed[index];
        let mut score = 0.0;
        let mut matched = 0usize;
        for terms in expansions {
            // The best expansion per word, so one prefix cannot add up many terms.
            let best = terms
                .iter()
                .map(|term| self.term_score(doc, term))
                .fold(0.0, f64::max);
            matched += usize::from(best > 0.0);
            score += best;
        }
        score *= (matched as f64 / word_count as f64).powi(2);
        score += self.number_bonus(index, id_query);
        (score > 0.0).then_some(Hit { index, score })
    }

    fn number_bonus(&self, index: usize, id_query: Option<&str>) -> f64 {
        let Some(id) = id_query else {
            return 0.0;
        };
        let Some(number) = self.docs[index].number else {
            return 0.0;
        };
        let number = number.to_string();
        if !number.starts_with(id) {
            return 0.0;
        }
        if number == id {
            EXACT_NUMBER_BONUS
        } else {
            PREFIX_NUMBER_BONUS
        }
    }

    /// The best `limit` matches for `query`, best first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let words = tokenize(query);
        if words.is_empty() {
            return Vec::new();
        }
        let last_is_prefix = !query.ends_with(char::is_whitespace);
        let expansions: Vec<Vec<String>> = words
            .iter()
            .enumerate()
            .map(|(i, word)| self.expand(word, last_is_prefix && i == words.len() - 1))
            .collect();
        let trimmed = query.trim();
        let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
        let id_query =
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(digits);

        let mut hits: Vec<Hit> = self
            .indexed
            .iter()
            .enumerate()
            .filter_map(|(index, _)| self.score_document(index, &expansions, words.len(), id_query))
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| natural_cmp(&self.docs[a.index].id, &self.docs[b.index].id))
        });
        hits.truncate(limit);
        hits
    }
}

/// Compares text with runs of digits as numbers (`localeCompare` with
/// `numeric: true`, without the locale).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut run = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        run.push(c);
                    }
                    run
                };
                let (x, y) = (take(&mut a), take(&mut b));
                let (xt, yt) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = xt.len().cmp(&yt.len()).then_with(|| xt.cmp(yt));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x
                    .to_lowercase()
                    .cmp(y.to_lowercase())
                    .then_with(|| x.cmp(&y));
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchDoc, SearchIndex, natural_cmp, tokenize};

    /// The web test's items, mapped as the board's `boardDocs` maps them:
    /// the short title, the parent's short title, and type, state and person.
    fn index() -> SearchIndex {
        let doc = |id: u64, title: &str, parent: &str, kind: &str, state: &str| {
            SearchDoc::new(id.to_string(), title)
                .number(id)
                .parent(parent)
                .meta(format!("{kind} {state} Doe, Jane"))
        };
        SearchIndex::new(vec![
            doc(
                1204258,
                "Write the crew onboarding guide for the job board",
                "",
                "Product Backlog Item",
                "New",
            ),
            doc(
                1204302,
                "Add a reader role to the sample module",
                "Write the crew onboarding guide for the job board",
                "Task",
                "To Do",
            ),
            doc(
                1258950,
                "Plan the job route for the harbour demo",
                "",
                "Task",
                "Done",
            ),
            doc(
                1259313,
                "Tidy the sample lint rules",
                "",
                "Task",
                "To Do",
            ),
        ])
    }

    fn ids(index: &SearchIndex, query: &str) -> Vec<String> {
        index
            .search(query, 20)
            .into_iter()
            .map(|hit| index.docs()[hit.index].id.clone())
            .collect()
    }

    #[test]
    fn tokenize_lowercases_strips_accents_and_stop_words() {
        assert_eq!(
            tokenize("The Café-Onboarding of Ops"),
            ["cafe", "onboarding", "ops"]
        );
        assert_eq!(
            tokenize("ﬁle Ｗide"),
            ["file", "wide"],
            "NFKD folds compatibility forms"
        );
    }

    #[test]
    fn the_last_word_matches_as_a_prefix_while_typing() {
        let index = index();
        assert_eq!(ids(&index, "onbo")[0], "1204258");
        // "job" is a whole term, so it matches exactly (not "journey");
        // matching both words (the child through its parent's title) outranks
        // matching one.
        assert_eq!(ids(&index, "job onbo"), ["1204258", "1204302", "1258950"]);
    }

    #[test]
    fn parent_titles_make_children_findable_below_the_parent() {
        assert_eq!(ids(&index(), "onboarding"), ["1204258", "1204302"]);
    }

    #[test]
    fn items_matching_more_words_rank_first() {
        assert_eq!(ids(&index(), "azure journey")[0], "1258950");
    }

    #[test]
    fn numbers_match_ids_by_prefix_and_exact_ids_rank_first() {
        let index = index();
        assert_eq!(ids(&index, "1258950")[0], "1258950");
        assert_eq!(ids(&index, "#1258950")[0], "1258950");
        assert_eq!(ids(&index, "49043"), ["1204302"]);
    }

    #[test]
    fn empty_queries_return_nothing_and_the_limit_holds() {
        let index = index();
        assert!(ids(&index, "  ").is_empty());
        assert!(ids(&index, "the of").is_empty());
        assert_eq!(index.search("task", 2).len(), 2);
    }

    #[test]
    fn ids_compare_numbers_as_numbers() {
        use std::cmp::Ordering::Less;
        assert_eq!(natural_cmp("item-9", "item-10"), Less);
        assert_eq!(natural_cmp("a", "B"), Less);
    }
}
