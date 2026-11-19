//! The About dialog's update section (REQ-06) and the startup check (REQ-07).
//! State lives in a GPUI global set once at startup, so it survives closing
//! and reopening the dialog.

pub(in crate::app) mod compact;
#[cfg(test)]
mod compact_tests;
mod error_details;
#[cfg(any(test, feature = "test-support", feature = "capture"))]
mod fixture;
mod install;
#[cfg(test)]
mod logging_tests;
#[cfg(test)]
mod runtime_tests;
mod updates;
mod view;
#[cfg(any(test, feature = "test-support"))]
pub(in crate::app) use fixture::seed_failure;
#[cfg(any(test, feature = "capture"))]
pub(in crate::app) use fixture::seed_installed;

use std::rc::Rc;

use gpui_kit::{App, AppContext as _, Context, Entity, SharedString, Subscription, Window};
use subspace_core::credentials::GITHUB_ACCOUNT_KEY;
use subspace_core::diagnostics::ErrorDetails;
use subspace_dashboard::prefs;
use subspace_update::{CheckOutcome, Release, UpdateProgress, Updater};

use super::super::SubspaceApp;
use error_details::UpdateFailure;
use updates::{
    Found, UpdateAction, UpdateEvent, UpdateState, progress_text, should_check_at_startup,
    startup_release, status_text,
};

/// All three network calls report failure the same way (REQ-06's error text).
fn display_error(error: impl std::fmt::Display) -> UpdateFailure {
    let chain = format!("{error:#}");
    UpdateFailure {
        message: chain.clone(),
        chain: Some(chain),
    }
}

fn found(outcome: &Result<CheckOutcome, UpdateFailure>) -> Result<Found, UpdateFailure> {
    outcome.as_ref().map(Found::from).map_err(Clone::clone)
}

type Restart = Rc<dyn Fn(&mut Window, &mut App)>;

struct UpdatePanel {
    updater: Option<Updater>,
    app_name: SharedString,
    state: UpdateState,
    error_details: Option<ErrorDetails>,
    /// The full release the last check found, kept alongside `state` to call
    /// `install` with; `UpdateState` only keeps the fields the UI shows.
    pending_release: Option<Release>,
    progress: UpdateProgress,
}

impl UpdatePanel {
    pub(super) fn new(updater: Option<Updater>, app_name: impl Into<SharedString>) -> Self {
        Self {
            updater,
            app_name: app_name.into(),
            state: UpdateState::default(),
            error_details: None,
            pending_release: None,
            progress: UpdateProgress::Preparing,
        }
    }

    pub(super) fn has_updater(&self) -> bool {
        self.updater.is_some()
    }

    fn current_version(&self) -> &str {
        self.updater.as_ref().map_or("", Updater::current_version)
    }

    fn apply(&mut self, event: UpdateEvent, cx: &mut Context<Self>) {
        let failure = error_details::accepted_failure(&self.state, &event);
        if let Some(details) = &failure {
            subspace_dashboard::copy_details::log_error("app", "update-error", details, cx);
        }
        self.state = std::mem::take(&mut self.state).apply(event);
        self.error_details = error_details::retained_failure(
            &self.state,
            failure.or_else(|| self.error_details.take()),
        );
        cx.notify();
    }

    /// "Check for updates": runs off the UI thread and never blocks the window.
    fn check(&mut self, cx: &mut Context<Self>) {
        let Some(updater) = self.begin_check(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { updater.check() }).await;
            let outcome = result.map_err(display_error);
            _ = this.update(cx, |panel, cx| panel.apply_check(outcome, cx));
        })
        .detach();
    }

    fn begin_check(&mut self, cx: &mut Context<Self>) -> Option<Updater> {
        if self.state.busy() {
            return None;
        }
        let updater = self.account_updater(cx)?;
        self.apply(UpdateEvent::CheckRequested, cx);
        Some(updater)
    }

    fn account_updater(&self, cx: &App) -> Option<Updater> {
        self.updater
            .clone()
            .map(|updater| updater.with_account(prefs::get(cx, GITHUB_ACCOUNT_KEY)))
    }

    fn apply_check(
        &mut self,
        outcome: Result<CheckOutcome, UpdateFailure>,
        cx: &mut Context<Self>,
    ) {
        let event = found(&outcome);
        self.pending_release = outcome.ok().and_then(|outcome| outcome.newer().cloned());
        self.apply(UpdateEvent::CheckResult(event), cx);
    }

    /// "Install": runs off the UI thread and never blocks the window.
    fn install(&mut self, cx: &mut Context<Self>) {
        if self.state.install_target().is_none() {
            return;
        }
        let Some(updater) = self.account_updater(cx) else {
            return;
        };
        let Some(release) = self.pending_release.clone() else {
            return;
        };
        self.progress = UpdateProgress::Preparing;
        self.apply(UpdateEvent::InstallRequested, cx);
        install::start(updater, release, cx);
    }

    /// The modal owns restart navigation. The retained updater owns Check and Install.
    fn run(
        panel: &Entity<Self>,
        action: UpdateAction,
        window: &mut Window,
        cx: &mut App,
        restart: impl FnOnce(&mut Window, &mut App),
    ) {
        if action != UpdateAction::of(&panel.read(cx).state) {
            return;
        }
        match action {
            UpdateAction::Check => panel.update(cx, Self::check),
            UpdateAction::Install => panel.update(cx, Self::install),
            UpdateAction::Restart => restart(window, cx),
        }
    }

    fn apply_progress(&mut self, progress: UpdateProgress, cx: &mut Context<Self>) {
        if !matches!(self.state, UpdateState::Installing(_)) {
            return;
        }
        self.progress = progress;
        cx.notify();
    }

    fn status(&self) -> Option<String> {
        if matches!(self.state, UpdateState::Installing(_)) {
            Some(progress_text(&self.progress))
        } else {
            status_text(&self.state, &self.app_name, self.current_version())
        }
    }

    fn release_link(&self) -> Option<&str> {
        self.state
            .install_target()
            .map(|release| release.html_url.as_str())
    }
}

