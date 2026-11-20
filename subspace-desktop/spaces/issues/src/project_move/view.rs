use super::{Issues, Phase, picker};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
    prelude::FluentBuilder as _,
};
use subspace_api_client::issue_moves::MovePreview;
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
        Phase::Failed(error) => v_flex().gap_3().child(error.to_string()).child(
            action_button(
                "issue-move-refresh",
                Intent::Neutral,
                None,
                "Refresh preview",
            )
            .on_click(cx.listener(|this, _, _, cx| this.load_move_preview(cx))),
        ),
        Phase::Preview(preview) => v_flex()
            .gap_3()
            .flex_1()
            .min_h_0()
            .child(format!(
                "Move {} {} to {} · {}?",
                preview.issues.len(),
                if preview.issues.len() == 1 {
                    "issue"
                } else {
                    "issues"
                },
                preview.destination.key,
                preview.destination.name
            ))
            .child(numbering(preview))
            .child(consequences(preview))
            .when(refuses_archived(preview), |col| {
                col.child(div().id("issue-move-archived").test_support().child(
                    subspace_api_client::issue_writes::IssueSaveError::ArchivedSubIssue.to_string(),
                ))
            })
            .child(
                v_flex()
                    .id("issue-move-identifiers")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .children(preview.issues.iter().map(|item| {
                        h_flex()
                            .gap_4()
                            .py_1()
                            .child(div().w_40().child(item.old_identifier.clone()))
                            .child(
                                div().w_40().child(
                                    item.new_identifier
                                        .clone()
                                        .unwrap_or_else(|| "New number on move".into()),
                                ),
                            )
                            .child(div().text_color(tokens.muted).child(format!(
                                "{}{}",
                                item.state_name,
                                if item.archived { " · Archived" } else { "" }
                            )))
                    })),
            )
            .child(
                h_flex().justify_end().child(
                    action_button(
                        "issue-move-confirm",
                        Intent::Primary,
                        Some(IconName::FolderInput),
                        "Move",
                    )
                    .disabled(refuses_archived(preview))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.confirm_project_move(window, cx)),
                    ),
                ),
            ),
    }
}

/// How the moved issues are numbered: as previewed, or, where the service
/// numbers them on confirmation, in the destination when the move completes.
fn numbering(preview: &MovePreview) -> String {
    if preview
        .issues
        .iter()
        .any(|item| item.new_identifier.is_none())
    {
        format!(
            "Each issue gets a new {} number when the move completes. The exact changes are shown below.",
            preview.destination.key
        )
    } else {
        "Each issue will receive a new project key and number. The exact changes are shown below."
            .into()
    }
}

/// NETT refuses to move an archived sub-issue with its parent; Subspace API
/// moves it and keeps it archived.
fn refuses_archived(preview: &MovePreview) -> bool {
    subspace_api_client::backend() == subspace_api_client::Backend::Nett
        && preview.moves_archived_sub_issue()
}

/// What the move clears and what happens to sub-issues. Subspace API's
/// preview keeps its fixed wording; NETT's follows the rows.
fn consequences(preview: &MovePreview) -> String {
    if subspace_api_client::backend() != subspace_api_client::Backend::Nett {
        return "Cycle, milestone and estimate values will be cleared. A separately moved child will leave its parent. History, files and old issue links will be kept.".into();
    }
    let issues = &preview.issues;
    let cleared: Vec<&str> = [
        (issues.iter().any(|i| i.cycle_cleared), "cycle"),
        (issues.iter().any(|i| i.milestone_cleared), "milestone"),
        (issues.iter().any(|i| i.estimate_cleared), "estimate"),
    ]
    .into_iter()
    .filter_map(|(cleared, name)| cleared.then_some(name))
    .collect();
    let mut text = match cleared.as_slice() {
        [] => String::new(),
        [one] => format!("{} values will be cleared. ", capitalised(one)),
        [first, second] => format!(
            "{} and {second} values will be cleared. ",
            capitalised(first)
        ),
        _ => "Cycle, milestone and estimate values will be cleared. ".into(),
    };
    text.push_str(if issues.iter().any(|i| i.parent_detached) {
        "A separately moved child will leave its parent. "
    } else if issues.len() > 1 {
        "Sub-issues move with their parent. "
    } else {
        ""
    });
    text.push_str("History, files and old issue links will be kept.");
    text
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
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
    .disabled(state.busy() || matches!(state.phase, Phase::Unconfirmed(_)))
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
