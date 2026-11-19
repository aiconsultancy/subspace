//! Which records the viewer shows: a pure decision the view draws.

use subspace_core::log::{Level, Record};

/// The viewer's three filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// Records at this level or more important.
    pub level: Level,
    /// One target, or every target.
    pub target: Option<String>,
    /// Words that must all appear in the line, ignoring case.
    pub text: String,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            level: Level::Info,
            target: None,
            text: String::new(),
        }
    }
}

/// The records `filter` keeps, in order.
pub fn visible<'a>(records: &'a [Record], filter: &Filter) -> Vec<&'a Record> {
    let words: Vec<String> = filter
        .text
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    records
        .iter()
        .filter(|record| record.level.passes(filter.level))
        .filter(|record| {
            filter
                .target
                .as_deref()
                .is_none_or(|target| record.target == target)
        })
        .filter(|record| words.is_empty() || matches_words(record, &words))
        .collect()
}

fn matches_words(record: &Record, words: &[String]) -> bool {
    let line = record.line().to_lowercase();
    words.iter().all(|word| line.contains(word))
}

#[cfg(test)]
mod tests {
    use subspace_core::log::{Level, Record};

    use super::{Filter, visible};

    fn record(level: Level, target: &str, message: &str) -> Record {
        Record {
            seq: 0,
            at: chrono::Utc::now(),
            level,
            target: target.into(),
            message: message.into(),
            fields: vec![("status".into(), "429".into())],
        }
    }

    #[test]
    fn the_filters_narrow_by_level_target_and_words() {
        let records = vec![
            record(Level::Info, "app", "started subspace"),
            record(Level::Debug, "http", "GET host /path"),
            record(Level::Warn, "http", "POST host /query"),
            record(Level::Error, "azure-costs", "read failed"),
        ];
        let shown = visible(&records, &Filter::default());
        assert_eq!(shown.len(), 3, "debug is hidden at the default level");
        let http = Filter {
            target: Some("http".into()),
            level: Level::Trace,
            ..Filter::default()
        };
        assert_eq!(visible(&records, &http).len(), 2);
        let words = Filter {
            level: Level::Trace,
            text: "HOST 429 post".into(),
            ..Filter::default()
        };
        let found = visible(&records, &words);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].message, "POST host /query");
    }
}