/// Once, after the window opens: checks in the background when an update
/// source exists and the setting is on, and toasts a newer release (REQ-07).
/// A failed or unauthenticated check shows no toast; About shows its result.
fn run_startup_check(
    panel: Entity<UpdatePanel>,
    app: gpui_kit::WeakEntity<SubspaceApp>,
    setting_enabled: bool,
    cx: &mut App,
) {
    let updater = panel.read(cx).updater.clone();
    if !should_check_at_startup(updater.is_some(), setting_enabled) {
        return;
    }
    let Some(updater) = panel.update(cx, |panel, cx| panel.begin_check(cx)) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_spawn(async move { updater.check() }).await;
        let outcome = result.map_err(display_error);
        let release = startup_release(&found(&outcome)).cloned();
        panel.update(cx, |panel, cx| panel.apply_check(outcome, cx));
        let Some(release) = release else {
            return;
        };
        let version = release.version.clone();
        _ = app.update(cx, |app, cx| app.push_update_toast(&version, cx));
    })
    .detach();
}

/// The About dialog's update entity (REQ-06), with no updater at all when
/// this app's product has no update source.
struct UpdatePanelHandle(Entity<UpdatePanel>);

impl gpui_kit::Global for UpdatePanelHandle {}

/// Once, after the window opens: builds the About dialog's update entity and
/// starts the background startup check (REQ-07). Called once from `app.rs`.
pub(super) fn install(cx: &mut Context<SubspaceApp>) {
    if cx.has_global::<UpdatePanelHandle>() {
        return;
    }
    let context = cx.global::<subspace_dashboard::CurrentProduct>().0.clone();
    let updater = Updater::for_product(&context.product, &context.version);
    let panel = cx.new(|_| UpdatePanel::new(updater, context.product.display_name.as_str()));
    check_at_startup(&panel, cx);
    cx.set_global(UpdatePanelHandle(panel));
}

/// The About dialog's update entity, when one has been installed and has an
/// updater; `None` hides the update section entirely.
fn panel(cx: &App) -> Option<Entity<UpdatePanel>> {
    let handle = cx.try_global::<UpdatePanelHandle>()?;
    handle.0.read(cx).has_updater().then(|| handle.0.clone())
}

pub(super) fn about_updates(
    restart: impl Fn(&mut Window, &mut App) + 'static,
    cx: &mut Context<SubspaceApp>,
) -> Option<Entity<view::AboutUpdates>> {
    let panel = panel(cx)?;
    Some(cx.new(|cx| view::AboutUpdates::new(panel, restart, cx)))
}

/// Shared read-only projection for the status bar and toolbar. It never checks
/// the network or creates another updater.
pub(crate) fn available_update_version(cx: &App) -> Option<SharedString> {
    panel(cx).and_then(|panel| {
        panel
            .read(cx)
            .state
            .install_target()
            .map(|release| release.version.clone().into())
    })
}

#[cfg(test)]
pub(crate) use runtime_tests::set_available_for_test;

/// Every shell window observes the same updater, including windows opened after
/// the once-per-app startup check has already begun.
pub(super) fn observe(cx: &mut Context<SubspaceApp>) -> Option<Subscription> {
    let panel = panel(cx)?;
    Some(cx.observe(&panel, |_, _, cx| cx.notify()))
}

fn check_at_startup(panel: &Entity<UpdatePanel>, cx: &mut Context<SubspaceApp>) {
    let enabled = crate::settings::check_updates_enabled(
        prefs::get(cx, crate::settings::CHECK_UPDATES_KEY).as_deref(),
    );
    run_startup_check(panel.clone(), cx.weak_entity(), enabled, cx);
}
