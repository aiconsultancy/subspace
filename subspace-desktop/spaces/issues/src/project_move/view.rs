use super::{Issues, Phase, picker};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::issue_moves::{Unreconciled, UnreconciledReason};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button, icon_button, menu_button},
};
use subspace_framework::lucide::IconName;
const CONTENT_WIDTH_REM: f32 = 48.;

pub(crate) fn render(board: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> AnyElement {
    let Some(state) = &board.project_move else {
        return div().into_any_element();
    };
    v_flex()
        .id("issue-project-move")
        .size_full()
        .track_focus(&board.focus)
        .key_context("IssueDetail")
        .on_action(
            cx.listener(|this, _: &gpui_kit::component::input::Escape, window, cx| {
                if this.menu.is_none() {
                    this.close_project_move(window, cx);
                }
            }),
        )
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(header(state, tokens, cx))
        .child(
            v_flex()
                .flex_1()
                .min_h_0()
                .p_4()
                .max_w(gpui_kit::rems(CONTENT_WIDTH_REM))
                .gap_4()
                .child(div().child("Destination project"))
                .child(project_picker(board, cx))
                .child(body(board, tokens, cx)),
        )
        .children(board.menu_anchors.view())
        .into_any_element()
}

fn body(board: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let Some(state) = &board.project_move else {
        return div();
    };
    match &state.phase {
        Phase::Choosing => div().child("Choose a destination to preview the move."),
        Phase::Loading => div().child("Loading move preview…"),
        Phase::Sending => div().child("Completing the move…"),
        Phase::Unconfirmed(_) => v_flex()
            .gap_3()
            .child("The move may have completed. Check the issue before trying again.")
            .child(
                action_button("issue-move-check", Intent::Neutral, None, "Check issue").on_click(
                    cx.listener(|this, _, window, cx| this.check_project_move(window, cx)),
                ),
            ),
        Phase::Moved(committed) => moved(committed, tokens, cx),
        Phase::Failed(error) => v_flex().gap_3().child(error.to_string()).child(
            action_button(
                "issue-move-refresh",
                Intent::Neutral,
                None,
                "Refresh preview",
            )
            .on_click(cx.listener(|this, _, _, cx| this.load_move_preview(cx))),
        ),
        Phase::Preview(preview) => super::preview::render(preview, tokens, cx),
    }
}

/// A move that committed with sub-issues it could not confirm in the
/// destination: each is listed with what reading it back found, so the
/// person checks them before relying on the move.
fn moved(committed: &super::Committed, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let (identifier, unreconciled) = (&committed.identifier, &committed.unreconciled);
    // The root opens by its stable ID, wherever it is now.
    let open = committed.id.clone();
    // The list takes what room there is and scrolls, however long, so Open
    // stays in view below it.
    v_flex()
        .gap_3()
        .flex_1()
        .min_h_0()
        .child(format!(
            "Moved to {identifier}. These sub-issues could not be confirmed there. Check each one before relying on the move."
        ))
        .child(
            v_flex()
                .id("issue-move-unreconciled")
                .test_support()
                .gap_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(unreconciled.iter().map(|item| unreconciled_row(item, tokens))),
        )
        .child(
            h_flex().justify_end().child(
                action_button(
                    "issue-move-open",
                    Intent::Primary,
                    Some(IconName::ChevronRight),
                    format!("Open {identifier}"),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.finish_project_move(open.clone(), window, cx)
                })),
            ),
        )
}

fn unreconciled_row(item: &Unreconciled, tokens: &Tokens) -> Div {
    h_flex()
        .gap_4()
        .py_1()
        .child(div().w_40().child(item.old_identifier.clone()))
        .child(
            div()
                .text_color(tokens.muted)
                .child(unreconciled_reason(item.reason)),
        )
}

/// What reading a sub-issue back found, in words.
fn unreconciled_reason(reason: UnreconciledReason) -> String {
    match reason {
        UnreconciledReason::Elsewhere => "Moved again since".into(),
        UnreconciledReason::Unread(error) => format!("Could not be read back: {error}"),
        UnreconciledReason::NotChecked => "Not read back in time".into(),
        _ => "Not confirmed in the destination yet".into(),
    }
}

