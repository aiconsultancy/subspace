use super::{
    UpdatePanel,
    updates::{ReleaseInfo, UpdateState},
    view::AboutUpdates,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use subspace_core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_dashboard::prefs;
use subspace_update::{CheckOutcome, UpdateProgress, Updater};

pub(crate) fn set_available_for_test(version: Option<&str>, cx: &mut gpui_kit::App) {
    let panel = super::panel(cx).expect("test product must have an updater");
    panel.update(cx, |panel, cx| {
        panel.state = version.map_or(UpdateState::UpToDate, |version| {
            UpdateState::Available(ReleaseInfo {
                version: version.into(),
                html_url: "https://example.test/releases/test-update".into(),
            })
        });
        cx.notify();
    });
}

fn mount(cx: &mut TestAppContext) -> (gpui_kit::Entity<UpdatePanel>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        prefs::init_file(None, cx);
    });
    let product = subspace_product::fixture::products().unwrap().remove(0);
    let updater = Updater::for_product(&product, "1.0.0");
    let (about, cx) = cx.add_window_view(move |_, cx| {
        let panel = cx.new(|_| UpdatePanel::new(updater, "Subspace"));
        AboutUpdates::new(panel, |_, _| {}, cx)
    });
    let panel = about.read_with(cx, |view, _| view.panel.clone());
    cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(about, window, cx)
        });
    });
    (panel, cx)
}

fn settle(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn about_does_not_duplicate_the_settings_startup_preference(cx: &mut TestAppContext) {
    let (_, cx) = mount(cx);
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("about-auto-check-updates").is_none());
        assert_eq!(
            window.find("check-for-updates").label(),
            Some("Check for updates")
        );
    });
}

#[gpui_kit::test]
fn available_release_has_a_concise_external_link_and_install_action(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        panel.state = UpdateState::Available(ReleaseInfo {
            version: "1.2.0".into(),
            html_url: "https://example.test/releases/1.2.0".into(),
        });
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find("update-status").label(),
            Some("Version 1.2.0 is available")
        );
        assert_eq!(
            window.find("update-release-link").label(),
            Some("View on GitHub")
        );
        assert_eq!(
            window.find("install-update").label(),
            Some("Install update")
        );
        window.click("update-release-link", cx);
    });
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://example.test/releases/1.2.0")
    );
}

#[gpui_kit::test]
fn installation_shows_a_busy_status_instead_of_generic_copy(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        panel.state = UpdateState::Installing(ReleaseInfo {
            version: "1.2.0".into(),
            html_url: String::new(),
        });
        cx.notify();
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("update-status").label(),
            Some("Preparing update…")
        );
        assert!(window.find("update-progress-spinner").bounds().size.width > gpui_kit::px(0.));
    });
}

#[gpui_kit::test]
fn download_progress_changes_to_verification_then_clears_on_failure(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        panel.state = UpdateState::Installing(ReleaseInfo {
            version: "1.2.0".into(),
            html_url: String::new(),
        });
        panel.apply_progress(
            UpdateProgress::Downloading {
                received: 1_048_576,
                total: Some(2_097_152),
            },
            cx,
        );
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("update-status").label(),
            Some("Downloading update… 1.0 of 2.0 MiB")
        );
        assert_eq!(
            window.find("update-download-progress").label(),
            Some("Update download: 50%")
        );
    });
    panel.update(cx, |panel, cx| {
        panel.apply_progress(UpdateProgress::VerifyingSignature, cx)
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("update-status").label(),
            Some("Verifying app signature…")
        );
        assert!(window.try_find("update-download-progress").is_none());
    });
    panel.update(cx, |panel, cx| {
        panel.apply(
            super::updates::UpdateEvent::InstallResult(Err("signature rejected".into())),
            cx,
        );
        panel.apply_progress(UpdateProgress::Installing, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("update-status").label(),
            Some("signature rejected")
        );
        assert!(window.try_find("update-progress-spinner").is_none());
    });
}

#[gpui_kit::test]
fn startup_and_manual_checks_share_one_busy_state(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        assert!(panel.begin_check(cx).is_some());
        assert!(panel.begin_check(cx).is_none());
        assert_eq!(panel.state, UpdateState::Checking);
        panel.apply_check(Ok(CheckOutcome::UpToDate), cx);
        assert_eq!(panel.state, UpdateState::UpToDate);
    });
}

#[gpui_kit::test]
fn accepted_failure_copies_once_and_stale_results_keep_its_details(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        panel.apply(super::updates::UpdateEvent::CheckRequested, cx);
        panel.apply_check(Err("Synthetic update failure".into()), cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(
            window.find("update-copy-details").label(),
            Some("Copy details")
        );
        window.click("update-copy-details", cx);
    });
    let first = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(first.contains("Update check failed"));
    assert!(first.contains("Synthetic update failure"));
    assert!(first.contains("check for updates"));
    panel.update(cx, |panel, cx| {
        panel.apply(
            super::updates::UpdateEvent::InstallResult(Err("stale fixture".into())),
            cx,
        );
    });
    settle(cx);
    cx.update(|window, cx| window.click("update-copy-details", cx));
    let second = cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert_eq!(first, second);
}

#[gpui_kit::test]
fn checks_use_the_github_account_chosen_in_settings_at_the_time(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    panel.update(cx, |panel, cx| {
        let updater = panel.begin_check(cx).expect("the product has an updater");
        assert_eq!(updater.account(), None);
        panel.apply_check(Ok(CheckOutcome::UpToDate), cx);
        prefs::set(cx, GITHUB_ACCOUNT_KEY, "octo-dev_acme");
        let updater = panel.begin_check(cx).expect("the product has an updater");
        assert_eq!(updater.account(), Some("octo-dev_acme"));
    });
}

#[gpui_kit::test]
fn no_published_release_is_a_calm_status_without_error_details(cx: &mut TestAppContext) {
    let (panel, cx) = mount(cx);
    let seq = subspace_core::log::recent()
        .last()
        .map_or(0, |record| record.seq);
    panel.update(cx, |panel, cx| {
        assert!(panel.begin_check(cx).is_some());
        panel.apply_check(
            Ok(CheckOutcome::NoRelease {
                repository: "acme/releases".into(),
            }),
            cx,
        );
        assert!(panel.error_details.is_none());
        assert!(panel.pending_release.is_none());
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("update-status").label(),
            Some("No Subspace releases have been published to acme/releases yet")
        );
        assert!(window.try_find("update-copy-details").is_none());
        assert!(window.try_find("update-release-link").is_none());
        assert_eq!(
            window.find("check-for-updates").label(),
            Some("Check for updates")
        );
    });
    let logged = subspace_core::log::recent_after(seq)
        .into_iter()
        .filter(|record| {
            record
                .fields
                .iter()
                .any(|(_, value)| value.contains("acme/releases"))
        })
        .count();
    assert_eq!(logged, 0, "no release is not logged as an update error");
}
