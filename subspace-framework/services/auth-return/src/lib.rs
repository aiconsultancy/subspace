//! Fixed browser return copy and artwork, shared by identity providers.
//! Only the caller knows when an exchange has actually completed.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Completing,
    Succeeded,
    Cancelled,
    TimedOut,
    Failed,
    NotFound,
}

/// No provider messages, codes or tokens can enter this page.
pub fn page(state: State) -> String {
    let (title, message) = copy(state);
    include_str!("page.html")
        .replace("{{title}}", title)
        .replace("{{message}}", message)
        .replace(
            "{{subspace_logo}}",
            &STANDARD.encode(include_bytes!("../assets/subspace.svg")),
        )
}

fn copy(state: State) -> (&'static str, &'static str) {
    match state {
        State::Completing => (
            "Completing sign-in",
            "Subspace is checking your account. Keep Subspace open.",
        ),
        State::Succeeded => (
            "Signed in to Subspace",
            "You can close this tab and return to Subspace.",
        ),
        State::Cancelled => (
            "Sign-in cancelled",
            "Your account has not changed. Return to Subspace when you are ready to try again.",
        ),
        State::TimedOut => (
            "Sign-in timed out",
            "Return to Subspace and choose Sign in again.",
        ),
        State::Failed => (
            "Sign-in did not finish",
            "Return to Subspace to see why and try again.",
        ),
        State::NotFound => (
            "Not found",
            "This address only completes a Subspace sign-in.",
        ),
    }
}

/// Fixed script data for a streamed return page. The nonce belongs to the
/// HTTP response, never to the OAuth exchange. No request value is rendered.
pub fn completion_script(state: State) -> String {
    let (title, message) = copy(state);
    format!(
        "document.title={title};document.querySelector('h1').textContent={title};document.querySelector('main p').textContent={message};",
        title = serde_json::json!(title),
        message = serde_json::json!(message)
    )
}

#[cfg(test)]
mod tests {
    use super::{State, page};

    #[test]
    fn every_outcome_uses_the_same_self_contained_branded_page() {
        for state in [
            State::Completing,
            State::Succeeded,
            State::Cancelled,
            State::TimedOut,
            State::Failed,
            State::NotFound,
        ] {
            let html = page(state);
            assert!(html.contains("alt=\"Subspace\""));
            assert!(!html.contains("{{") && !html.contains("src=\"http"));
            assert_eq!(html.matches("src=\"data:image/svg+xml;base64,").count(), 1);
        }
        assert!(!page(State::Completing).contains("Signed in to Subspace"));
    }
}
