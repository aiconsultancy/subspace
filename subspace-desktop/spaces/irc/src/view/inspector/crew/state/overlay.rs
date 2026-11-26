//! Apply changes between bounded roster snapshots to a full-detail context.

use std::borrow::Cow;

use subspace_client_fleet::model::{CrewContext, CrewMember, Git, PullRequest};

use super::preview::overlay_text;
use super::roster::{same_checkout, task_revision};
use crate::markup::PrRef;

macro_rules! overlay {
    ($shown:ident, $old:ident, $new:ident; $($field:ident),+ $(,)?) => {{$(
        if $old.$field != $new.$field {
            $shown.$field.clone_from(&$new.$field);
        }
    )+}};
}

pub(super) fn overlay_facts(shown: &mut Cow<'_, CrewMember>, old: &CrewContext, new: &CrewContext) {
    let git_changed = git_changed(&old.git, &new.git);
    let pr_changed =
        old.pull_request != new.pull_request || old.pull_request_known != new.pull_request_known;
    let task_changed = task_revision(old.current.as_ref()) != task_revision(new.current.as_ref());
    if !(git_changed || pr_changed || task_changed)
        && old.activity == new.activity
        && old.provisioned == new.provisioned
    {
        return;
    }

    let contexts = &mut shown.to_mut().contexts;
    let Some(context) = contexts.iter_mut().find(|ctx| ctx.project == new.project) else {
        contexts.push(new.clone());
        return;
    };
    if git_changed || pr_changed {
        overlay_checkout(context, old, new, git_changed, pr_changed);
    }
    if task_changed {
        match (&mut context.current, &old.current, &new.current) {
            (Some(shown), Some(old), Some(new)) if shown.id == new.id => {
                overlay_text(shown, old, new);
                overlay!(shown, old, new; status, updated_at)
            }
            (shown, _, new) => shown.clone_from(new),
        }
    }
    overlay!(context, old, new; activity, provisioned);
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
