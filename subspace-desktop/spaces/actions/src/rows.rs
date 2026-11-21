//! Shared list and detail representations of Actions records.

use std::collections::BTreeMap;

use subspace_framework::dashboard::list::{Group, Row};
use subspace_framework::dashboard::{Glyph, Tone};
use subspace_framework::lucide::IconName;

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

/// A status's glyph and tone. Each kind of state has its own shape, so the
/// glyph reads without its colour.
pub(crate) fn status_look(status: &str) -> (IconName, Tone) {
    let icon = match status {
        "success" | "approved" | "active" | "published" => IconName::CircleCheck,
        "failure" | "timed_out" | "rejected" | "error" | "startup_failure" => IconName::CircleX,
        "in_progress" => IconName::LoaderCircle,
        "waiting" | "queued" | "pending" | "requested" | "action_required" => IconName::Clock,
        "cancelled" => IconName::CircleSlash,
        "skipped" | "neutral" | "stale" | "draft" => IconName::CircleDashed,
        _ => IconName::Circle,
    };
    (icon, tone(status))
}

/// A status as words, such as "in progress" for `in_progress`.
pub(crate) fn status_word(status: &str) -> String {
    status.replace('_', " ")
}

/// What item search matches beyond the title: the run's branch, actor,
/// event and status, which the title no longer carries.
pub(crate) fn search_meta(record: &Record) -> String {
    let mut words = vec![status_word(&record.status)];
    if let Some(run) = &record.run {
        words.extend([run.branch.clone(), run.actor.clone(), run.workflow.clone()]);
    }
    if let Some(facts) = &record.facts {
        words.push(facts.event.clone());
    }
    words.retain(|word| !word.is_empty());
    words.join(" ")
}

/// A row's accessible name: the title and status and, for a run, the
/// branch, actor, event and age its lanes show.
pub(crate) fn accessible_label(record: &Record) -> String {
    let mut parts = vec![record.title.clone(), status_word(&record.status)];
    if let (Some(run), Some(facts)) = (&record.run, &record.facts) {
        parts.extend([
            run.branch.clone(),
            run.actor.clone(),
            facts.event.clone(),
            facts.age.clone(),
        ]);
    }
    parts.retain(|part| !part.is_empty());
    parts.join(", ")
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
    use super::{accessible_label, groups, search_meta, status_look, status_word, tone};
    use crate::model::{Filters, View};
    use subspace_framework::dashboard::Tone;
    use subspace_framework::lucide::IconName;

    #[test]
    fn each_kind_of_status_has_its_own_glyph_and_words() {
        let shapes = [
            "success",
            "failure",
            "in_progress",
            "waiting",
            "cancelled",
            "skipped",
        ]
        .map(|status| status_look(status).0);
        for (at, shape) in shapes.iter().enumerate() {
            assert!(!shapes[at + 1..].contains(shape), "{at} shares a glyph");
        }
        assert_eq!(status_look("timed_out"), (IconName::CircleX, Tone::Red));
        assert_eq!(status_look("queued"), (IconName::Clock, Tone::Yellow));
        assert_eq!(status_look("anything"), (IconName::Circle, Tone::Muted));
        assert_eq!(status_word("in_progress"), "in progress");
        assert_eq!(status_word("success"), "success");
    }

    #[test]
    fn search_finds_a_run_by_its_branch_actor_and_event() {
        let run = crate::test_support::run("octo/app");
        assert_eq!(search_meta(&run), "success main ann CI push");
        let notice = crate::load::mapping::notice("octo/app", "gone", "Gone");
        assert_eq!(search_meta(&notice), "notice");
        assert_eq!(accessible_label(&run), "CI, success, main, ann, push, 4h");
        assert_eq!(accessible_label(&notice), "Gone, notice");
    }

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
