//! The retained proposal fields, with separate groups for details and evidence.

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Entity, IntoElement as _, ParentElement as _, Styled as _, div,
};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::anchored;
use subspace_framework::dashboard::fields::{self, InputRole};
use subspace_framework::lucide::IconName;

use super::Mode;
use super::draft::Draft;
use super::evidence::Evidence;
use super::picker::Picker;
use crate::Decisions;

pub(super) fn render(draft: &Draft, sending: bool, cx: &mut Context<Decisions>) -> AnyElement {
    v_flex()
        .gap_3()
        .child(project_and_date(draft, sending))
        .child(line(&draft.title, "Title", sending))
        .child(paragraph(&draft.decision, "Decision", sending))
        .child(paragraph(&draft.context, "Context", sending))
        .child(line(
            &draft.issues,
            "Linked issues",
            sending || draft.hidden_issues,
        ))
        .children(draft.hidden_issues.then(|| {
            div().child(
            "Only some issue links are available. Save other changes to preserve every existing link.",
        )
        }))
        .children(
            draft
                .predecessor_title
                .as_ref()
                .map(|title| div().child(format!("Replacing {title}"))),
        )
        .child(disclosure(
            FormSection::Details,
            draft.details_open,
            sending,
            cx,
        ))
        .children(draft.details_open.then(|| details(draft, sending)))
        .children(
            draft
                .evidence
                .as_ref()
                .map(|_| disclosure(FormSection::Evidence, draft.evidence_open, sending, cx)),
        )
        .children(
            draft
                .evidence
                .as_ref()
                .filter(|_| draft.evidence_open)
                .map(|evidence| evidence_form(evidence, sending)),
        )
        .into_any_element()
}

fn project_and_date(draft: &Draft, sending: bool) -> gpui_kit::Div {
    let project = match &draft.project {
        Some(picker) => select(picker, "decision-project", "Project", sending),
        None => fields::form_field(
            "Project",
            div().child(
                draft
                    .fixed_project
                    .as_ref()
                    .map(|(_, key)| key.clone())
                    .unwrap_or_default(),
            ),
        ),
    };
    h_flex()
        .gap_3()
        .items_end()
        .child(project.flex_1())
        .child(line(&draft.date, "Decided on", sending).w_40())
}

#[derive(Clone, Copy)]
enum FormSection {
    Details,
    Evidence,
}

fn disclosure(
    section: FormSection,
    open: bool,
    sending: bool,
    cx: &mut Context<Decisions>,
) -> gpui_kit::Div {
    let icon = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let (id, title) = match section {
        FormSection::Details => ("decision-more-fields", "Additional fields"),
        FormSection::Evidence => ("decision-maker-and-evidence", "Maker and evidence"),
    };
    h_flex().child(
        action_button(id, Intent::Neutral, Some(icon), title)
            .disabled(sending)
            .on_click(cx.listener(move |board, _, _, cx| {
                if let Some(editor) = &mut board.author
                    && let Mode::Draft { draft, .. } = &mut editor.mode
                {
                    let open = match section {
                        FormSection::Details => &mut draft.details_open,
                        FormSection::Evidence => &mut draft.evidence_open,
                    };
                    *open = !*open;
                    cx.notify();
                }
            })),
    )
}

fn details(draft: &Draft, sending: bool) -> AnyElement {
    v_flex()
        .gap_3()
        .child(paragraph(&draft.why, "Why", sending))
        .child(paragraph(&draft.implications, "Implications", sending))
        .child(paragraph(&draft.alternatives, "Alternatives", sending))
        .child(line(&draft.tags, "Tags", sending))
        .children(
            draft
                .predecessor_title
                .is_none()
                .then(|| line(&draft.supersedes, "Supersedes", sending)),
        )
        .into_any_element()
}

fn evidence_form(evidence: &Evidence, sending: bool) -> AnyElement {
    v_flex()
        .gap_3()
        .child(
            h_flex()
                .gap_3()
                .child(select(&evidence.maker, "decision-maker", "Made by", sending).flex_1())
                .child(
                    select(
                        &evidence.kind,
                        "decision-evidence-kind",
                        "Evidence type",
                        sending,
                    )
                    .flex_1(),
                ),
        )
        .child(line(&evidence.reference, "Evidence reference", sending))
        .child(paragraph(&evidence.excerpt, "Evidence excerpt", sending))
        .into_any_element()
}

fn line(field: &Entity<InputState>, label: &'static str, sending: bool) -> gpui_kit::Div {
    fields::form_field(
        label,
        fields::input(field, InputRole::Configuration, label).disabled(sending),
    )
}

fn paragraph(field: &Entity<TextareaState>, label: &'static str, sending: bool) -> gpui_kit::Div {
    fields::form_field(label, fields::textarea(field, label).disabled(sending))
}

fn select(picker: &Picker, id: &'static str, label: &'static str, sending: bool) -> gpui_kit::Div {
    fields::form_field(
        label,
        anchored::select(&picker.select)
            .id(id)
            .accessibility_label(label)
            .small()
            .disabled(sending),
    )
}
