use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_api_client::model::{IssueDetail, IssueProvenance};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_framework::dashboard::{Tokens, badge::badge, heading_font};
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::model::{ProjectScope, StateKind};
use crate::view::lanes;

const ISSUE_TITLE_SCALE: f32 = 1.6;

pub(super) fn path(
    issues: &Issues,
    detail: Option<&IssueDetail>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let row = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .child(
            action_button(
                "issue-back",
                Intent::Quiet,
                Some(IconName::ArrowLeft),
                issues.back.label(),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.go_back(window, cx);
            })),
        )
        .when_some(detail, |row, detail| {
            row.child(path_segments(issues, detail, tokens, cx))
        })
        .child(div().flex_1())
        .when(issues.can_edit_issue(), |row| {
            row.child(crate::editing::view::edit_button(cx))
        })
        .child(super::more::render(issues, cx));
    // The bar spans the window; its contents keep to the page column, so
    // Back lines up with the title below it.
    super::page::band()
        .flex_shrink_0()
        .border_b_1()
        .border_color(tokens.hair)
        .child(super::page::column().py_2().child(row))
}

fn path_segments(
    issues: &Issues,
    detail: &IssueDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let key = &detail.fields.project_key;
    let project = issues
        .projects
        .iter()
        .find(|project| &project.key == key)
        .map_or_else(
            || key.clone(),
            |project| format!("{key} · {}", project.name),
        );
    h_flex()
        .gap_2()
        .min_w_0()
        .text_color(tokens.muted)
        .text_size(tokens.secondary_size())
        .child(
            action_button(
                "issue-project",
                Intent::Quiet,
                Some(IconName::Users),
                project,
            )
            .on_click(cx.listener({
                let key = key.clone();
                move |this, _, window, cx| {
                    if this.inline_navigation_blocked(cx) {
                        return;
                    }
                    this.close_detail(cx);
                    this.set_project(ProjectScope::Project(key.clone()), cx);
                    this.focus_list(window, cx);
                }
            })),
        )
        .when_some(detail.parent.as_ref(), |path, parent| {
            let identifier = parent.identifier.clone();
            path.child(
                action_button(
                    "issue-parent",
                    Intent::Quiet,
                    Some(IconName::ChevronRight),
                    identifier.clone(),
                )
                .accessibility_label(format!("Open parent {identifier}"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus_list(window, cx);
                    this.open_detail(identifier.clone(), cx);
                })),
            )
        })
}

pub(super) fn render(detail: &IssueDetail, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let fields = &detail.fields;
    let tracker = fields.kind == "tracker";
    let (_, tone) = lanes::state_icon(StateKind::parse(&fields.state.kind));
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .child(
                    div()
                        .id("issue-title")
                        .aria_label(fields.title.clone())
                        .test_support()
                        .font_family(heading_font(cx))
                        .text_size(tokens.heading_font * ISSUE_TITLE_SCALE)
                        .font_weight(FontWeight::BOLD)
                        .text_color(tokens.fg_strong)
                        .child(subspace_framework::dashboard::rich::plain_text(
                            "issue-title-text",
                            &fields.title,
                        )),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(badge(
                            fields.state.name.clone(),
                            tokens.surface,
                            tokens.tone(tone),
                        ))
                        .when(tracker, |row| {
                            row.child(badge("Tracker", tokens.surface, tokens.fg))
                        })
                        .when(fields.archived_at.is_some(), |row| {
                            row.child(badge("Archived", tokens.surface, tokens.muted))
                        }),
                ),
        )
        .child(metadata(detail, tokens))
}

fn metadata(detail: &IssueDetail, tokens: &Tokens) -> Div {
    let fields = &detail.fields;
    v_flex()
        .gap_1()
        .text_color(tokens.muted)
        .text_size(tokens.secondary_size())
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .items_start()
                .child(
                    h_flex()
                        .gap_2()
                        .child(copyable_identifier(&fields.identifier))
                        .when_some(detail.created_by.as_ref(), |row, person| {
                            let verb = if fields.kind == "tracker" {
                                "Added by"
                            } else {
                                "Opened by"
                            };
                            row.child(subspace_framework::dashboard::rich::plain_text(
                                "issue-opened-by",
                                &format!("· {verb} {}", person.display_name),
                            ))
                        }),
                )
                .children(super::header_progress::render(detail, tokens)),
        )
        .when_some(detail.fields.provenance.as_ref(), |column, provenance| {
            column.child(subspace_framework::dashboard::rich::plain_text(
                "issue-provenance",
                &provenance_line(provenance),
            ))
        })
}

fn copyable_identifier(value: &str) -> AnyElement {
    let value: SharedString = value.to_owned().into();
    div()
        .id("issue-identifier")
        .aria_label(value.clone())
        .test_support()
        .child(
            subspace_framework::dashboard::rich::plain_text("issue-identifier-text", &value)
                .menu(false),
        )
        .anchored_context_menu(move |menu, _, _| {
            let value = value.clone();
            menu.item(
                PopupMenuItem::new("Copy issue number")
                    .icon(IconName::Copy)
                    .on_click(move |_, _, cx| {
                        subspace_framework::dashboard::links::copy(value.to_string(), cx)
                    }),
            )
        })
        .into_any_element()
}

fn provenance_line(source: &IssueProvenance) -> SharedString {
    let system = if source.source_system == "linear" {
        "Linear"
    } else {
        &source.source_system
    };
    let mut text = format!("Imported from {system} · {}", source.original_identifier);
    if let Some(author) = &source.original_creator_name {
        text.push_str(" · originally by ");
        text.push_str(author);
    }
    text.into()
}
