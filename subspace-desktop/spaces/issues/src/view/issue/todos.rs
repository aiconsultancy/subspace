//! The open issue's to-dos: each named list with how many of its items are
//! done, and a check per item. A check is saved at the version shown; one
//! that changed elsewhere is read again instead of overwritten.

use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{Context, Div, InteractiveElement as _, ParentElement as _, Styled as _, div};
use subspace_api_client::hub::{IssueSnapshot, TodoListView};
use subspace_api_client::nett::TodoItem;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{ActionState, Intent, action_button};
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::requests::Request;

pub(super) fn render(
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<Div> {
    let lists = open.todos();
    if lists.is_none_or(<[TodoListView]>::is_empty) && !open.todos_failed() {
        return None;
    }
    let body = v_flex()
        .min_w_0()
        .gap_3()
        .children(open.todo_notice().map(|notice| {
            div()
                .id("issue-todos-notice")
                .text_color(tokens.muted)
                .child(notice)
        }))
        .children(
            lists
                .into_iter()
                .flatten()
                .map(|list| list_section(list, open, tokens, cx)),
        )
        .children(open.todos_failed().then(|| retry(open, tokens, cx)));
    Some(
        div().min_w_0().child(
            div()
                .id("issue-todos")
                .min_w_0()
                .child(super::panel::group("To-dos", IconName::ListChecks, tokens).child(body)),
        ),
    )
}

/// "Release checks · 2 of 5 done".
pub(crate) fn heading(list: &TodoListView) -> String {
    let (done, total) = list.counts();
    format!("{} · {done} of {total} done", list.list.name)
}

fn list_section(
    list: &TodoListView,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    v_flex()
        .min_w_0()
        .gap_1()
        .child(
            div()
                .id(format!("issue-todo-list:{}", list.list.id))
                .text_color(tokens.muted)
                .child(heading(list)),
        )
        .children(list.items.iter().map(|item| row(item, open, tokens, cx)))
}

fn row(item: &TodoItem, open: &IssueSnapshot, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let (id, done) = (item.id.clone(), !item.completed);
    let icon = if item.completed {
        IconName::Check
    } else {
        IconName::Square
    };
    let button = action_button(
        format!("issue-todo:{}", item.id),
        Intent::Quiet,
        Some(icon),
        item.text.clone(),
    )
    .small()
    .max_w_full()
    .accessibility_label(format!(
        "{}, {}",
        item.text,
        if item.completed { "done" } else { "not done" }
    ))
    .on_click(cx.listener(move |this, _, _, cx| {
        this.request(
            Request::SetTodo {
                item: id.clone(),
                done,
            },
            cx,
        );
    }));
    let button = if open.todo_saving(&item.id) {
        ActionState::Busy.apply(button)
    } else {
        button
    };
    h_flex()
        .min_w_0()
        .border_b_1()
        .border_color(tokens.hair)
        .child(button)
}

fn retry(open: &IssueSnapshot, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let button = action_button(
        "issue-todos-retry",
        Intent::Neutral,
        Some(IconName::RotateCw),
        "Retry",
    )
    .on_click(cx.listener(|this, _, _, cx| this.request(Request::RetryTodos, cx)));
    let button = if open.todos_loading() {
        ActionState::Busy.apply(button)
    } else {
        button
    };
    h_flex()
        .gap_2()
        .child(div().text_color(tokens.muted).child("Couldn't load to-dos"))
        .child(button)
}
