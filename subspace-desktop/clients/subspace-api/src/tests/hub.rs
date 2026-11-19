use super::{client, common, page, wait};
use crate::{
    ListQuery, ListTab,
    hub::{Hub, HubStatus},
};
use serde_json::json;
use subspace_framework::client_test_support::Mock;

#[test]
fn connection_reads_me_projects_and_only_selected_project_workflows() {
    let mock = Mock::new(common);
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    let snapshot = wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    assert_eq!(snapshot.me().unwrap().display_name, "Ada Fixture");
    assert_eq!(snapshot.projects()[0].key, "SUB");
    assert!(snapshot.projects()[0].states.is_empty());
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("workflow-states"))
    );
    hub.activate(ListQuery::new(ListTab::Active).project(Some("SUB")))
        .unwrap();
    let loaded = wait(&hub, |snapshot| snapshot.list().is_some());
    assert_eq!(loaded.projects()[0].states[0].project_id, "project-1");
    assert!(
        mock.requests()
            .iter()
            .any(|request| request.path.contains("workflow-states?limit=200"))
    );
    assert!(
        mock.requests()
            .iter()
            .any(|request| request.path.contains("includeArchived=true"))
    );
    hub.stop();
}

#[test]
fn project_and_state_catalogues_follow_cursors_without_eager_fanout() {
    let mock = Mock::new(|request| {
        if request.path.starts_with("/api/v1/projects?") {
            let next = !request.path.contains("cursor=");
            let key = if next { "SUB" } else { "OPS" };
            (
                200,
                page(
                    vec![
                        json!({"id":key,"key":key,"name":"Fixture project","defaultStateId":"state","archivedAt":null}),
                    ],
                    next.then_some("next"),
                ),
            )
        } else {
            common(request)
        }
    });
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    let snapshot = wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    assert_eq!(snapshot.projects().len(), 2);
    assert!(
        snapshot
            .projects()
            .iter()
            .all(|project| project.states.is_empty())
    );
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|request| request.path.starts_with("/api/v1/projects?"))
            .count(),
        2
    );
    hub.stop();
}

#[test]
fn connection_retry_is_reload_and_does_not_use_an_auth_route() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let blocked = Arc::new(AtomicBool::new(true));
    let edge = blocked.clone();
    let mock = Mock::new(move |request| {
        if edge.load(Ordering::SeqCst) {
            (403, "edge access page".into())
        } else {
            common(request)
        }
    });
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    wait(&hub, |snapshot| {
        matches!(
            snapshot.status(),
            HubStatus::Error(crate::hub::ReadError::SignInNeeded)
        )
    });
    blocked.store(false, Ordering::SeqCst);
    hub.reload(ListQuery::default()).unwrap();
    let snapshot = wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    assert!(snapshot.me().is_some());
    assert!(
        mock.requests()
            .iter()
            .all(|request| !request.path.contains("auth") && !request.path.contains("orgs"))
    );
    hub.stop();
}

#[test]
fn reference_resolution_deduplicates_and_uses_the_builtin_profile() {
    let mock = Mock::new(common);
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    hub.resolve_reference("SUB-1", "other-profile");
    assert!(hub.irc_snapshot().cards().is_empty());
    hub.resolve_reference("SUB-1", crate::PROFILE_ID);
    hub.resolve_reference("SUB-1", crate::PROFILE_ID);
    wait(&hub, |_| !hub.irc_snapshot().cards().is_empty());
    let irc = hub.irc_snapshot();
    assert_eq!(irc.cards()[0].card().unwrap().title(), "A fixture issue");
    assert_eq!(irc.project_keys(), &["SUB"]);
    assert!(irc.api_origin().starts_with("http://127.0.0.1:"));
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|request| request.path == "/api/v1/issues/SUB-1")
            .count(),
        1
    );
    hub.stop();
    assert!(hub.irc_snapshot().cards().is_empty());
}

#[test]
fn missing_references_are_neutral_but_transport_errors_are_retryable() {
    let mock = Mock::new(common);
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    hub.resolve_reference("SUB-404", crate::PROFILE_ID);
    wait(&hub, |_| !hub.irc_snapshot().cards().is_empty());
    assert!(hub.irc_snapshot().cards()[0].card().is_none());
    hub.resolve_reference("SUB-404", crate::PROFILE_ID);
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|request| request.path == "/api/v1/issues/SUB-404")
            .count(),
        1
    );
    hub.stop();
}

#[test]
fn list_network_failure_keeps_independent_reference_cards_available() {
    let mock = Mock::new(|request| {
        if request.path.contains("/issues/groups?") {
            (0, String::new())
        } else {
            common(request)
        }
    });
    let hub = Hub::from_client(client(&mock), crate::PROFILE_NAME);
    wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    hub.resolve_reference("SUB-1", crate::PROFILE_ID);
    wait(&hub, |_| !hub.irc_snapshot().cards().is_empty());
    hub.activate(ListQuery::default()).unwrap();
    wait(&hub, |snapshot| {
        matches!(
            snapshot.status(),
            HubStatus::Error(crate::hub::ReadError::Network)
        )
    });
    let snapshot = hub.irc_snapshot();
    assert_eq!(snapshot.cards()[0].card().unwrap().identifier(), "SUB-1");
    assert!(snapshot.reference_error("SUB-1").is_none());
    hub.stop();
}
