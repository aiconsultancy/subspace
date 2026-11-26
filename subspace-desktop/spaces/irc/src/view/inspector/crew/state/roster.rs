//! The bounded member from the ship feed and the facts that drive a reread.

use subspace_client_fleet::model::{CrewContext, CrewMember, Git, ShipDetail, Task, TaskStatus};

use super::preview;

#[derive(PartialEq, Eq)]
pub(in super::super) enum Roster {
    Present(Box<CrewMember>),
    Gone,
    Unavailable,
}

impl Roster {
    pub(in super::super) fn context_count(&self) -> usize {
        match self {
            Self::Present(member) => member.contexts.len(),
            _ => 0,
        }
    }

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

    pub(super) fn needs_read(&self, next: &Self, full: Option<&CrewMember>) -> bool {
        !self.same_task_revision(next) || self.divergent_preview(next, full)
    }

    fn same_task_revision(&self, next: &Self) -> bool {
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

    fn divergent_preview(&self, next: &Self, full: Option<&CrewMember>) -> bool {
        let (Self::Present(old), Self::Present(new)) = (self, next) else {
            return false;
        };
        old.contexts.iter().any(|context| {
            let key = context.project.as_deref();
            let old = context.current.as_ref();
            let new = current_in(new, key).and_then(|context| context.current.as_ref());
            let full = full
                .and_then(|member| current_in(member, key))
                .and_then(|context| context.current.as_ref());
            matches!((old, new), (Some(old), Some(new)) if preview::diverged(old, new, full))
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

pub(super) fn task_revision(task: Option<&Task>) -> Option<(&str, TaskStatus, Option<i64>)> {
    task.map(|task| (task.id.as_str(), task.status, task.updated_at))
}

pub(super) fn same_checkout(left: Option<&Git>, right: Option<&Git>) -> bool {
    fn identity(git: &Git) -> (Option<&str>, Option<&str>, Option<&str>) {
        (
            git.repository.as_deref(),
            git.branch.as_deref(),
            git.head.as_deref(),
        )
    }
    left.map(identity) == right.map(identity)
}
