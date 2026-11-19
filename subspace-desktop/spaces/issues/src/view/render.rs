use super::{Issues, Tokens, activity_controls, body_element, controls, header, issue, scope};
use crate::keys;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, KeyContext, ParentElement as _,
    Render, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::{DASHBOARD_CONTEXT, ListDashboard as _, list};
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
    if crate::initiative::no_initiative_projects(issues) && issues.is_ready() {
        return root
            .children(no_projects(issues, &tokens, cx))
            .children(issues.menu_anchors.view())
            .into_any_element();
    }
    if issues.activity.visible && issues.is_ready() {
        return root
            .child(header(issues, &tokens, cx))
            .child(activity_controls(issues, &tokens, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(list::render(issues, window, cx)),
            )
            .children(issues.menu_anchors.view())
            .into_any_element();
    }
    root.child(header(issues, &tokens, cx))
        .when(body.controls(), |root| {
            root.child(controls(issues, &tokens, cx))
        })
        .child(body_element(issues, &body, &tokens, window, cx))
        .children(issues.menu_anchors.view())
        .into_any_element()
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

fn no_projects(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> [AnyElement; 3] {
    [
        header(issues, tokens, cx).into_any_element(),
        if issues.activity.visible {
            activity_controls(issues, tokens, cx)
        } else {
            controls(issues, tokens, cx)
        }
        .into_any_element(),
        no_projects_state(issues, tokens, cx),
    ]
}

fn no_projects_state(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> AnyElement {
    if let crate::listing::Status::Failed(failure) = &issues.listing.status {
        return super::states::render(issues, &scope::Screen::Failed(failure.clone()), tokens, cx);
    }
    div()
        .px(issues.list_side_inset().unwrap_or(tokens.pad_x))
        .text_color(tokens.muted)
        .child("No active projects in this initiative")
        .into_any_element()
}
