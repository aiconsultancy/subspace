//! The reading column's body: the issue once it is read, or that it is
//! loading, could not be read or is not available.

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, TestSupportExt as _, div,
};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::row_menu::MenuChat;

use crate::Issues;
use crate::scope::Screen;
use crate::view::states;

pub(super) fn render(
    issues: &Issues,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    if let Some(detail) = open.detail() {
        return super::content(issues, detail, open, tokens, cx).into_any_element();
    }
    let identifier = open.identifier();
    match open.status() {
        IssueStatus::Loading => loading(identifier, tokens),
        IssueStatus::Error(reason) => {
            let screen = Screen::Failed(crate::sync::failure(reason));
            states::render_screen(issues, &screen, Some(identifier), tokens, cx)
        }
        IssueStatus::Unavailable | IssueStatus::Loaded => unavailable(identifier, tokens),
    }
}

fn loading(identifier: &str, tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("issues-detail-loading")
        .role(Role::Status)
        .aria_label(format!("Loading {identifier}…"))
        .test_support()
        .child(feedback::skeleton(tokens))
        .into_any_element()
}

fn unavailable(identifier: &str, tokens: &Tokens) -> AnyElement {
    let words = Words {
        chat: MenuChat::Off,
        ..Words::new(format!("{identifier} isn't available"), "")
    };
    div()
        .id("issues-detail-unavailable")
        .aria_label(words.title.clone())
        .test_support()
        .child(feedback::empty_words(
            tokens,
            "issue-unavailable",
            words,
            std::iter::empty::<AnyElement>(),
        ))
        .into_any_element()
}
