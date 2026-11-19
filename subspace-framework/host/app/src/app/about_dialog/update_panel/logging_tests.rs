use super::updates::{ReleaseInfo, UpdateEvent, UpdateState};
use super::{UpdatePanel, display_error};
use gpui_kit::{AppContext as _, TestAppContext};
use subspace_core::log::{self, Level, Record};

fn records(seq: u64, message: &str) -> Vec<Record> {
    log::recent_after(seq)
        .into_iter()
        .filter(|record| {
            record.target == "app" && record.fields.contains(&("message".into(), message.into()))
        })
        .collect()
}

#[gpui_kit::test]
fn independent_update_failures_log_again_but_stale_completions_are_silent(cx: &mut TestAppContext) {
    let message = "update accepted occurrence fixture";
    let seq = log::recent().last().map_or(0, |record| record.seq);
    cx.update(|cx| {
        let panel = cx.new(|_| UpdatePanel::new(None, "Fixture app"));
        panel.update(cx, |panel, cx| {
            panel.apply_check(Err(message.into()), cx);
            assert!(records(seq, message).is_empty(), "idle completion is stale");
            for expected in [1, 2] {
                panel.apply(UpdateEvent::CheckRequested, cx);
                panel.apply(UpdateEvent::CheckRequested, cx);
                panel.apply_check(Err(message.into()), cx);
                panel.apply_check(Err(message.into()), cx);
                panel.apply(UpdateEvent::InstallResult(Err(message.into())), cx);
                assert_eq!(records(seq, message).len(), expected);
            }
        });
    });
    let records = records(seq, message);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].level, Level::Error);
    assert!(
        !records[0]
            .fields
            .iter()
            .any(|(key, _)| key == "error_chain")
    );
    assert!(
        records[0]
            .fields
            .contains(&("stage".into(), "check for updates".into()))
    );
}

#[gpui_kit::test]
fn accepted_install_failure_keeps_the_captured_chain_and_ignores_old_check(
    cx: &mut TestAppContext,
) {
    let product = subspace_product::fixture::products().unwrap().remove(0);
    let updater = subspace_update::Updater::for_product(&product, "1.0.0").unwrap();
    // Invalid versions fail before constructing the authenticated/network client.
    let underlying = updater.find("test-only-invalid-release").unwrap_err();
    let cause = underlying.to_string();
    let error = underlying.context("update chain fixture");
    assert_eq!(error.to_string(), "update chain fixture");
    let failure = display_error(error);
    let message = failure.message.clone();
    assert_eq!(message, format!("update chain fixture: {cause}"));
    let seq = log::recent().last().map_or(0, |record| record.seq);
    cx.update(|cx| {
        let panel = cx.new(|_| UpdatePanel::new(None, "Fixture app"));
        panel.update(cx, |panel, cx| {
            panel.state = UpdateState::Available(ReleaseInfo {
                version: "2.0.0".into(),
                html_url: "https://example.invalid/release".into(),
            });
            panel.apply(UpdateEvent::InstallRequested, cx);
            panel.apply_check(Err(failure.clone()), cx);
            assert!(records(seq, &message).is_empty());
            panel.apply(UpdateEvent::InstallResult(Err(failure)), cx);
        });
    });
    let records = records(seq, &message);
    assert_eq!(records.len(), 1);
    assert!(records[0].fields.contains(&("error_chain".into(), message)));
    assert!(
        records[0]
            .fields
            .contains(&("stage".into(), "install update".into()))
    );
}
