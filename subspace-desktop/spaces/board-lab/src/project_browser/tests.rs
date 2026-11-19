use gpui_kit::TestAppContext;
use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
use std::collections::BTreeMap;
use std::time::Duration;

use super::ProjectBrowser;
use crate::project::{ProjectDefinition, ProjectRevision};
use crate::project_service;

mod layout;
mod observation;

fn project(root: &std::path::Path) -> crate::project::LoadedProject {
    let service = project_service::shared(root).unwrap();
    let definition = ProjectDefinition::new("example", "python-data", "main.py");
    service
        .create(
            &definition,
            BTreeMap::from([("main.py".into(), b"# test-only source\n".to_vec())]),
        )
        .unwrap()
}

fn mount_open<'a>(
    root: &std::path::Path,
    initial: crate::project_service::ProjectPresentation,
    cx: &'a mut TestAppContext,
) -> (
    gpui_kit::Entity<ProjectBrowser>,
    &'a mut gpui_kit::VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let path = root.to_owned();
    cx.add_window_view(move |window, cx| {
        let mut browser = ProjectBrowser::new(path, window, cx);
        browser.accept(initial, window, cx);
        browser
    })
}

#[gpui_kit::test]
async fn opens_and_closes_existing_project_through_native_commands(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let loaded = project(root.path());
    cx.update(gpui_kit::init);
    let path = root.path().to_owned();
    let (browser, cx) = cx.add_window_view(move |window, cx| {
        let mut browser = ProjectBrowser::new(path, window, cx);
        browser.projects = vec![loaded];
        browser
    });
    cx.update(|window, cx| window.click("open-project-example", cx));
    let handle = cx.update(|window, _| window.window_handle());
    cx.wait_for(handle, Duration::from_secs(2), |window, _| {
        window.try_find("project-close").is_some()
    })
    .await;
    cx.update(|window, cx| {
        let context = browser.read(cx).agent_context().unwrap();
        assert!(context.key.starts_with("board-project:"));
        assert!(context.content.contains("Definition (variable names only"));
        assert_eq!(window.find("project-start").label(), Some("Start…"));
        window.click("project-close", cx);
    });
    cx.wait_for(handle, Duration::from_secs(2), |window, _| {
        window.try_find("project-close").is_none()
    })
    .await;
    cx.update(|_, cx| assert!(browser.read(cx).presentation.is_none()));
    assert!(
        project_service::shared(root.path())
            .unwrap()
            .live_state("example")
            .is_err()
    );
}

#[gpui_kit::test]
async fn rejected_refresh_retains_last_native_view_and_revisions(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let loaded = project(root.path());
    let initial = project_service::shared(root.path())
        .unwrap()
        .open("example")
        .unwrap();
    let (browser, cx) = mount_open(root.path(), initial, cx);
    std::fs::write(
        loaded.directory.join("dashboard.json"),
        b"invalid test-only definition",
    )
    .unwrap();
    cx.update(|window, cx| window.click("open-project-example", cx));
    let handle = cx.update(|window, _| window.window_handle());
    let observed = browser.clone();
    cx.wait_for(handle, Duration::from_secs(2), move |_, cx| {
        observed.read(cx).error.is_some()
    })
    .await;
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("project-close").visible());
        assert_eq!(
            browser
                .read(cx)
                .presentation
                .as_ref()
                .unwrap()
                .working
                .revision,
            loaded.revision
        );
    });
}

#[gpui_kit::test]
fn hiding_browser_does_not_close_root_state(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    project(root.path());
    let service = project_service::shared(root.path()).unwrap();
    let initial = service.open("example").unwrap();
    let (browser, cx) = mount_open(root.path(), initial, cx);
    cx.update(|_, cx| browser.update(cx, |browser, _| browser.deactivate()));
    assert_eq!(
        service.live_state("example").unwrap().session_id(),
        "stopped"
    );
}

#[gpui_kit::test]
async fn creates_readable_project_with_enter_without_provisioning_or_starting(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let mut browser = ProjectBrowser::new(path.clone(), window, cx);
        browser.environments = vec![crate::contracts::EnvironmentInfo {
            spec: crate::contracts::EnvironmentSpec {
                id: "test-python".into(),
                title: "Test Python".into(),
                runtime: crate::contracts::RuntimeKind::Python,
                version: "3.12.8".into(),
                package_manager: "uv@0.6.0".into(),
                packages: Vec::new(),
                tools: Vec::new(),
            },
            ready: false,
            project: path.join("not-provisioned"),
            executable: path.join("not-installed"),
            summary: "Test-only configured environment".into(),
        }];
        browser.sync_environments(window, cx);
        browser
    });
    cx.update(|window, cx| {
        window.click("project-new", cx);
        window.click("project-create-id", cx);
        window.input("created-example", cx);
        window.press("enter", cx);
    });
    let handle = cx.update(|window, _| window.window_handle());
    cx.wait_for(handle, Duration::from_secs(2), |window, _| {
        window.try_find("project-close").is_some()
    })
    .await;
    let service = project_service::shared(root.path()).unwrap();
    let created = service.load("created-example").unwrap();
    assert_eq!(created.definition.environment, "test-python");
    assert!(
        std::str::from_utf8(&created.files["main.py"])
            .unwrap()
            .contains("def on_init(")
    );
    assert_eq!(
        service.open("created-example").unwrap().status,
        crate::project_runtime::SessionStatus::Stopped
    );
    assert!(
        !root.path().join("environments").exists(),
        "Create must not provision"
    );
}

#[test]
fn launch_disclosure_names_exact_revision_and_capabilities_without_values() {
    let mut definition = ProjectDefinition::new("example", "python-data", "main.py");
    definition.environment_keys.push("TEST_ONLY_TOKEN".into());
    definition.destinations.push("api.example.test:443".into());
    let revision = ProjectRevision::new("definition-exact", "source-exact");
    let text = super::commands::launch_disclosure(&definition, &revision);
    for expected in [
        "definition-exact",
        "source-exact",
        "python-data",
        "TEST_ONLY_TOKEN",
        "api.example.test:443",
        "can read",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[gpui_kit::test]
fn action_errors_report_session_and_request_once_per_shared_outcome(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let (browser, cx) = cx.add_window_view(move |window, cx| ProjectBrowser::new(path, window, cx));
    cx.update(|_, cx| {
        browser.update(cx, |browser, cx| {
            let outcome = crate::project_service::ActionOutcome::new(
                "session-one".into(),
                "request-one".into(),
                Some("Test-only action failure".into()),
            );
            browser.observe_outcomes(vec![outcome.clone()], cx);
            let error = browser.error.as_deref().unwrap();
            assert!(error.contains("session-one"));
            assert!(error.contains("request-one"));
            assert!(error.contains("Test-only action failure"));
            browser.error = None;
            browser.observe_outcomes(vec![outcome], cx);
            assert!(
                browser.error.is_none(),
                "unchanged shared history must not repeat notices"
            );
        })
    });
}
