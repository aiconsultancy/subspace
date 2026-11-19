//! Exercise the real service-to-browser path with GPUI's deterministic clock.
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::super::ProjectBrowser;
use crate::project::{InputDeclaration, InputValue, ProjectDefinition};
use crate::project_service::{InputRevision, ProjectService, shared};

fn fixture(root: &std::path::Path) -> Arc<ProjectService> {
    let mut definition = ProjectDefinition::new("live-test", "python-data", "main.py");
    definition.inputs.insert(
        "sample".into(),
        InputDeclaration::Number {
            default: 0.,
            min: 0.,
            max: 1000.,
            step: 1.,
        },
    );
    let service = shared(root).unwrap();
    service
        .create(
            &definition,
            BTreeMap::from([("main.py".into(), b"# test only\n".to_vec())]),
        )
        .unwrap();
    service
}

fn publish(service: &ProjectService, value: u64) {
    let live = service.live_state("live-test").unwrap();
    service
        .set_value(
            "live-test",
            "sample",
            InputValue::Number(value as f64),
            &InputRevision::new(live.session_id(), live.input_revision()),
        )
        .unwrap();
}

fn revision(browser: &Entity<ProjectBrowser>, cx: &mut VisualTestContext) -> u64 {
    browser.read_with(cx, |browser, _| {
        browser.presentation.as_ref().unwrap().live.input_revision()
    })
}

fn advance(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn updates_at_ten_hz_and_coalesces_bursts_without_losing_latest_state(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let (browser, cx) = super::mount_open(root.path(), service.open("live-test").unwrap(), cx);
    let updates = Rc::new(Cell::new(0));
    let counted = updates.clone();
    let _subscription =
        cx.update(|_, cx| cx.observe(&browser, move |_, _| counted.set(counted.get() + 1)));
    // Repeated activation must not create another notification consumer.
    cx.update(|window, cx| browser.update(cx, |browser, cx| browser.observe(window, cx)));
    cx.update(|window, cx| browser.update(cx, |browser, cx| browser.observe(window, cx)));
    cx.run_until_parked();
    for sample in 1..=10 {
        publish(&service, sample);
        cx.run_until_parked();
        advance(cx, 33);
        assert_eq!(
            revision(&browser, cx),
            sample,
            "100ms publications must not wait one second"
        );
        assert_eq!(
            updates.get(),
            sample,
            "one presentation update per publication"
        );
        advance(cx, 67);
    }
    for sample in 11..=110 {
        publish(&service, sample);
    }
    cx.run_until_parked();
    advance(cx, 32);
    assert_eq!(
        revision(&browser, cx),
        10,
        "a burst must wait for its frame budget"
    );
    advance(cx, 1);
    assert_eq!(revision(&browser, cx), 110);
    assert_eq!(updates.get(), 11, "one notification for the whole burst");
    advance(cx, 33);
    assert_eq!(revision(&browser, cx), 110, "no queued replay of a burst");
    assert_eq!(updates.get(), 11);
}

#[gpui_kit::test]
fn slow_fallback_observes_external_edits_and_retries_after_busy(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let initial = service.open("live-test").unwrap();
    let file = initial.working.directory.join("dashboard.json");
    let mut definition = initial.working.definition.clone();
    let (browser, cx) = super::mount_open(root.path(), initial, cx);
    cx.update(|window, cx| {
        browser.update(cx, |browser, cx| {
            browser.observe(window, cx);
            browser.busy = true;
        })
    });
    cx.run_until_parked();
    publish(&service, 1);
    cx.run_until_parked();
    advance(cx, 100);
    assert_eq!(
        revision(&browser, cx),
        0,
        "busy browsers must not accept a refresh"
    );
    cx.update(|_, cx| browser.update(cx, |browser, _| browser.busy = false));
    definition.title = "External edit".into();
    std::fs::write(file, serde_json::to_vec(&definition).unwrap()).unwrap();
    advance(cx, 1000);
    advance(cx, 33);
    browser.read_with(cx, |browser, _| {
        let presentation = browser.presentation.as_ref().unwrap();
        assert_eq!(presentation.live.input_revision(), 1);
        assert_eq!(presentation.working.definition.title, "External edit");
    });
}

#[gpui_kit::test]
fn deactivation_cancels_pending_refresh_and_window_drop_releases_observer(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let service = fixture(root.path());
    let (browser, cx) = super::mount_open(root.path(), service.open("live-test").unwrap(), cx);
    cx.update(|window, cx| browser.update(cx, |browser, cx| browser.observe(window, cx)));
    cx.run_until_parked();
    publish(&service, 1);
    cx.run_until_parked();
    cx.update(|_, cx| browser.update(cx, |browser, _| browser.deactivate()));
    advance(cx, 2000);
    assert_eq!(revision(&browser, cx), 0);
    cx.update(|window, cx| browser.update(cx, |browser, cx| browser.observe(window, cx)));
    cx.run_until_parked();
    let weak = browser.downgrade();
    cx.update(|window, _| window.remove_window());
    drop(browser);
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "the observer must not keep the view alive"
    );
    publish(&service, 2);
    advance(cx, 2000);
    assert!(weak.upgrade().is_none());
    assert_eq!(service.live_state("live-test").unwrap().input_revision(), 2);
}
