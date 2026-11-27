//! Finds the record a list row draws without scanning every record.

use std::collections::HashMap;

use crate::model::Record;

/// Each record's position in the listed records, by key. The list draws every
/// row of every open group on each frame, so a scan per row would cost the
/// square of the record count.
#[derive(Debug, Default)]
pub(crate) struct RecordIndex {
    positions: HashMap<String, usize>,
}

impl RecordIndex {
    /// The index of `records`. A key that repeats resolves to its first record,
    /// as a scan from the front would.
    pub(crate) fn new(records: &[Record]) -> Self {
        let mut positions = HashMap::with_capacity(records.len());
        for (position, record) in records.iter().enumerate() {
            positions.entry(record.key.clone()).or_insert(position);
        }
        Self { positions }
    }

    /// The record with `key` in `records`, which must be the records this index
    /// was built from. A stale position that no longer names `key` finds nothing.
    pub(crate) fn find<'a>(&self, records: &'a [Record], key: &str) -> Option<&'a Record> {
        let record = records.get(*self.positions.get(key)?)?;
        (record.key == key).then_some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::RecordIndex;
    use crate::model::Record;

    fn record(key: &str, title: &str) -> Record {
        Record {
            key: key.into(),
            repo: "ann/app".into(),
            title: title.into(),
            status: "success".into(),
            active: false,
            link: String::new(),
            properties: Vec::new(),
            sections: Vec::new(),
            run: None,
            approval: None,
            facts: None,
        }
    }

    #[test]
    fn a_key_finds_its_first_record_and_an_unknown_key_finds_none() {
        let records = [
            record("run:1", "first"),
            record("run:2", "second"),
            record("run:1", "repeat"),
        ];
        let index = RecordIndex::new(&records);
        assert_eq!(
            index.find(&records, "run:2").map(|r| &*r.title),
            Some("second")
        );
        assert_eq!(
            index.find(&records, "run:1").map(|r| &*r.title),
            Some("first")
        );
        assert!(index.find(&records, "run:3").is_none());
        assert!(index.find(&[], "run:1").is_none());
    }
}
