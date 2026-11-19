use super::{running, running_with_changes, state};
use crate::project::{
    HostEvent, InputValue, LivePatch, MessageContext, ProjectRevision, ResultUpdate, ResultValue,
    WorkerEvent, WorkerMessage,
};
use crate::project_runtime::SessionStatus;
use std::collections::BTreeMap;

fn text_patch(text: &str) -> LivePatch {
    let mut patch = LivePatch::default();
    patch.results.insert(
        "summary".into(),
        ResultUpdate::new(
            "synthetic-local",
            1234,
            ResultValue::Text { text: text.into() },
        ),
    );
    patch
}

#[test]
fn burst_coalesces_notifications_without_discarding_accepted_state() {
    let (sender, mut changes) = tokio::sync::watch::channel(());
    let mut state = running_with_changes(Some(sender));
    changes.borrow_and_update();
    let action = state.invoke("refresh", 0).unwrap();
    for index in 0..100 {
        let mut patch = text_patch(&format!("update-{index}"));
        if index == 0 {
            patch
                .values
                .insert("amount".into(), InputValue::Number(7.0));
        }
        if index == 1 {
            patch
                .values
                .insert("metric".into(), InputValue::String("total".into()));
        }
        state
            .accept(WorkerMessage::new(
                action.context.clone(),
                WorkerEvent::Patch { patch },
            ))
            .unwrap();
    }
    assert!(changes.has_changed().unwrap());
    changes.borrow_and_update();
    assert!(!changes.has_changed().unwrap());
    assert_eq!(state.live().values()["amount"], InputValue::Number(7.0));
    assert_eq!(
        state.live().values()["metric"],
        InputValue::String("total".into())
    );
    assert_eq!(
        state.live().results()["summary"].value,
        ResultValue::Text {
            text: "update-99".into()
        }
    );
}

#[test]
fn stale_completion_and_termination_wake_observers_and_retain_outcomes() {
    let (sender, mut changes) = tokio::sync::watch::channel(());
    let mut state = running_with_changes(Some(sender));
    changes.borrow_and_update();
    let stale = state.invoke("refresh", 0).unwrap();
    state
        .set_value("amount", InputValue::Number(7.0), 0)
        .unwrap();
    let pending = state.invoke("refresh", 1).unwrap();
    assert!(
        state
            .accept(WorkerMessage::new(
                stale.context,
                WorkerEvent::Result {
                    patch: LivePatch::default()
                }
            ))
            .is_err()
    );
    assert!(changes.has_changed().unwrap());
    changes.borrow_and_update();
    state.fail("worker failed");
    assert!(changes.has_changed().unwrap());
    let outcomes = state.take_completions();
    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes
            .iter()
            .any(|item| item.request_id() == pending.context.request_id && item.error().is_some())
    );
}

#[test]
fn ready_must_acknowledge_the_actual_init_request_before_running() {
    let mut state = state();
    assert_eq!(state.status(), SessionStatus::Starting);
    let init = state.init().expect("host init");
    assert!(matches!(init.event, HostEvent::Init { .. }));
    let wrong = MessageContext::new("unknown-request", "fixture-session", 0);
    assert!(
        state
            .accept(WorkerMessage::new(wrong, WorkerEvent::Ready))
            .is_err()
    );
    assert_eq!(state.status(), SessionStatus::Starting);
    assert!(
        state
            .accept(WorkerMessage::new(init.context, WorkerEvent::Ready))
            .is_ok()
    );
    assert_eq!(state.status(), SessionStatus::Running);
}

#[test]
fn committed_input_rejects_in_flight_old_revision_before_the_worker_echo() {
    let mut state = running();
    let old = state.invoke("refresh", 0).expect("first action");
    let input = state
        .set_value("amount", InputValue::Number(7.0), 0)
        .expect("commit input");
    assert_eq!(state.live().input_revision(), 1);
    assert_eq!(input.context.input_revision, 1);
    assert!(
        state
            .accept(WorkerMessage::new(
                old.context,
                WorkerEvent::Result {
                    patch: text_patch("stale")
                }
            ))
            .is_err()
    );
    assert!(state.live().results().is_empty());
    assert_eq!(state.live().values()["amount"], InputValue::Number(7.0));
    assert!(state.invoke("refresh", 0).is_err());
}

