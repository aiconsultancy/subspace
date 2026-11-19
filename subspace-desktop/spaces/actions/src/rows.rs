//! Shared list and detail representations of Actions records.

use std::collections::BTreeMap;

use subspace_framework::dashboard::list::{Detail, Group, Row, Section, ServiceLink};
use subspace_framework::dashboard::{Glyph, Tone};

use crate::model::{self, Filters, Record, View};

pub(crate) fn tone(status: &str) -> Tone {
    match status {
        "success" | "approved" | "active" => Tone::Green,
        "failure" | "timed_out" | "rejected" | "error" => Tone::Red,
        "in_progress" => Tone::Blue,
        "waiting" | "queued" | "pending" | "requested" => Tone::Yellow,
        _ => Tone::Muted,
    }
}

pub(crate) fn row(record: &Record) -> Row {
    let mut trailing = vec![Glyph::new(record.status.clone(), tone(&record.status))];
    if record.active {
        trailing.push(Glyph::new("…", Tone::Blue));
    }
    Row {
        key: record.key.clone().into(),
        icon: Glyph::new("●", tone(&record.status)),
        id: record.run.as_ref().map(|run| run.id.to_string().into()),
        title: record.title.clone().into(),
        trailing,
        done: false,
        indent: 0,
    }
}

pub(crate) fn detail(record: &Record) -> Detail {
    Detail {
        kind: "GitHub Actions".into(),
        id: record.key.clone().into(),
        title: record.title.clone().into(),
        link: (!record.link.is_empty()).then(|| ServiceLink::new("GitHub", record.link.clone())),
        props: record
            .properties
            .iter()
            .map(|(key, value)| (key.clone().into(), value.clone().into()))
            .collect(),
        sections: record
            .sections
            .iter()
            .map(|(title, text)| Section::text(title.clone(), text.clone()))
            .collect(),
    }
}

pub(crate) fn groups(
    records: &[Record],
    repos: &[String],
    filters: &Filters,
    view: View,
) -> Vec<Group> {
    repos
        .iter()
        .flat_map(|repo| repo_groups(records, repo, filters, view))
        .collect()
}

fn repo_groups(records: &[Record], repo: &str, filters: &Filters, view: View) -> Vec<Group> {
    let mut grouped: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for record in records.iter().filter(|record| record.repo == repo) {
        if view != View::Runs || model::matches(record, filters) {
            let name = if view == View::Runs {
                model::group_name(record, filters.by_status)
            } else {
                repo.to_owned()
            };
            grouped.entry(name).or_default().push(row(record));
        }
    }
    if grouped.is_empty() {
        grouped.insert(repo.to_owned(), Vec::new());
    }
    grouped
        .into_iter()
        .map(|(name, rows)| Group {
            key: name.clone().into(),
            icon: Glyph::new("◇", Tone::Muted),
            name: name.into(),
            count: Some(rows.len().to_string().into()),
            rows,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{groups, tone};
    use crate::model::{Filters, View};
    use subspace_framework::dashboard::Tone;

    #[test]
    fn empty_repositories_keep_configured_order() {
        let groups = groups(
            &[],
            &["z/app".into(), "a/app".into()],
            &Filters::default(),
            View::Runs,
        );
        assert_eq!(
            groups
                .iter()
                .map(|group| group.name.as_ref())
                .collect::<Vec<_>>(),
            ["z/app", "a/app"]
        );
        assert_eq!(tone("failure"), Tone::Red);
        assert_eq!(tone("in_progress"), Tone::Blue);
        assert_eq!(tone("skipped"), Tone::Muted);
    }
}
