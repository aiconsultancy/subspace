//! What the three toolbar filters keep, what the status line counts, and the
//! mark the switcher tab shows.

use subspace_framework::core::copilot_agent_tasks::TaskRef;

use crate::model::{AgentRow, Kind, StateGroup, subject_key};

/// What the toolbar's three filters keep. `None` means every value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    pub kind: Option<Kind>,
    pub group: Option<StateGroup>,
    pub repo: Option<String>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether `row` passes every filter that is set.
    pub fn keeps(&self, row: &AgentRow) -> bool {
        self.kind.is_none_or(|kind| kind == row.kind)
            && self.group.is_none_or(|group| group == row.group())
            && self
                .repo
                .as_ref()
                .is_none_or(|repo| repo.eq_ignore_ascii_case(&row.repo))
    }
}

pub fn filter_rows(rows: Vec<AgentRow>, filters: &Filters) -> Vec<AgentRow> {
    rows.into_iter().filter(|row| filters.keeps(row)).collect()
}

/// The repositories the rows name, sorted, each once. The repository filter
/// offers these, so it never offers a repository with no rows.
pub fn repos(rows: &[AgentRow]) -> Vec<String> {
    let mut names: Vec<String> = rows
        .iter()
        .map(|row| row.repo.clone())
        .filter(|repo| !repo.is_empty())
        .collect();
    names.sort_by_key(|repo| repo.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

/// The subject key and the reference that record a monitor choice for a task
/// the file does not hold yet. A task found only through the task list has no
/// subject of its own, so it is filed under `other`.
pub fn monitor_ref(row: &AgentRow, monitor: bool) -> (String, TaskRef) {
    let subject = row
        .subject
        .as_ref()
        .map_or_else(|| format!("other:{}", row.id), subject_key);
    (
        subject,
        TaskRef {
            repo: row.repo.clone(),
            task_id: row.id.clone(),
            created_at: row.created_at.clone(),
            monitor,
        },
    )
}

/// The counts the status line reads: active rows and rows waiting for input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub active: usize,
    pub waiting: usize,
    /// Monitored cloud tasks that wait for input.
    pub watched_waiting: usize,
}

pub fn counts(rows: &[AgentRow]) -> Counts {
    let mut counts = Counts {
        total: rows.len(),
        ..Counts::default()
    };
    for row in rows {
        match row.group() {
            StateGroup::NeedsInput => {
                counts.active += 1;
                counts.waiting += 1;
                if row.monitor && row.kind == Kind::Cloud {
                    counts.watched_waiting += 1;
                }
            }
            StateGroup::Active => counts.active += 1,
            StateGroup::Finished => {}
        }
    }
    counts
}

/// The mark the switcher tab shows: `!` while a monitored task waits for an
/// answer, else how many rows are running, else nothing.
pub fn badge(counts: Counts) -> Option<String> {
    if counts.watched_waiting > 0 {
        return Some("!".to_owned());
    }
    (counts.active > 0).then(|| counts.active.to_string())
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{Subject, SubjectKind};

    use super::{Counts, Filters, badge, counts, filter_rows, monitor_ref, repos};
    use crate::model::tests::row;
    use crate::model::{Kind, StateGroup};

    #[test]
    fn each_filter_narrows_the_list_and_they_combine() {
        let mut local = row("local:s", Kind::Local, "open", "2026-10-02T00:00:00Z");
        local.repo = String::new();
        let rows = vec![
            row(
                "cloud:a",
                Kind::Cloud,
                "in_progress",
                "2026-10-02T00:00:00Z",
            ),
            row("cloud:b", Kind::Cloud, "completed", "2026-10-03T00:00:00Z"),
            local,
        ];
        let kept = |filters: &Filters| {
            filter_rows(rows.clone(), filters)
                .into_iter()
                .map(|row| row.key)
                .collect::<Vec<String>>()
        };
        assert_eq!(kept(&Filters::default()).len(), 3);
        assert_eq!(
            kept(&Filters {
                kind: Some(Kind::Cloud),
                ..Filters::default()
            }),
            ["cloud:a", "cloud:b"]
        );
        assert_eq!(
            kept(&Filters {
                group: Some(StateGroup::Finished),
                ..Filters::default()
            }),
            ["cloud:b", "local:s"],
            "a chat with no running turn is finished work, like a done task"
        );
        assert_eq!(
            kept(&Filters {
                repo: Some("owner/one".into()),
                ..Filters::default()
            }),
            ["cloud:a", "cloud:b"],
            "the repository filter ignores case"
        );
        assert_eq!(
            kept(&Filters {
                kind: Some(Kind::Cloud),
                group: Some(StateGroup::Active),
                repo: Some("Owner/one".into()),
            }),
            ["cloud:a"]
        );
        assert!(Filters::default().is_empty());
    }

    #[test]
    fn the_repository_filter_offers_each_repository_once() {
        let mut other = row("c", Kind::Cloud, "completed", "");
        other.repo = "owner/ONE".into();
        let mut blank = row("d", Kind::Local, "saved", "");
        blank.repo = String::new();
        let rows = vec![row("a", Kind::Cloud, "completed", ""), other, blank];
        assert_eq!(repos(&rows), ["Owner/one"]);
        assert!(repos(&[]).is_empty());
    }

    #[test]
    fn the_counts_and_the_badge_follow_what_needs_attention() {
        let mut waiting = row("ask", Kind::Cloud, "waiting_for_user", "");
        waiting.monitor = true;
        let rows = vec![
            waiting,
            row("run", Kind::Cloud, "in_progress", ""),
            row("done", Kind::Cloud, "completed", ""),
        ];
        let counted = counts(&rows);
        assert_eq!(counted.total, 3);
        assert_eq!(counted.active, 2);
        assert_eq!(counted.waiting, 1);
        assert_eq!(counted.watched_waiting, 1);
        assert_eq!(badge(counted).as_deref(), Some("!"));
        let running = counts(&[row("run", Kind::Cloud, "queued", "")]);
        assert_eq!(badge(running).as_deref(), Some("1"));
        assert_eq!(badge(Counts::default()), None);
        let unwatched = counts(&[row("ask", Kind::Cloud, "waiting_for_user", "")]);
        assert_eq!(
            badge(unwatched).as_deref(),
            Some("1"),
            "an unmonitored task that waits is only counted"
        );
    }

    #[test]
    fn a_monitor_choice_is_filed_under_the_subject_or_under_other() {
        let mut listed = row("cloud:t1", Kind::Cloud, "in_progress", "");
        listed.id = "t1".into();
        let (subject, entry) = monitor_ref(&listed, true);
        assert_eq!(subject, "other:t1");
        assert_eq!(entry.task_id, "t1");
        assert_eq!(entry.repo, "Owner/one");
        assert!(entry.monitor);
        listed.subject = Some(Subject {
            kind: SubjectKind::Chat,
            id: "abc".into(),
        });
        let (subject, entry) = monitor_ref(&listed, false);
        assert_eq!(subject, "chat:abc");
        assert!(!entry.monitor);
    }
}
