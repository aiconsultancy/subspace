//! Apply changes between bounded roster snapshots to a full-detail context.

use std::borrow::Cow;
use std::cmp::Ordering;

use subspace_client_fleet::model::{CrewContext, CrewMember, Git, PullRequest, Task};

use super::preview::{PartialFields, overlay_text};
use super::roster::{same_checkout, task_revision};
use crate::markup::PrRef;

macro_rules! overlay {
    ($shown:ident, $old:ident, $new:ident; $($field:ident),+ $(,)?) => {{$(
        if $old.$field != $new.$field {
            $shown.$field.clone_from(&$new.$field);
        }
    )+}};
}

pub(super) fn overlay_facts(
    shown: &mut Cow<'_, CrewMember>,
    old: &CrewContext,
    new: &CrewContext,
) -> PartialFields {
    let git_changed = git_changed(&old.git, &new.git);
    let pr_changed =
        old.pull_request != new.pull_request || old.pull_request_known != new.pull_request_known;
    let index = shown
        .contexts
        .iter()
        .position(|context| context.project == new.project);
    let fetched = index.and_then(|index| shown.contexts[index].current.as_ref());
    let change = TaskChange::between(fetched, old.current.as_ref(), new.current.as_ref());
    if !git_changed
        && !pr_changed
        && change == TaskChange::None
        && old.activity == new.activity
        && old.provisioned == new.provisioned
    {
        return PartialFields::default();
    }

    let contexts = &mut shown.to_mut().contexts;
    let Some(context) = index.and_then(|index| contexts.get_mut(index)) else {
        contexts.push(new.clone());
        return PartialFields::from_roster(new.current.as_ref());
    };
    if git_changed || pr_changed {
        overlay_checkout(context, old, new, git_changed, pr_changed);
    }
    overlay!(context, old, new; activity, provisioned);
    overlay_task(context, old, new, change)
}

#[derive(PartialEq, Eq)]
enum TaskChange {
    None,
    NewerFeed,
    RosterDelta,
}

impl TaskChange {
    fn between(fetched: Option<&Task>, start: Option<&Task>, feed: Option<&Task>) -> Self {
        match (task_order(fetched, feed), fetched, feed) {
            // An older feed cannot replace a newer reply, even across task IDs.
            (Some(Ordering::Greater), _, _) => Self::None,
            (Some(Ordering::Less), Some(fetched), Some(feed)) if fetched.id == feed.id => {
                Self::NewerFeed
            }
            _ if task_revision(start) != task_revision(feed) => Self::RosterDelta,
            _ => Self::None,
        }
    }
}

fn overlay_task(
    context: &mut CrewContext,
    old: &CrewContext,
    new: &CrewContext,
    change: TaskChange,
) -> PartialFields {
    match (change, &mut context.current, &old.current, &new.current) {
        (TaskChange::None, ..) => PartialFields::default(),
        (TaskChange::NewerFeed, Some(shown), _, Some(new)) => {
            shown.status = new.status;
            shown.updated_at = new.updated_at;
            overlay_text(shown, None, new)
        }
        (_, Some(shown), Some(old), Some(new)) if shown.id == new.id => {
            overlay_without_newer_revision(shown, old, new)
        }
        (_, shown, _, new) => {
            shown.clone_from(new);
            PartialFields::from_roster(new.as_ref())
        }
    }
}

fn task_order(fetched: Option<&Task>, feed: Option<&Task>) -> Option<Ordering> {
    Some(fetched?.updated_at?.cmp(&feed?.updated_at?))
}

fn overlay_without_newer_revision(shown: &mut Task, old: &Task, new: &Task) -> PartialFields {
    if task_revision(Some(shown)) == task_revision(Some(new)) {
        return PartialFields::default();
    }
    overlay!(shown, old, new; status, updated_at);
    overlay_text(shown, Some(old), new)
}

fn git_changed(old: &Option<Git>, new: &Option<Git>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => {
            let mut old = old.clone();
            old.checked_at = new.checked_at;
            &old != new
        }
        _ => old != new,
    }
}

fn overlay_checkout(
    context: &mut CrewContext,
    old: &CrewContext,
    new: &CrewContext,
    git_changed: bool,
    pr_changed: bool,
) {
    let moved = !same_checkout(context.git.as_ref(), new.git.as_ref())
        || !same_checkout(old.git.as_ref(), new.git.as_ref());
    if git_changed || moved {
        context.git.clone_from(&new.git);
    }
    if pr_changed || moved {
        overlay_pull_request(context, old, new, moved);
    }
}

fn overlay_pull_request(
    context: &mut CrewContext,
    old: &CrewContext,
    new: &CrewContext,
    moved: bool,
) {
    if !moved
        && let (Some(shown), Some(pr_old), Some(pr_new)) = (
            context.pull_request.as_mut(),
            old.pull_request.as_ref(),
            new.pull_request.as_ref(),
        )
        && let Some(key) = pr_key(pr_new)
        && pr_key(shown).as_ref() == Some(&key)
        && pr_key(pr_old).as_ref() == Some(&key)
    {
        overlay!(shown, pr_old, pr_new; title, url, state, checks);
        overlay!(context, old, new; pull_request_known);
    } else {
        context.pull_request.clone_from(&new.pull_request);
        context.pull_request_known = new.pull_request_known;
    }
}

fn pr_key(pr: &PullRequest) -> Option<String> {
    PrRef::from_url(&pr.url).map(|reference| reference.key)
}
