//! A state in place of the list: connection failures and
//! the errors of spec section 5, with the shared empty and error surfaces.

use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, TestSupportExt as _, div,
};
use subspace_framework::core::diagnostics::ErrorDetails;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::copy_details::CopyDetails;
use subspace_framework::dashboard::feedback::{self, Words};
use subspace_framework::dashboard::row_menu::MenuChat;
use subspace_framework::lucide::IconName;

use crate::Issues;
use crate::listing::Failure;
use crate::scope::{self, Screen, ScreenAction, ScreenText};

const INSTANCE: &str = "issues-state";

pub(super) fn render(
    issues: &Issues,
    screen: &Screen,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    render_screen(issues, screen, None, tokens, cx)
}

pub(super) fn render_screen(
    issues: &Issues,
    screen: &Screen,
    identifier: Option<&str>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let text = screen_text(screen, issues.profile_name(), identifier);
    let instance = if identifier.is_some() {
        "issues-detail-error"
    } else {
        INSTANCE
    };
    let actions: Vec<AnyElement> = scope::screen_actions(screen)
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            action_element(
                action,
                screen,
                (identifier.is_some(), &text),
                intent(index),
                cx,
            )
        })
        .collect();
    let title = text.title.clone();
    let words = Words {
        chat: MenuChat::Off,
        ..Words::new(text.title, text.message)
    };
    let surface = if text.error {
        feedback::error_box_words(tokens, instance, words, actions)
    } else {
        feedback::empty_words(tokens, instance, words, actions)
    };
    div()
        .id(instance)
        .aria_label(title)
        .test_support()
        .child(surface)
        .into_any_element()
}

/// The first action is the state's default; the others are secondary.
fn intent(_index: usize) -> Intent {
    Intent::Neutral
}

fn action_element(
    action: ScreenAction,
    screen: &Screen,
    (issue_read, text): (bool, &ScreenText),
    intent: Intent,
    cx: &mut Context<Issues>,
) -> AnyElement {
    match action {
        ScreenAction::Retry => feedback::recovery_action(
            "issues-retry",
            intent,
            IconName::RotateCw,
            "Retry",
            cx.listener(move |this, _, _, cx| {
                if issue_read {
                    this.reload_detail(cx);
                } else {
                    this.retry(cx);
                }
            }),
        )
        .into_any_element(),
        ScreenAction::CopyDetails => {
            CopyDetails::new("issues-copy-details", details(screen, issue_read, text))
                .into_any_element()
        }
    }
}

/// What the copied details say of a failed issue read, naming the service.
fn issue_read_failed() -> String {
    format!(
        "The {} issue read failed.",
        subspace_api_client::service_name()
    )
}

/// **Copy details**: the status and the kind of read only, never a body,
/// a query or an identifier.
fn details(screen: &Screen, issue_read: bool, text: &ScreenText) -> ErrorDetails {
    let code = match screen {
        Screen::Failed(Failure::Unexpected {
            status: Some(status),
        }) => format!("HTTP {status}"),
        Screen::Failed(Failure::Unexpected { status: None }) => "invalid response".to_owned(),
        _ => "network".to_owned(),
    };
    if issue_read {
        ErrorDetails::new(
            "Couldn't load issue",
            issue_read_failed(),
            "Issue detail read",
        )
        .with_code(code)
    } else {
        ErrorDetails::new(text.title.clone(), text.message.clone(), "Issues list read")
            .with_code(code)
    }
}

fn screen_text(screen: &Screen, profile: &str, identifier: Option<&str>) -> ScreenText {
    let mut text = scope::screen_text(screen, profile);
    if let Some(identifier) = identifier {
        text.title = format!("Couldn't load {identifier}");
    }
    text
}

#[cfg(test)]
mod details_tests {
    #[test]
    fn the_default_build_names_subspace_api_in_copied_issue_details() {
        assert_eq!(
            super::issue_read_failed(),
            "The Subspace API issue read failed."
        );
    }
}
