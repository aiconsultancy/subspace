//! Compact role badges at the end of an issue row.
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, Size, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Div, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px, rems,
};
use subspace_api_client::model::{IssueListItem, Person};
use subspace_framework::dashboard::{Tokens, avatar};
use subspace_framework::lucide::IconName;

const PERSON_SLOT_PX: f32 = 20.;
const OWNER_WIDTH_REMS: f32 = 1.5;
const OWNER_ROLE_MARK_PX: f32 = 10.;

pub(super) fn people(item: &IssueListItem, tokens: &Tokens) -> Div {
    let fields = &item.fields;
    h_flex()
        .flex_shrink_0()
        .gap_1()
        .when(subspace_api_client::capabilities().issue_owners, |row| {
            let person = fields.owner.as_ref();
            row.child(person_badge(
                ("Owner", "No owner"),
                &fields.id,
                person,
                owner_badge(person, tokens),
            ))
        })
        .child(person_badge(
            ("Assignee", "Unassigned"),
            &fields.id,
            fields.assignee.as_ref(),
            initials(fields.assignee.as_ref(), tokens),
        ))
}

fn owner_badge(person: Option<&Person>, tokens: &Tokens) -> AnyElement {
    let badge = h_flex()
        .relative()
        .justify_center()
        .w(rems(OWNER_WIDTH_REMS))
        .h(px(PERSON_SLOT_PX))
        .rounded(tokens.control_radius)
        .border_1()
        .border_color(tokens.line);
    match person {
        Some(person) => badge
            .pr_1()
            .child(avatar::member(person.display_name.clone(), tokens))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .bottom_0()
                    .flex()
                    .rounded(tokens.control_radius)
                    .bg(tokens.surface)
                    .child(
                        Icon::from(IconName::Shield)
                            .with_size(Size::Size(px(OWNER_ROLE_MARK_PX)))
                            .text_color(tokens.muted),
                    ),
            ),
        None => badge.child(
            Icon::from(IconName::Minus)
                .xsmall()
                .text_color(tokens.muted),
        ),
    }
    .into_any_element()
}

fn initials(person: Option<&Person>, tokens: &Tokens) -> AnyElement {
    match person {
        Some(person) => h_flex()
            .size(px(PERSON_SLOT_PX))
            .justify_center()
            .child(avatar::member(person.display_name.clone(), tokens))
            .into_any_element(),
        None => div()
            .size(px(PERSON_SLOT_PX))
            .rounded(tokens.control_radius)
            .border_1()
            .border_color(tokens.hair)
            .into_any_element(),
    }
}

fn person_badge(
    (role, empty): (&str, &str),
    id: &str,
    person: Option<&Person>,
    badge: AnyElement,
) -> AnyElement {
    let value = person.map_or_else(|| empty.to_owned(), Person::fact_label);
    let label: SharedString = format!("{role}: {value}").into();
    div()
        .id(SharedString::from(format!("issues-person:{role}:{id}")))
        .test_support()
        .role(Role::Image)
        .aria_label(label.clone())
        .child(badge)
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        .into_any_element()
}
