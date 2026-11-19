use super::support::{client, reviewed, send, toasts};
use crate::report::Phase;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use subspace_client_test_support::Mock;

const CREATED: &str =
    "{\"number\":42,\"html_url\":\"https://github.com/example-org/example-app/issues/42\"}";

#[gpui_kit::test]
fn submits_the_reviewed_plan_once_and_retains_created_link(cx: &mut TestAppContext) {
    let server = Mock::new(|request| {
        if request.method == "POST" {
            (201, CREATED.into())
        } else {
            (404, "{}".into())
        }
    });
    let (handle, report) = reviewed(
        cx,
        client(server.base_url(), true),
        "Create synthetic issue",
    );
    let expected = cx.update(|cx| {
        let Phase::Review(plan) = &report.read(cx).state.phase else {
            panic!("preview")
        };
        (plan.title().to_owned(), plan.body().to_owned())
    });
    let received = toasts(cx, &report);
    cx.update_window(handle, |_, window, cx| {
        window.click("report-submit", cx);
        window.click("report-submit", cx);
        report.update(cx, |report, cx| {
            assert!(report.state.busy());
            report.submit(window, cx);
            report.review(window, cx);
            assert!(matches!(report.state.phase, Phase::Submitting));
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        report.update(cx, |report, cx| {
            let Phase::Created(url) = &report.state.phase else {
                panic!("created result")
            };
            assert_eq!(url, "https://github.com/example-org/example-app/issues/42");
            report.submit(window, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("report-submit").is_none());
        window.click("report-open-result", cx);
    })
    .unwrap();
    assert_post(&server, &expected);
    assert_eq!(received.borrow().len(), 1);
    assert_eq!(
        received.borrow()[0].link.as_ref().unwrap().href.as_ref(),
        "https://github.com/example-org/example-app/issues/42"
    );
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/example-org/example-app/issues/42")
    );
}

fn assert_post(server: &Mock, expected: &(String, String)) {
    let posts: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(posts.len(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&posts[0].body).unwrap(),
        serde_json::json!({"title":expected.0,"body":expected.1})
    );
}

#[gpui_kit::test]
fn friendly_failure_preserves_draft_and_requires_a_new_review(cx: &mut TestAppContext) {
    let server = Mock::new(|_| (403, "{\"message\":\"TEST_ONLY_PRIVATE_HTTP_BODY\"}".into()));
    let (handle, report) = reviewed(
        cx,
        client(server.base_url(), true),
        "Retain this exact draft",
    );
    send(cx, handle);
    cx.update(|cx| {
        let report = report.read(cx);
        assert!(matches!(report.state.phase, Phase::Editing));
        assert_eq!(
            report.form.draft(cx).description(),
            "Retain this exact draft"
        );
        let error = report.state.error.as_deref().unwrap();
        assert!(error.contains("GitHub denied access"));
        assert!(!error.contains("TEST_ONLY_PRIVATE_HTTP_BODY"));
    });
    assert_eq!(server.requests().len(), 1);
}

#[gpui_kit::test]
fn uncertain_post_requires_check_then_deliberate_retry(cx: &mut TestAppContext) {
    let server = Mock::new(|_| (201, "malformed accepted response".into()));
    let (handle, report) = reviewed(cx, client(server.base_url(), true), "Uncertain fixture");
    send(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(matches!(
            report.read(cx).state.phase,
            Phase::Uncertain { opened: false }
        ));
        window.click("report-retry-checked", cx);
        report.update(cx, |report, cx| {
            report.retry_checked(cx);
            assert!(matches!(
                report.state.phase,
                Phase::Uncertain { opened: false }
            ));
        });
        window.click("report-check-github", cx);
        window.click("report-retry-checked", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(matches!(report.read(cx).state.phase, Phase::Editing)));
    assert_eq!(
        server.requests().len(),
        1,
        "checking and acknowledging cannot repeat POST"
    );
}
