use super::shell::{self, Head};
use super::{Issues, Tokens, body_element, issue};
use crate::keys;
use crate::scope::{self, Body};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, KeyContext, ParentElement as _,
    Render, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::workspace::text_edge;
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, list};
impl Render for Issues {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let identity = subspace_framework::dashboard::Dashboard::find_documents(self, cx).0;
        let content = content(self, window, cx);
        subspace_framework::dashboard::find_in_page::identified(identity, true, content, cx)
    }
}
fn content(issues: &mut Issues, window: &mut Window, cx: &mut Context<Issues>) -> AnyElement {
    issues.width = f32::from(window.viewport_size().width);
    let tokens = Tokens::current(cx);
    issues.text_edge = text_edge(&tokens);
    if let Some(page) = special_page(issues, &tokens, cx) {
        return page;
    }

    let body = scope::body(&issues.access, &issues.listing);
    let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
    context.add(DASHBOARD_CONTEXT);
    context.add(crate::ID);
    if issues.issue_view.snapshot.is_some() {
        context.add(keys::DETAIL_CONTEXT);
    }
    let root = keys::handlers(
        v_flex()
            .size_full()
            .relative()
            .key_context(context)
            .track_focus(&issues.focus),
        cx,
    )
    .bg(tokens.bg)
    .text_color(tokens.fg);
    if let Some(open) = &issues.issue_view.snapshot {
        return root
            .child(issue::render(issues, open, &tokens, window, cx))
            .children(crate::media::view::viewer(issues, cx))
            .children(issues.menu_anchors.view())
            .into_any_element();
    }
    root.child(list_workspace(issues, &body, &tokens, window, cx))
        .children(issues.menu_anchors.view())
        .into_any_element()
}

/// The list's workspace: the issue list, the activity feed, an initiative
/// with no projects, or the screen that stands in for the list.
fn list_workspace(
    issues: &Issues,
    body: &Body,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> AnyElement {
    if crate::initiative::no_initiative_projects(issues) && issues.is_ready() {
        let head = if issues.activity.visible {
            Head::Activity
        } else {
            Head::List
        };
        let state = no_projects_state(issues, tokens, cx);
        return shell::list(issues, state, head, cx);
    }
    if issues.activity.visible && issues.is_ready() {
        let feed = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(list::render(issues, window, cx))
            .into_any_element();
        return shell::list(issues, feed, Head::Activity, cx);
    }
    let head = if body.controls() {
        Head::List
    } else {
        Head::Bare
    };
    let content = body_element(issues, body, tokens, window, cx);
    shell::list(issues, content, head, cx)
}

fn special_page(board: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> Option<AnyElement> {
    if board
        .project_move
        .as_ref()
        .is_some_and(|state| state.confirming)
    {
        return Some(crate::project_move::view::render(board, tokens, cx));
    }
    board.full_editor_active().then(|| {
        div()
            .id("issue-editor-root")
            .size_full()
            .test_support()
            .child(crate::editing::view::render(board, tokens, cx))
            .into_any_element()
    })
}

fn no_projects_state(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> AnyElement {
    if let crate::listing::Status::Failed(failure) = &issues.listing.status {
        return super::states::render(issues, &scope::Screen::Failed(failure.clone()), tokens, cx);
    }
    div()
        .px(text_edge(tokens))
        .py_3()
        .text_color(tokens.muted)
        .child("No active projects in this initiative")
        .into_any_element()
}
