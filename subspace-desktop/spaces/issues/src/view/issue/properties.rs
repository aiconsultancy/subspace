use super::panel::{self, Fact};
use crate::{Issues, editing::EditTarget, model::IssuePriority};
use gpui_kit::Styled as _;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, TestSupportExt as _,
};
use subspace_api_client::model::{IssueDetail, Person, Project};
use subspace_framework::dashboard::action::{Intent, action_button, icon_button};
use subspace_framework::dashboard::{Glyph, Tokens, avatar, truncated};
use subspace_framework::lucide::IconName;

pub(super) fn facts(detail: &IssueDetail, projects: &[Project]) -> Vec<Fact> {
    let f = &detail.fields;
    let labels = if f.labels.is_empty() {
        "No labels".into()
    } else {
        f.labels
            .iter()
            .map(|label| label.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut facts = vec![
        Fact::new("State", &f.state.name),
        Fact::new(
            "Priority",
            IssuePriority::from_value(f.priority).look().label,
        ),
        Fact::new(
            "Assignee",
            f.assignee
                .as_ref()
                .map_or_else(|| "Unassigned".into(), person),
        ),
        Fact::new("Labels", labels),
        Fact::new("Due date", f.due_date.as_deref().unwrap_or("No due date")),
    ];
    facts.extend(context(detail, projects));
    // The projects contract exposes no estimate/cycle capability flags. Populated
    // issue values are the only positive signal; never invent empty controls.
    facts.extend(f.estimate.map(|n| Fact::new("Estimate", n.to_string())));
    facts.extend(
        f.cycle
            .as_ref()
            .map(|c| Fact::new("Cycle", format!("Cycle {} · {}", c.number, c.state))),
    );
    facts
}

fn context(detail: &IssueDetail, projects: &[Project]) -> Vec<Fact> {
    let f = &detail.fields;
    let project = projects.iter().find(|project| project.id == f.project_id);
    vec![
        Fact::new(
            "Project",
            project.map_or_else(
                || f.project_key.clone(),
                |p| format!("{} · {}", p.key, p.name),
            ),
        ),
        Fact::new(
            "Milestone",
            if f.milestone_id.is_some() {
                "Unavailable"
            } else {
                "No milestone"
            },
        ),
        Fact::new(
            "Parent",
            detail.parent.as_ref().map_or_else(
                || {
                    if f.parent_id.is_some() {
                        "Unavailable"
                    } else {
                        "No parent"
                    }
                    .into()
                },
                |p| p.identifier.clone(),
            ),
        ),
        Fact::new(
            "Type",
            if f.kind == "tracker" {
                "Tracker"
            } else {
                "Issue"
            },
        ),
    ]
}

pub(super) fn person(person: &Person) -> String {
    if person.is_former_member {
        format!("{} (former member)", person.display_name)
    } else {
        person.display_name.clone()
    }
}

pub(super) fn render(
    detail: &IssueDetail,
    issues: &Issues,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let rows = facts(detail, &issues.projects).into_iter().map(|mut fact| {
        if fact.label == "Milestone"
            && let Some(name) = milestone_label(issues)
        {
            fact.value = name.to_owned();
        }
        let inline = crate::editing::view::inline_row(issues, fact.label, tokens, cx)
            .or_else(|| crate::project_move::view::inline(issues, fact.label, tokens, cx));
        if let Some(inline) = inline {
            return panel::fact_row(fact.label, "Editing".into(), "property", tokens)
                .child(inline)
                .into_any_element();
        }
        let edit = edit_action(issues, &fact, detail, cx);
        if fact.label == "Parent"
            && let Some(parent) = &detail.parent
        {
            let identifier = parent.identifier.clone();
            return panel::fact_row(fact.label, fact.value, "property", tokens)
                .child(
                    h_flex().flex_1().min_w_0().justify_start().child(
                        action_button(
                            "issue-property-parent",
                            Intent::Quiet,
                            Some(IconName::ArrowUpRight),
                            identifier.clone(),
                        )
                        .max_w_full()
                        .accessibility_label(format!("Open parent {identifier}"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.focus_list(window, cx);
                                this.open_detail(identifier.clone(), cx);
                            },
                        )),
                    ),
                )
                .children(edit)
                .into_any_element();
        }
        let value = value(&fact, detail, tokens);
        panel::fact_row(fact.label, fact.value, "property", tokens)
            .child(value)
            .children(edit)
            .into_any_element()
    });
    gpui_kit::div()
        .id("issue-properties")
        .aria_label("Properties")
        .test_support()
        .child(
            panel::group("Properties", IconName::SlidersHorizontal, tokens)
                .child(panel::grid().children(rows)),
        )
        .into_any_element()
}

fn milestone_label(issues: &Issues) -> Option<&str> {
    let open = issues.issue_view.snapshot.as_ref()?;
    open.milestone_name()
        .or_else(|| open.milestone_loading().then_some("Loading milestone…"))
}

fn edit_action(
    issues: &Issues,
    fact: &Fact,
    detail: &IssueDetail,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    if !issues.can_edit_issue() {
        return None;
    }
    let target = EditTarget::property(fact.label, detail.fields.kind == "tracker")?;
    Some(
        icon_button(
            SharedString::from(format!("issue-property-edit:{}", fact.label)),
            Intent::Quiet,
            IconName::Pencil,
            format!("Edit {}…", fact.label.to_lowercase()),
        )
        .compact()
        .disabled(issues.inline_field_active())
        .on_click(cx.listener(move |this, _, window, cx| this.edit_issue_field(target, window, cx)))
        .into_any_element(),
    )
}

fn value(fact: &Fact, detail: &IssueDetail, tokens: &Tokens) -> AnyElement {
    let id = SharedString::from(format!("issue-property-value:{}", fact.label));
    match fact.label {
        "State" => {
            let (icon, tone) = crate::view::lanes::state_icon(crate::model::StateKind::parse(
                &detail.fields.state.kind,
            ));
            marked(
                Icon::from(icon).xsmall().text_color(tokens.tone(tone)),
                id,
                fact,
                tokens,
            )
        }
        "Priority" => {
            let priority = IssuePriority::from_value(detail.fields.priority).look();
            let icon = Icon::from(priority.icon)
                .xsmall()
                .text_color(tokens.tone(priority.tone));
            marked(icon, id, fact, tokens)
        }
        "Assignee" if detail.fields.assignee.is_some() => {
            marked(avatar::member(fact.value.clone(), tokens), id, fact, tokens)
        }
        "Labels" if !detail.fields.labels.is_empty() => labels(&detail.fields.labels, tokens),
        _ => panel::value(id, fact.value.clone(), tokens).into_any_element(),
    }
}

/// A value led by its glyph or avatar. The mark keeps its size; the text is
/// cut with a tooltip when the panel is narrow.
fn marked(mark: impl IntoElement, id: SharedString, fact: &Fact, tokens: &Tokens) -> AnyElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(gpui_kit::div().flex_shrink_0().child(mark))
        .child(panel::value(id, fact.value.clone(), tokens))
        .into_any_element()
}

/// The labels, wrapping onto more lines. A label longer than the panel is
/// cut with an ellipsis, and its whole name is in the tooltip.
fn labels(labels: &[subspace_api_client::model::Label], tokens: &Tokens) -> AnyElement {
    h_flex()
        .id("issue-property-labels")
        .test_support()
        .flex_1()
        .min_w_0()
        .flex_wrap()
        .gap_x_2()
        .gap_y_1()
        .children(labels.iter().map(|label| {
            let colour = crate::view::lanes::label_rgb(&label.color)
                .map_or(tokens.muted, |rgb| tokens.glyph(&Glyph::rgb("●", rgb)));
            h_flex()
                .id(SharedString::from(format!("issue-label:{}", label.id)))
                .test_support()
                .min_w_0()
                .max_w_full()
                .overflow_hidden()
                .gap_1()
                .child(
                    gpui_kit::div()
                        .flex_shrink_0()
                        .size_2()
                        .rounded_full()
                        .bg(colour),
                )
                .child(
                    gpui_kit::div()
                        .min_w_0()
                        .overflow_hidden()
                        .child(truncated::text(
                            SharedString::from(format!("issue-label-name:{}", label.id)),
                            label.name.clone(),
                        )),
                )
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests;
