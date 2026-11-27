//! New issue and Edit issue use the same focused form.
mod actions_view;
mod description;
mod inline;
mod notices;
mod parent;
mod picker;
mod summary;
pub(crate) use inline::row as inline_row;
pub(super) use parent::{ParentSearch, open as open_parent, searches_trackers};
pub(crate) use picker::open as open_property;

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, Div, InteractiveElement as _, KeyBinding, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, actions, div, rems,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    fields::{self, InputRole},
};
use subspace_framework::lucide::IconName;

pub(crate) use actions_view::{edit_button, new_button};

use super::{Field, IssueEditor, Phase};
use crate::{Issues, view::reveal_focus};

actions!(issue_editor, [CancelIssueEditor, SubmitIssueEditor]);
const EDITOR_CONTEXT: &str = "IssueEditor";
const FORM_WIDTH_REMS: f32 = 64.;

pub(crate) fn bind(cx: &mut App) {
    let context = Some(EDITOR_CONTEXT);
    let submit = if cfg!(target_os = "macos") {
        "cmd-enter"
    } else {
        "ctrl-enter"
    };
    cx.bind_keys([
        KeyBinding::new("escape", CancelIssueEditor, context),
        KeyBinding::new(submit, SubmitIssueEditor, context),
    ]);
}

pub(crate) fn render(board: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let Some(editor) = &board.editor else {
        return div();
    };
    let name = if editor.editing_existing() {
        "Edit issue"
    } else {
        "New issue"
    };
    v_flex()
        .size_full()
        .relative()
        .track_focus(&board.focus)
        .key_context(EDITOR_CONTEXT)
        .on_action(cx.listener(|board, _: &CancelIssueEditor, window, cx| {
            if board.menu.is_none() {
                board.cancel_issue_editor(window, cx);
            }
        }))
        .on_action(cx.listener(
            |board, _: &gpui_kit::component::input::Escape, window, cx| {
                if board.menu.is_none() {
                    board.cancel_issue_editor(window, cx);
                }
            },
        ))
        .on_action(cx.listener(|board, _: &SubmitIssueEditor, window, cx| {
            if board.menu.is_none() {
                board.save_issue_editor(window, cx);
            }
        }))
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(header(board, editor, name, tokens, cx))
        .child(
            v_flex()
                .id("issue-editor-scroll")
                .track_scroll(&editor.scroll)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(form(board, editor, tokens, cx)),
        )
        .children(board.menu_anchors.view())
}

fn header(
    board: &Issues,
    editor: &IssueEditor,
    name: &str,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    h_flex()
        .gap_2()
        .px(tokens.pad_x)
        .py_2()
        .border_b_1()
        .border_color(tokens.hair)
        .child(
            action_button(
                "issue-editor-cancel",
                Intent::Quiet,
                Some(IconName::ArrowLeft),
                "Back",
            )
            .disabled(editor.busy())
            .on_click(cx.listener(|board, _, window, cx| board.cancel_issue_editor(window, cx))),
        )
        .child(
            div()
                .text_size(tokens.heading_font)
                .text_color(tokens.fg_strong)
                .child(name.to_owned()),
        )
        .when_some(editor.basis.as_ref(), |row, issue| {
            row.child(
                div()
                    .text_color(tokens.muted)
                    .child(issue.fields.identifier.clone()),
            )
        })
        .child(div().flex_1())
        .child(submit(board, editor, cx))
}

fn submit(
    board: &Issues,
    editor: &IssueEditor,
    cx: &mut Context<Issues>,
) -> impl gpui_kit::IntoElement {
    let label = match &editor.phase {
        Phase::Sending => "Saving…",
        Phase::Checked => "Create again",
        _ if editor.editing_existing() => "Save changes",
        _ => "Create issue",
    };
    let held = editor.busy()
        || editor.discard
        || matches!(
            editor.phase,
            Phase::Conflict(_)
                | Phase::ConfirmingChildren(_)
                | Phase::Unconfirmed
                | Phase::CheckFailed
                | Phase::SeveralMatches
        )
        || (editor.editing_existing() && !board.can_edit_issue());
    action_button(
        "issue-editor-save",
        Intent::Primary,
        Some(IconName::Check),
        label,
    )
    .disabled(
        held || editor
            .changes(cx)
            .validate(!editor.editing_existing())
            .is_err(),
    )
    .on_click(cx.listener(|board, _, window, cx| {
        board.save_issue_editor(window, cx);
    }))
}

fn form(board: &Issues, editor: &IssueEditor, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let locked = editor.controls_locked();
    v_flex()
        .mx_auto()
        .w_full()
        .max_w(rems(FORM_WIDTH_REMS))
        .p_6()
        .gap_4()
        .child(scope(board, tokens))
        .child(notices::notice(editor, tokens, cx))
        .child(reveal_focus::scope(
            "title",
            &editor.scroll,
            fields::form_field(
                "Title",
                fields::input(&editor.title, InputRole::Configuration, "Issue title")
                    .id("issue-edit-title")
                    .disabled(locked),
            ),
        ))
        .when(
            subspace_api_client::capabilities().issue_summaries,
            |form| {
                form.child(reveal_focus::scope(
                    "summary",
                    &editor.scroll,
                    fields::form_field(
                        "Summary",
                        summary::input(editor, InputRole::Configuration, locked, cx),
                    ),
                ))
            },
        )
        .child(description::render(editor, locked, tokens, cx))
        .child(properties(board, editor, locked, tokens, cx))
}

fn scope(board: &Issues, tokens: &Tokens) -> Div {
    let scope = crate::scope::scope_line(board.profile_name(), &board.access);
    div().text_color(tokens.muted).text_sm().child(scope)
}

fn properties(
    board: &Issues,
    editor: &IssueEditor,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    v_flex()
        .gap_3()
        .pt_2()
        .border_t_1()
        .border_color(tokens.hair)
        .child(div().text_size(tokens.heading_font).child("Properties"))
        .child(
            h_flex()
                .gap_3()
                .flex_wrap()
                .children(Field::ALL.into_iter().map(|field| {
                    reveal_focus::scope(
                        field.facts().0,
                        &editor.scroll,
                        picker::render(board, editor, field, locked, cx),
                    )
                })),
        )
        .child(additional_properties(board, editor, locked, cx))
}

fn additional_properties(
    board: &Issues,
    editor: &IssueEditor,
    locked: bool,
    cx: &mut Context<Issues>,
) -> Div {
    h_flex()
        .items_start()
        .flex_wrap()
        .gap_3()
        .child(
            div().min_w_48().flex_1().child(reveal_focus::scope(
                "due-date",
                &editor.scroll,
                fields::form_field(
                    "Due date",
                    fields::input(
                        &editor.due,
                        InputRole::Configuration,
                        "Due date in YYYY-MM-DD format",
                    )
                    .id("issue-edit-due")
                    .disabled(locked),
                ),
            )),
        )
        .child(div().min_w_48().flex_1().child(reveal_focus::scope(
            "parent",
            &editor.scroll,
            fields::form_field(
                "Parent",
                parent::control(
                    board,
                    editor,
                    (InputRole::Configuration, "issue-edit-parent"),
                    locked,
                    cx,
                ),
            ),
        )))
}
