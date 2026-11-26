//! The crew card and one inspector row per Fleet API project.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, Styled as _, div,
};
use subspace_client_fleet::model::{CrewContext, CrewMember};
use subspace_client_irc::BufferKey;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::badge::Pill;
use subspace_framework::dashboard::{Tokens, feedback, workspace};

use crate::live::ship::state_word;
use crate::{Irc, pull_requests};

use super::super::ship::state_tone;

mod observations;
mod task_text;
use observations::Project;
pub(super) use task_text::TaskPages;
use task_text::visible;

type Facts = Vec<(&'static str, String)>;

pub(super) fn view(
    member: &CrewMember,
    pages: &TaskPages,
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
    let empty = member
        .contexts
        .is_empty()
        .then(|| feedback::empty_line(&tokens, "irc-crew-no-projects", "No projects reported."));
    let projects = workspace::section("Projects", cx).children(empty).children(
        member
            .contexts
            .iter()
            .enumerate()
            .map(|(index, context)| project_row(context, index, pages, buffer, cx)),
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
                .children(fact_rows(member_facts(member), cx)),
        )
        .child(projects)
        .into_any_element()
}

fn member_facts(member: &CrewMember) -> Facts {
    let mut facts = Facts::new();
    optional(&mut facts, "Model", member.model.as_deref());
    let (id, status) = member.current_task().map_or(("None", None), |task| {
        (task.id.as_str(), Some(task_text::status_label(task.status)))
    });
    optional(&mut facts, "Current task", Some(id));
    optional(&mut facts, "Task status", status);
    facts
}

fn fact_rows(facts: Facts, cx: &App) -> impl Iterator<Item = AnyElement> {
    facts.into_iter().map(|(label, value)| {
        workspace::fact(label, fact_text(visible(&value)), cx).into_any_element()
    })
}

fn fact_text(value: String) -> impl IntoElement {
    let text = SharedString::from(value);
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .child(text.clone())
        .test_text(text)
}

fn project_row(
    context: &CrewContext,
    index: usize,
    pages: &TaskPages,
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
        .children(project_state(context, cx))
        .children(task_text::rows(context.current.as_ref(), &key, pages, cx))
        .children(fact_rows(project.facts, cx))
        .children(actions)
        .into_any_element()
}

fn project_state(context: &CrewContext, cx: &App) -> Option<AnyElement> {
    state_word(context.activity).map(|state| workspace::fact("State", state, cx).into_any_element())
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

fn nonempty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

fn optional(facts: &mut Facts, label: &'static str, value: Option<&str>) {
    if let Some(value) = value.and_then(nonempty) {
        facts.push((label, value.to_owned()));
    }
}

#[cfg(test)]
#[path = "detail_tests.rs"]
mod tests;
