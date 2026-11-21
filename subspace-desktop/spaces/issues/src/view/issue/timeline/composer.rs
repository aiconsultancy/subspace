//! The comment composer at the end of the discussion. At rest it is one
//! compact line, *Write a comment…* with Comment; a click on either, or
//! Enter or Space on either, opens the shared editor (Write, Preview,
//! Attach files…, its key hints and Leave editor) and focuses it. A draft in
//! progress, or a reason the issue takes no comments, shows the editor as
//! before. Once opened it stays open while the issue's discussion shows.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, Styled as _, TestSupportExt as _, WeakEntity,
    Window, div,
};
use subspace_api_client::hub::IssueSnapshot;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{Intent, action_button, row_button};
use subspace_framework::lucide::IconName;

use crate::Issues;

const PLACEHOLDER: &str = "Write a comment…";

pub(super) fn render(
    issues: &Issues,
    issue: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let editor = crate::authoring::view::root(issues, tokens, cx);
    let Some(id) = issue.detail().map(|detail| detail.fields.id.clone()) else {
        return editor;
    };
    CommentEntry {
        rests: rests(issues, &id),
        key: format!("issue-comment-opened:{id}").into(),
        editor,
        owner: issues.weak.clone(),
    }
    .into_any_element()
}

/// Whether the composer may rest as one line: no draft to keep in view and
/// nothing to explain.
fn rests(issues: &Issues, issue: &str) -> bool {
    issues.hub.as_ref().is_some_and(|hub| {
        hub.comment_draft(issue, None).is_none() && hub.comment_reason(issue, None).is_none()
    })
}

/// The editor, or the compact line until the person opens it.
#[derive(IntoElement)]
struct CommentEntry {
    editor: AnyElement,
    rests: bool,
    /// Whether this issue's composer was opened, kept while it shows.
    key: SharedString,
    owner: WeakEntity<Issues>,
}

impl RenderOnce for CommentEntry {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let opened = window.use_keyed_state(self.key, cx, |_, _| false);
        if !self.rests || *opened.read(cx) {
            return self.editor;
        }
        at_rest(&opened, &self.owner, &Tokens::current(cx))
    }
}

/// One line: a quiet field-like control and Comment. Both open the editor.
fn at_rest(opened: &Entity<bool>, owner: &WeakEntity<Issues>, tokens: &Tokens) -> AnyElement {
    let start = row_button(
        "issue-comment-start",
        "Write a comment",
        div().text_color(tokens.muted).child(PLACEHOLDER),
    )
    .on_click(open(opened.clone(), owner.clone()));
    let comment = action_button(
        "issue-comment-start-submit",
        Intent::Neutral,
        Some(IconName::MessageSquare),
        "Comment",
    )
    .on_click(open(opened.clone(), owner.clone()));
    h_flex()
        .id("issue-comment-composer")
        .test_support()
        .w_full()
        .min_w_0()
        .gap_2()
        .p_1()
        .border_1()
        .border_color(tokens.line)
        .rounded(tokens.control_radius)
        .child(div().flex_1().min_w_0().child(start))
        .child(comment)
        .into_any_element()
}

/// Opens the editor and moves focus into it.
fn open(
    opened: Entity<bool>,
    owner: WeakEntity<Issues>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        opened.update(cx, |opened, cx| {
            *opened = true;
            cx.notify();
        });
        _ = owner.update(cx, |this, cx| this.focus_comment(None, window, cx));
    }
}
