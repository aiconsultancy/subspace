use super::{CancelIssueEditor, EDITOR_CONTEXT, SubmitIssueEditor, notices, parent, picker};
use crate::{
    Issues,
    editing::{EditTarget, Phase},
};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, icon_button},
    fields::{self, InputRole},
};
use subspace_framework::lucide::IconName;

pub(crate) fn row(
    board: &Issues,
    label: &str,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let editor = board.editor.as_ref()?;
    let target = editor.inline.filter(|target| target.label() == label)?;
    let input = control(board, target, cx)?;
    Some(
        v_flex()
            .id("issue-inline-editor")
            .test_support()
            .flex_1()
            .min_w_0()
            .gap_1()
            .key_context(EDITOR_CONTEXT)
            .on_action(cx.listener(|this, _: &CancelIssueEditor, window, cx| {
                if this.menu.is_none() {
                    this.cancel_inline_field(window, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &gpui_kit::component::input::Escape, window, cx| {
                    if this.menu.is_none() {
                        this.cancel_inline_field(window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &SubmitIssueEditor, window, cx| {
                if this.menu.is_none() {
                    this.save_issue_editor(window, cx);
                }
            }))
            .child(controls(board, input, cx))
            .child(notices::notice(editor, tokens, cx))
            .into_any_element(),
    )
}

// qual:allow(coupling, oi) reason: "Cancelling coordinates the retained field draft and focus restoration."
impl Issues {
    fn cancel_inline_field(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if self
            .editor
            .as_ref()
            .is_some_and(|e| e.busy() || matches!(e.phase, Phase::Unconfirmed))
        {
            return;
        }
        if let Some(editor) = &mut self.editor
            && matches!(editor.phase, Phase::ConfirmingChildren(_))
        {
            editor.phase = Phase::Editing;
            cx.notify();
            return;
        }
        self.close_issue_editor(window, cx);
        self.menu = None;
    }
}

fn control(board: &Issues, target: EditTarget, cx: &mut Context<Issues>) -> Option<AnyElement> {
    let editor = board.editor.as_ref()?;
    Some(match target {
        EditTarget::Property(field) => picker::control(board, editor, field, editor.frozen(), cx),
        EditTarget::Summary => {
            super::summary::input(editor, InputRole::Compact, editor.frozen(), cx)
                .into_any_element()
        }
        EditTarget::DueDate => fields::input(
            &editor.due,
            InputRole::Compact,
            "Due date in YYYY-MM-DD format",
        )
        .id("issue-inline-due")
        .disabled(editor.frozen())
        .into_any_element(),
        EditTarget::Parent => parent::control(
            board,
            editor,
            (InputRole::Compact, "issue-inline-parent"),
            editor.frozen(),
            cx,
        ),
        _ => return None,
    })
}
fn controls(board: &Issues, input: AnyElement, cx: &mut Context<Issues>) -> gpui_kit::Div {
    let editor = board.editor.as_ref();
    let frozen =
        editor.is_none_or(|e| e.frozen() || !e.dirty(cx) || e.changes(cx).validate(false).is_err());
    let busy = editor.is_none_or(|e| e.busy() || matches!(e.phase, Phase::Unconfirmed));
    h_flex()
        .gap_1()
        .items_start()
        .child(div().flex_1().min_w_0().child(input))
        .child(
            icon_button(
                "issue-inline-save",
                Intent::Primary,
                IconName::Check,
                "Save field",
            )
            .compact()
            .disabled(frozen)
            .on_click(cx.listener(|this, _, window, cx| this.save_issue_editor(window, cx))),
        )
        .child(
            icon_button(
                "issue-inline-cancel",
                Intent::Quiet,
                IconName::X,
                "Cancel field edit",
            )
            .compact()
            .disabled(busy)
            .on_click(cx.listener(|this, _, window, cx| this.cancel_inline_field(window, cx))),
        )
}
