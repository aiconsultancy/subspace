//! The debug-build failure fixture for screenshots. When
//! `SUBSPACE_CHAT_FAILURE_FIXTURE` names a category, optionally followed by
//! `:details`, the chat panel starts no runtime and shows one tab with the
//! message "Fix my config" and a synthetic failure of that category, built
//! from fixture values only. A stopped or not-responding fixture shows the
//! chat as not ready, so its own Reconnect appears; the others are ready, so
//! Retry is enabled. With no runtime, Retry and Reconnect change nothing.

use std::time::Duration;

use subspace_agent::failure::{NOT_RESPONDING, STREAM_CLOSED, TurnContext};
use subspace_agent::{Entry, FailureCategory, TranscriptEntry, TurnFailure};

use super::super::tabs::{LastTurn, Tab};

const FIXTURE_ENV: &str = "SUBSPACE_CHAT_FAILURE_FIXTURE";
/// The suffix that opens the Details disclosure.
const DETAILS_SUFFIX: &str = ":details";
const MESSAGE: &str = "Fix my config";
const ELAPSED: Duration = Duration::from_secs(41);

/// The SDK error type, HTTP status and message of each category's fixture.
/// Stopped and not-responding use Subspace's own messages; the others are
/// representative of the runtime's text.
const FACTS: [(FailureCategory, &str, Option<i32>, &str); 9] = [
    (
        FailureCategory::NoResponse,
        "query",
        None,
        "No response was returned. Send your message again to retry.",
    ),
    (
        FailureCategory::SignIn,
        "authentication",
        Some(401),
        "Authentication failed. Sign in again to continue.",
    ),
    (
        FailureCategory::Limit,
        "rate_limit",
        Some(429),
        "You have reached the rate limit for this model. Try again later.",
    ),
    (
        FailureCategory::NotAllowed,
        "policy",
        Some(403),
        "This model is not enabled by your organization's Copilot policy.",
    ),
    (
        FailureCategory::ContextFull,
        "context_limit",
        Some(400),
        "The prompt is longer than the model's context window.",
    ),
    (
        FailureCategory::Network,
        "model_call",
        Some(503),
        "The Copilot service is unavailable.",
    ),
    (
        FailureCategory::Stopped,
        "runtime_stopped",
        None,
        STREAM_CLOSED,
    ),
    (
        FailureCategory::NotResponding,
        "runtime_not_responding",
        None,
        NOT_RESPONDING,
    ),
    (
        FailureCategory::Unknown,
        "execution",
        None,
        "The turn ended with an unexpected runtime error.",
    ),
];

/// A fixture the environment asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::agent_chat) struct Fixture {
    category: FailureCategory,
    details: bool,
}

/// The fixture `SUBSPACE_CHAT_FAILURE_FIXTURE` names, if any.
pub(in crate::agent_chat) fn from_env() -> Option<Fixture> {
    parse(&std::env::var(FIXTURE_ENV).unwrap_or_default())
}

/// `no-response` or `no-response:details`, for every category slug.
pub(super) fn parse(value: &str) -> Option<Fixture> {
    let value = value.trim();
    let (name, details) = match value.strip_suffix(DETAILS_SUFFIX) {
        Some(name) => (name, true),
        None => (value, false),
    };
    FailureCategory::from_slug(name).map(|category| Fixture { category, details })
}

impl Fixture {
    /// The one tab the panel shows: the message, then its failure.
    pub(in crate::agent_chat) fn tab(self) -> Tab {
        let mut tab = Tab::new(1);
        tab.title = MESSAGE.into();
        if !self.category.needs_reconnect() {
            tab.ready();
        }
        tab.entries.push(TranscriptEntry {
            id: "fixture-message".into(),
            entry: Entry::User {
                text: MESSAGE.into(),
                steering: false,
            },
        });
        tab.last_turn = Some(LastTurn {
            text: MESSAGE.into(),
            context: Vec::new(),
            images: Vec::new(),
        });
        super::super::attention::apply_chat_event(
            &mut tab,
            subspace_agent::ChatEvent::Failure(Box::new(self.failure())),
        );
        if let Some(shown) = &mut tab.failure {
            shown.details_open = self.details;
        }
        tab
    }

    fn failure(self) -> TurnFailure {
        let (error_type, status_code, message) = FACTS
            .iter()
            .find(|facts| facts.0 == self.category)
            .map_or(("runtime", None, ""), |facts| (facts.1, facts.2, facts.3));
        let session_error = !self.category.needs_reconnect();
        let id = |kind: &str| {
            session_error.then(|| format!("00000000-0000-0000-0000-fixture0000{kind}"))
        };
        TurnFailure {
            error_type: error_type.to_owned(),
            status_code,
            provider_call_id: id("1"),
            service_request_id: id("2"),
            originating_message_id: id("3"),
            model: Some("claude-sonnet-5.5".into()),
            effort: Some("high".into()),
            runtime_version: Some("1.0.105".into()),
            elapsed: Some(ELAPSED),
            attachments: 1,
            ..TurnFailure::stopped(self.category, message, &TurnContext::default(), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use subspace_agent::{Entry, FailureCategory};

    use super::{Fixture, parse};

    #[test]
    fn the_value_names_a_category_and_may_open_the_details() {
        assert_eq!(
            parse("no-response"),
            Some(Fixture {
                category: FailureCategory::NoResponse,
                details: false
            })
        );
        assert_eq!(
            parse(" not-responding:details "),
            Some(Fixture {
                category: FailureCategory::NotResponding,
                details: true
            })
        );
        assert_eq!(parse(""), None);
        assert_eq!(parse(":details"), None);
        assert_eq!(parse("no-response:open"), None);
    }

    #[test]
    fn every_category_has_a_fixture_tab_with_the_message_and_its_failure() {
        for category in FailureCategory::ALL {
            let tab = parse(&format!("{}:details", category.slug()))
                .expect("a fixture")
                .tab();
            let shown = tab.failure.as_ref().expect("a fixture failure");
            let failure = &shown.failure;
            assert_eq!(failure.category, category);
            assert_ne!(failure.message, "", "{category:?}");
            assert_eq!(failure.model.as_deref(), Some("claude-sonnet-5.5"));
            assert!(shown.details_open);
            assert_eq!(tab.is_ready(), !category.needs_reconnect(), "{category:?}");
            assert!(matches!(
                tab.entries.as_slice(),
                [row] if matches!(&row.entry, Entry::User { text, .. } if text == "Fix my config")
            ));
            assert_eq!(
                tab.last_turn.as_ref().map(|turn| turn.text.as_str()),
                Some("Fix my config")
            );
        }
    }
}
