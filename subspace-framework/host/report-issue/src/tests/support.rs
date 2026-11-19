use crate::{Destination, Diagnostics, IssueRepository, ReportIssue};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, ElementId, Entity, FocusHandle, IntoElement,
    ParentElement as _, Render, Styled as _, TestAppContext, Window, WindowBounds, WindowOptions,
    div, px, size,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use subspace_client_github::Client;
use subspace_core::{
    config::Config,
    credentials::{Credentials, Environment, Runner},
};
use subspace_dashboard::action::{Intent, action_button};
use subspace_lucide::IconName;

pub(super) struct Harness {
    pub report: Entity<ReportIssue>,
    pub trigger: FocusHandle,
}
impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            action_button(
                "open-report",
                Intent::Neutral,
                Some(IconName::Bug),
                "Report issue…",
            )
            .track_focus(&self.trigger)
            .focus_on_click()
            .on_click(
                cx.listener(|this, _, window, cx| ReportIssue::show(&this.report, window, cx)),
            ),
        )
    }
}

struct FixtureEnvironment {
    auth: bool,
}
impl Environment for FixtureEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        match name {
            "GH_CONFIG_DIR" => Some("/test-only-report-fixture-no-config".into()),
            "GH_TOKEN" if self.auth => Some("test-only-nonworking-report-credential".into()),
            _ => None,
        }
    }
}
struct NoCommands;
impl Runner for NoCommands {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("No commands in the offline fixture".into())
    }
}

pub(super) fn client(base: &str, auth: bool) -> Arc<Client> {
    Arc::new(
        Client::new(Arc::new(Credentials::new(
            &Config::default(),
            FixtureEnvironment { auth },
            NoCommands,
        )))
        .with_api_url(base),
    )
}

/// A fictional repository: the Framework names no real one.
pub(super) fn repository() -> IssueRepository {
    IssueRepository::parse("example-org/example-app").unwrap()
}

pub(super) fn destination(client: Arc<Client>) -> Destination {
    Destination {
        client,
        repository: repository(),
    }
}

pub(super) fn diagnostics() -> Diagnostics {
    Diagnostics::new("subspace", "1.15.0")
        .with_build("dev", "fixture")
        .with_platform("macos", "aarch64")
        .with_appearance("one-dark", "medium")
}

pub(super) fn setup(
    cx: &mut TestAppContext,
    client: Arc<Client>,
) -> (AnyWindowHandle, Entity<ReportIssue>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        // The dialog entrance animation runs on wall-clock time,
        // so clicks must land on a settled dialog.
        cx.set_reduce_motion(true);
    });
    let (handle, harness) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1100.), px(1100.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| Harness {
                    report: cx.new(|cx| {
                        ReportIssue::new(
                            destination(client),
                            diagnostics(),
                            PathBuf::from("."),
                            window,
                            cx,
                        )
                    }),
                    trigger: cx.focus_handle().tab_stop(true),
                })
            },
        )
        .unwrap()
    });
    let report = cx.update(|cx| harness.read(cx).report.clone());
    cx.update_window(handle, |_, window, cx| window.click("open-report", cx))
        .unwrap();
    cx.run_until_parked();
    (handle, report)
}

pub(super) fn input(
    window: &mut Window,
    report: &Entity<ReportIssue>,
    label: &str,
    text: &str,
    cx: &mut gpui_kit::App,
) {
    window.click(field(report, label, cx), cx);
    window.input(text, cx);
}

pub(super) fn field(report: &Entity<ReportIssue>, label: &str, cx: &gpui_kit::App) -> ElementId {
    let form = &report.read(cx).form;
    match label {
        "Description (required)" => ("input", form.description.entity_id()).into(),
        "Steps to reproduce (optional)" => ("input", form.steps.entity_id()).into(),
        "Exact title" => ("input", form.preview_title.entity_id()).into(),
        "Exact issue body" => ("input", form.preview_body.entity_id()).into(),
        "Title (optional)" => "report-title".into(),
        _ => panic!("unknown fixture field"),
    }
}

pub(super) fn review(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| window.click("report-review", cx))
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn reviewed(
    cx: &mut TestAppContext,
    client: Arc<Client>,
    description: &str,
) -> (AnyWindowHandle, Entity<ReportIssue>) {
    let (handle, report) = setup(cx, client);
    cx.update_window(handle, |_, window, cx| {
        input(window, &report, "Description (required)", description, cx)
    })
    .unwrap();
    review(cx, handle);
    (handle, report)
}

pub(super) fn send(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    cx.update_window(handle, |_, window, cx| window.click("report-submit", cx))
        .unwrap();
    cx.run_until_parked();
}

pub(super) fn pending_report(
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<ReportIssue>, String) {
    let (handle, report) = setup(cx, client("http://127.0.0.1:1", false));
    super::multiple_images::pick_two(cx, handle);
    cx.update_window(handle, |_, window, cx| {
        input(
            window,
            &report,
            "Description (required)",
            &"界".repeat(1_000),
            cx,
        )
    })
    .unwrap();
    review(cx, handle);
    let body = cx.update(|cx| {
        report
            .read(cx)
            .form
            .preview_body
            .read(cx)
            .value()
            .to_string()
    });
    send(cx, handle);
    (handle, report, body)
}

pub(super) fn toasts(
    cx: &mut TestAppContext,
    report: &Entity<ReportIssue>,
) -> std::rc::Rc<std::cell::RefCell<Vec<subspace_dashboard::Toast>>> {
    let received = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = received.clone();
    cx.update(|cx| {
        cx.subscribe(report, move |_, event, _| {
            let crate::ReportIssueEvent::Toast(toast) = event;
            sink.borrow_mut().push(toast.clone());
        })
        .detach()
    });
    received
}
