//! Compact inspector facts about a project's checkout and pull request.

use subspace_client_fleet::model::CrewContext;

use super::{Facts, optional};
use crate::markup::PrRef;

#[derive(Debug)]
pub(super) struct Project {
    pub(super) name: String,
    pub(super) facts: Facts,
    pub(super) pull_request: Option<PrRef>,
}

impl Project {
    pub(super) fn of(context: &CrewContext) -> Self {
        let mut facts = Facts::new();
        git_facts(&mut facts, context);
        let pull_request = pr_facts(&mut facts, context);
        Self {
            name: context
                .project
                .clone()
                .unwrap_or_else(|| "Default project".to_owned()),
            facts,
            pull_request,
        }
    }
}

fn git_facts(facts: &mut Facts, context: &CrewContext) {
    let Some(git) = &context.git else {
        let text = match context.provisioned.as_str() {
            "unprovisioned" | "pending" => "No checkout yet.",
            _ => "Git state unavailable.",
        };
        facts.push(("Git", text.to_owned()));
        return;
    };
    optional(facts, "Repository", git.repository.as_deref());
    optional(facts, "Head", git.head.as_deref());
    optional(facts, "Base", git.base.as_deref());
    let branch = git
        .branch
        .as_deref()
        .or_else(|| (git.head.is_some() && git.error.is_none()).then_some("Detached HEAD"));
    optional(facts, "Branch", branch);
    facts.extend(
        [
            ("Ahead", git.ahead),
            ("Behind", git.behind),
            ("Changed paths", git.changed_files),
        ]
        .into_iter()
        .filter_map(|(label, count)| Some((label, count?.to_string()))),
    );
    optional(facts, "Git", git.error.as_deref());
}

fn pr_facts(facts: &mut Facts, context: &CrewContext) -> Option<PrRef> {
    let Some(pr) = &context.pull_request else {
        if context.pull_request_known {
            facts.push(("Pull request", "None for this branch.".to_owned()));
        }
        return None;
    };
    facts.push((
        "Pull request",
        if pr.title.is_empty() {
            format!("#{}", pr.number)
        } else {
            format!("#{} {}", pr.number, pr.title)
        },
    ));
    optional(facts, "PR state", Some(&pr.state));
    optional(facts, "Checks", Some(&pr.checks));
    let pull_request = PrRef::from_url(&pr.url);
    optional(facts, "PR URL", pull_request.is_none().then_some(&pr.url));
    pull_request
}
