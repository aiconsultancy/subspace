//! The bounded member from the ship feed and the facts that drive a reread.

use subspace_client_fleet::model::{CrewContext, CrewMember, ShipDetail, Task, TaskStatus};

#[derive(PartialEq, Eq)]
pub(in super::super) enum Roster {
    Present(Box<CrewMember>),
    Gone,
    Unavailable,
}

impl Roster {
    pub(in super::super) fn find(detail: Option<ShipDetail>, member: &str) -> Self {
        let Some(detail) = detail.filter(|detail| detail.ship.available && !detail.detail_stale)
        else {
            return Self::Unavailable;
        };
        detail
            .crew_members
            .into_iter()
            .find(|entry| entry.id == member)
            .map_or(Self::Gone, |member| Self::Present(Box::new(member)))
    }

    pub(in super::super) fn member_returned(&self, next: &Self) -> bool {
        !matches!(self, Self::Present(_)) && matches!(next, Self::Present(_))
    }

    pub(super) fn same_assignment(&self, next: &Self) -> bool {
        let (Self::Present(old), Self::Present(new)) = (self, next) else {
            return false;
        };
        old.id == new.id && old.backend == new.backend && assignments(old).eq(assignments(new))
    }

    pub(super) fn same_task_revision(&self, next: &Self) -> bool {
        let (Self::Present(old), Self::Present(new)) = (self, next) else {
            return false;
        };
        task_revision(old.current_task()) == task_revision(new.current_task())
            && old.contexts.iter().all(|ctx| {
                let next =
                    current_in(new, ctx.project.as_deref()).and_then(|ctx| ctx.current.as_ref());
                task_revision(ctx.current.as_ref()) == task_revision(next)
            })
    }
}

fn assignments(member: &CrewMember) -> impl Iterator<Item = (Option<&str>, &str)> {
    let mut tasks: Vec<_> = member
        .contexts
        .iter()
        .filter_map(|ctx| Some((ctx.project.as_deref(), ctx.current.as_ref()?.id.as_str())))
        .collect();
    tasks.sort_unstable();
    tasks.into_iter()
}

pub(super) fn current_in<'a>(member: &'a CrewMember, key: Option<&str>) -> Option<&'a CrewContext> {
    member
        .contexts
        .iter()
        .find(|ctx| ctx.project.as_deref() == key)
}

fn task_revision(task: Option<&Task>) -> Option<(&str, TaskStatus, Option<i64>)> {
    task.map(|task| (task.id.as_str(), task.status, task.updated_at))
}

pub(super) fn same_task_facts(old: Option<&Task>, new: Option<&Task>) -> bool {
    fn text(task: Option<&Task>) -> Option<(&str, &str, &str)> {
        task.map(|task| {
            (
                task.summary.as_str(),
                task.phase.as_str(),
                task.progress.as_str(),
            )
        })
    }
    task_revision(old) == task_revision(new) && text(old) == text(new)
}
