//! The crew card and one inspector row per Fleet API project.

use std::fmt::Write as _;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, div,
};
use subspace_client_fleet::model::{CrewContext, CrewMember, Task};
use subspace_client_irc::BufferKey;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::badge::Pill;
use subspace_framework::dashboard::{Tokens, feedback, workspace};

use crate::live::ship::{is_bidi_control, state_word};
use crate::{Irc, markup::PrRef, pull_requests};

use super::super::ship::state_tone;

type Facts = Vec<(&'static str, String)>;

pub(super) fn view(
    member: &CrewMember,
    current: Option<&Task>,
    buffer: &BufferKey,
    cx: &Context<Irc>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let title = nonempty(&member.name).unwrap_or(&member.id);
    let pill = Pill::new(
        "irc-crew-state",
        state_word(member.activity).unwrap_or("Unknown"),
    )
    .hue(state_tone(member.activity));
    let mut facts = Facts::new();
    optional(&mut facts, "Model", member.model.as_deref());
    task_facts(&mut facts, current);
    let empty = member
        .contexts
        .is_empty()
        .then(|| feedback::empty_line(&tokens, "irc-crew-no-projects", "No projects reported."));
    let projects = workspace::section("Projects", cx).children(empty).children(
        member
            .contexts
            .iter()
            .enumerate()
            .map(|(index, context)| project_row(context, index, buffer, cx)),
    );
    v_flex()
        .w_full()
        .min_w_0()
        .gap_4()
        .child(
            v_flex()
                .items_start()
                .min_w_0()
                .w_full()
                .gap_2()
                .child(
                    div()
                        .text_size(tokens.row_font)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tokens.fg_strong)
                        .child(visible(title)),
                )
                .child(pill)
                .children(fact_rows(facts, cx)),
        )
        .child(projects)
        .into_any_element()
}

fn fact_rows(facts: Facts, cx: &App) -> impl Iterator<Item = AnyElement> {
    facts.into_iter().map(|(label, value)| {
        workspace::fact(label, div().whitespace_normal().child(visible(&value)), cx)
            .into_any_element()
    })
}

fn project_row(
    context: &CrewContext,
    index: usize,
    buffer: &BufferKey,
    cx: &Context<Irc>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let key = project_key(context.project.as_deref(), index);
    let buffer = buffer.clone();
    let project = Project::of(context);
    let name = visible(&project.name);
    let actions = project
        .pull_request
        .zip(context.pull_request.as_ref())
        .map(|(link, pr)| {
            action_button(format!("irc-crew-pr:{key}"), Intent::Quiet, None, "Open PR")
                .accessibility_label(pr_button_name(pr.number, &name))
                .on_click(cx.listener(move |_, _, _, cx| pull_requests::open(&link, &buffer, cx)))
        });
    v_flex()
        .id(format!("irc-crew-project:{key}"))
        .w_full()
        .min_w_0()
        .items_start()
        .gap_1()
        .child(
            div()
                .text_size(tokens.secondary_size())
                .font_weight(FontWeight::SEMIBOLD)
                .child(name),
        )
        .children(fact_rows(project.facts, cx))
        .children(actions)
        .into_any_element()
}

fn project_key(project: Option<&str>, index: usize) -> String {
    project.filter(|name| !name.is_empty()).map_or_else(
        || format!("default:{index}"),
        |name| format!("named:{name}"),
    )
}

fn pr_button_name(number: u64, project: &str) -> String {
    format!("Open PR #{number} for {project}")
}

#[derive(Debug)]
pub(super) struct Project {
    pub name: String,
    pub facts: Facts,
    pub pull_request: Option<PrRef>,
}

impl Project {
    pub(super) fn of(context: &CrewContext) -> Self {
        let mut facts = Facts::new();
        optional(&mut facts, "State", state_word(context.activity));
        context
            .current
            .iter()
            .for_each(|task| task_facts(&mut facts, Some(task)));
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

fn task_facts(facts: &mut Facts, task: Option<&Task>) {
    if let Some(task) = task {
        facts.push((
            "Current task",
            nonempty(&task.summary).unwrap_or(&task.id).to_owned(),
        ));
        optional(facts, "Phase", Some(&task.phase));
        optional(facts, "Progress", Some(&task.progress));
    } else {
        facts.push(("Current task", "None".to_owned()));
    }
}

fn nonempty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

fn optional(facts: &mut Facts, label: &'static str, value: Option<&str>) {
    if let Some(value) = value.and_then(nonempty) {
        facts.push((label, value.to_owned()));
    }
}

/// Show control codes without giving them control over the surrounding text.
fn visible(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        if (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) || is_bidi_control(ch) {
            let _ = write!(result, "⟨U+{:04X}⟩", u32::from(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
#[path = "detail_tests.rs"]
mod tests;