fn header(state: &super::ProjectMove, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    h_flex()
        .gap_2()
        .px(tokens.pad_x)
        .py_2()
        .border_b_1()
        .border_color(tokens.hair)
        .child(
            action_button(
                "issue-move-cancel",
                Intent::Quiet,
                Some(IconName::ArrowLeft),
                "Cancel",
            )
            .disabled(matches!(state.phase, Phase::Sending))
            .on_click(cx.listener(|this, _, window, cx| this.close_project_move(window, cx))),
        )
        .child(
            div()
                .text_size(tokens.heading_font)
                .child("Move issue to another project"),
        )
}
fn project_picker(board: &Issues, cx: &mut Context<Issues>) -> AnyElement {
    let Some(state) = &board.project_move else {
        return div().into_any_element();
    };
    let name = state
        .destination
        .as_ref()
        .and_then(|id| board.projects.iter().find(|p| &p.id == id))
        .map_or_else(
            || "Choose a project…".to_owned(),
            |p| format!("{} · {}", p.key, p.name),
        );
    let picker = menu_button(
        picker::ID,
        IconName::Folder,
        name,
        state.destination.is_some(),
    )
    .accessibility_label("Destination project")
    .disabled(state.busy() || state.sent())
    .on_click(cx.listener(|this, _, window, cx| picker::open(this, window, cx)));
    board
        .menu_anchors
        .trigger(picker::ID, picker, cx)
        .into_any_element()
}
pub(crate) fn inline(
    board: &Issues,
    label: &str,
    _tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let namespace = if crate::pickers::teams() {
        "Team"
    } else {
        "Project"
    };
    if label != namespace {
        return None;
    }
    let state = board.project_move.as_ref()?;
    if state.confirming {
        return None;
    }
    let notice = match &state.phase {
        Phase::Loading => "Loading move preview…".to_owned(),
        Phase::Failed(error) => error.to_string(),
        _ => String::new(),
    };
    Some(
        v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .track_focus(&state.focus)
            .key_context("IssueEditor")
            .on_action(cx.listener(
                |this, _: &crate::editing::view::CancelIssueEditor, window, cx| {
                    if this.menu.is_none() {
                        this.close_project_move(window, cx);
                    }
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::editing::view::SubmitIssueEditor, window, cx| {
                    this.review_project_move(window, cx)
                },
            ))
            .child(inline_controls(board, cx))
            .children((!notice.is_empty()).then_some(notice))
            .into_any_element(),
    )
}

fn inline_controls(board: &Issues, cx: &mut Context<Issues>) -> Div {
    let ready = board
        .project_move
        .as_ref()
        .is_some_and(|s| matches!(s.phase, Phase::Preview(_)));
    h_flex()
        .items_start()
        .gap_1()
        .child(div().flex_1().min_w_0().child(project_picker(board, cx)))
        .child(
            icon_button(
                "issue-move-review",
                Intent::Primary,
                IconName::Check,
                "Review project move…",
            )
            .compact()
            .disabled(!ready)
            .on_click(cx.listener(|this, _, window, cx| this.review_project_move(window, cx))),
        )
        .child(
            icon_button(
                "issue-move-inline-cancel",
                Intent::Quiet,
                IconName::X,
                "Cancel project move",
            )
            .compact()
            .on_click(cx.listener(|this, _, window, cx| this.close_project_move(window, cx))),
        )
}

#[cfg(test)]
mod tests {
    use super::unreconciled_reason;
    use subspace_api_client::issue_moves::UnreconciledReason;

    /// Only a sub-issue seen in a third team, changed since the preview, is
    /// said to have moved again; a stale or unchanged read says nothing of
    /// the kind.
    #[test]
    fn each_unconfirmed_sub_issue_says_only_what_its_read_showed() {
        assert_eq!(
            unreconciled_reason(UnreconciledReason::Elsewhere),
            "Moved again since"
        );
        assert_eq!(
            unreconciled_reason(UnreconciledReason::NotConfirmed),
            "Not confirmed in the destination yet"
        );
        assert_eq!(
            unreconciled_reason(UnreconciledReason::NotChecked),
            "Not read back in time"
        );
        assert_eq!(
            unreconciled_reason(UnreconciledReason::Unread(
                subspace_api_client::Error::Network
            )),
            "Could not be read back: The service could not be reached. Try again."
        );
    }
}