#[test]
fn unsolicited_updates_use_current_correlation_and_do_not_consume_action_completion() {
    let mut state = running();
    let action = state.invoke("refresh", 0).expect("action");
    let context = action.context;
    state
        .accept(WorkerMessage::new(
            context.clone(),
            WorkerEvent::Patch {
                patch: text_patch("timer"),
            },
        ))
        .expect("unsolicited update");
    assert_eq!(state.live().results()["summary"].evaluated_at_ms, 1234);
    assert!(state.take_completions().is_empty());
    state
        .accept(WorkerMessage::new(
            context.clone(),
            WorkerEvent::Result {
                patch: text_patch("complete"),
            },
        ))
        .expect("action completion");
    let completions = state.take_completions();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].request_id(), context.request_id);
    assert!(completions[0].error().is_none());
    assert!(
        state
            .accept(WorkerMessage::new(
                context,
                WorkerEvent::Result {
                    patch: LivePatch::default()
                }
            ))
            .is_err()
    );
}

#[test]
fn action_capacity_includes_untaken_completions_and_rejects_work_without_silent_loss() {
    let mut state = running();
    let requests: Vec<_> = (0..64)
        .map(|_| state.invoke("refresh", 0).expect("within capacity"))
        .collect();
    assert!(state.invoke("refresh", 0).is_err());
    for request in requests {
        state
            .accept(WorkerMessage::new(
                request.context,
                WorkerEvent::Result {
                    patch: LivePatch::default(),
                },
            ))
            .expect("retained action result");
    }
    assert!(state.invoke("refresh", 0).is_err());
    assert_eq!(state.take_completions().len(), 64);
    assert!(state.invoke("refresh", 0).is_ok());
}

#[test]
fn fatal_failure_retains_last_view_and_explicitly_finishes_all_pending_actions() {
    let mut state = running();
    let action = state.invoke("refresh", 0).expect("action");
    state
        .accept(WorkerMessage::new(
            action.context,
            WorkerEvent::Patch {
                patch: text_patch("retained"),
            },
        ))
        .expect("valid display");
    let previous = state.live().results().clone();
    state.fail("invalid worker frame");
    assert_eq!(state.status(), SessionStatus::Failed);
    assert_eq!(state.live().results(), &previous);
    let outcomes = state.take_completions();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].error().is_some());
    assert!(state.invoke("refresh", 0).is_err());
}

#[test]
fn diagnostic_tail_redacts_selected_values_across_chunks_before_truncation() {
    let mut state = state();
    let selected = vec!["dummy-test-only-credential".to_owned()];
    state.configure(
        ProjectRevision::new("fixture-definition", "fixture-source"),
        selected,
    );
    state.stderr(b"prefix dummy-test-");
    state.stderr(b"only-credential suffix\n");
    assert!(!state.diagnostics().contains("dummy-test-only-credential"));
    assert!(state.diagnostics().contains("[redacted]"));
    state.stderr(&vec![b'x'; 20 * 1024]);
    assert!(state.diagnostics().len() <= 16 * 1024);
}

#[test]
fn seeded_values_reach_init_without_inventing_a_host_input_revision() {
    let mut state = state();
    state
        .seed_values(BTreeMap::from([("amount".into(), InputValue::Number(9.0))]))
        .unwrap();
    let init = state.init().unwrap();
    assert_eq!(init.context.input_revision, 0);
    let HostEvent::Init { values, .. } = init.event else {
        panic!("expected Init");
    };
    assert_eq!(values["amount"], InputValue::Number(9.0));
    assert_eq!(values["metric"], InputValue::String("count".into()));
    assert!(state.seed_values(BTreeMap::new()).is_err());
}

#[test]
fn unknown_or_wrong_revision_completion_cannot_consume_a_real_pending_action() {
    let mut state = running();
    let action = state.invoke("refresh", 0).unwrap();
    let wrong_revision =
        MessageContext::new(action.context.request_id.clone(), "fixture-session", 1);
    assert!(
        state
            .accept(WorkerMessage::new(
                wrong_revision,
                WorkerEvent::Result {
                    patch: LivePatch::default()
                }
            ))
            .is_err()
    );
    let wrong_session =
        MessageContext::new(action.context.request_id.clone(), "replaced-session", 0);
    assert!(
        state
            .accept(WorkerMessage::new(
                wrong_session,
                WorkerEvent::Result {
                    patch: LivePatch::default()
                }
            ))
            .is_err()
    );
    assert!(state.take_completions().is_empty());
    assert!(
        state
            .accept(WorkerMessage::new(
                action.context,
                WorkerEvent::Result {
                    patch: LivePatch::default()
                }
            ))
            .is_ok()
    );
    assert_eq!(state.take_completions().len(), 1);
}

#[test]
fn diagnostic_redaction_does_not_split_a_complete_self_overlapping_value() {
    let mut state = state();
    state.configure(
        ProjectRevision::new("fixture-definition", "fixture-source"),
        vec!["test-test".into()],
    );
    state.stderr(b"test-test");
    state.stderr(b" suffix\n");
    assert_eq!(state.diagnostics(), "[redacted] suffix\n");
    state.stderr(b" test-");
    state.stderr(b"test\n");
    assert!(!state.diagnostics().contains("test-test"));
}
