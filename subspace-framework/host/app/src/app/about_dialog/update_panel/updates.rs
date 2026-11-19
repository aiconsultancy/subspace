//! Pure update-check state, transitions and display text (REQ-06, REQ-07).
//! Kept free of GPUI so the state machine is directly unit tested.
//!
//! `subspace_update::Release` has a private field (its asset map), so it
//! cannot be built outside that crate; this module keeps only the two public
//! fields the UI shows. [`super::update_panel`] holds the real `Release`
//! alongside this state to call `install` with.

use super::error_details::UpdateFailure;
use subspace_update::{CheckOutcome, Installed, UpdateProgress};

const BYTES_PER_MIB: f64 = 1024. * 1024.;
const PERCENT: f64 = 100.;

/// The two `Release` fields the UI shows, without the private asset map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReleaseInfo {
    pub(super) version: String,
    pub(super) html_url: String,
}

impl From<&subspace_update::Release> for ReleaseInfo {
    fn from(release: &subspace_update::Release) -> Self {
        Self {
            version: release.version.clone(),
            html_url: release.html_url.clone(),
        }
    }
}

/// What a finished check found, keeping only what the UI shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Found {
    Newer(ReleaseInfo),
    UpToDate,
    /// Nothing is published in `repository` for this build's channel yet.
    NoRelease {
        repository: String,
    },
}

impl From<&CheckOutcome> for Found {
    fn from(outcome: &CheckOutcome) -> Self {
        match outcome {
            CheckOutcome::Newer(release) => Self::Newer(ReleaseInfo::from(release)),
            CheckOutcome::UpToDate => Self::UpToDate,
            CheckOutcome::NoRelease { repository } => Self::NoRelease {
                repository: repository.clone(),
            },
        }
    }
}

/// The About dialog's update section, and the one state the startup check
/// can also produce.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A calm result, not a failure: the update source has no release yet.
    NoRelease {
        repository: String,
    },
    Available(ReleaseInfo),
    Installing(ReleaseInfo),
    Installed(Installed),
    Failed(String),
}

/// What changes an [`UpdateState`]. A stale result (for example a check
/// result arriving while installing) is ignored.
#[derive(Debug, Clone)]
pub(super) enum UpdateEvent {
    CheckRequested,
    CheckResult(Result<Found, UpdateFailure>),
    InstallRequested,
    InstallResult(Result<Installed, UpdateFailure>),
}

impl UpdateState {
    pub(super) fn apply(self, event: UpdateEvent) -> Self {
        match event {
            UpdateEvent::CheckRequested if self.busy() => self,
            UpdateEvent::CheckRequested => Self::Checking,
            UpdateEvent::CheckResult(result) => self.apply_check_result(result),
            UpdateEvent::InstallRequested => self.apply_install_requested(),
            UpdateEvent::InstallResult(result) => self.apply_install_result(result),
        }
    }

    pub(super) fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing(_))
    }

    /// Only a result for the in-flight check applies; any other result is stale.
    fn apply_check_result(self, result: Result<Found, UpdateFailure>) -> Self {
        if !matches!(self, Self::Checking) {
            return self;
        }
        match result {
            Ok(Found::Newer(release)) => Self::Available(release),
            Ok(Found::UpToDate) => Self::UpToDate,
            Ok(Found::NoRelease { repository }) => Self::NoRelease { repository },
            Err(error) => Self::Failed(error.message),
        }
    }

    /// Install can only start from an available release.
    fn apply_install_requested(self) -> Self {
        match self {
            Self::Available(release) => Self::Installing(release),
            other => other,
        }
    }

    /// Only a result for the in-flight install applies; any other result is stale.
    fn apply_install_result(self, result: Result<Installed, UpdateFailure>) -> Self {
        if !matches!(self, Self::Installing(_)) {
            return self;
        }
        match result {
            Ok(installed) => Self::Installed(installed),
            Err(error) => Self::Failed(error.message),
        }
    }

    pub(super) fn install_target(&self) -> Option<&ReleaseInfo> {
        match self {
            Self::Available(release) => Some(release),
            _ => None,
        }
    }
}

/// What the update button does in each state: check, install the release
/// found, or quit and restart into the installed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UpdateAction {
    Check,
    Install,
    Restart,
}

impl UpdateAction {
    pub(super) fn of(state: &UpdateState) -> Self {
        match state {
            UpdateState::Available(_) => Self::Install,
            UpdateState::Installed(_) => Self::Restart,
            _ => Self::Check,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Check => "Check for updates",
            Self::Install => "Install update",
            Self::Restart => "Quit and restart",
        }
    }
}

/// Whether the startup check (REQ-07) should run at all.
pub(super) fn should_check_at_startup(has_updater: bool, setting_enabled: bool) -> bool {
    has_updater && setting_enabled
}

/// A newer release found at startup, to push a toast for (REQ-07). Up to
/// date, no release yet and `Err` show no toast; About still shows the result.
pub(super) fn startup_release(result: &Result<Found, UpdateFailure>) -> Option<&ReleaseInfo> {
    match result {
        Ok(Found::Newer(release)) => Some(release),
        Ok(Found::UpToDate | Found::NoRelease { .. }) | Err(_) => None,
    }
}

/// The About dialog's status line; `None` before the first check (REQ-06).
pub(super) fn status_text(
    state: &UpdateState,
    app_name: &str,
    current_version: &str,
) -> Option<String> {
    match state {
        UpdateState::Idle => None,
        UpdateState::Checking => Some("Checking for updates…".into()),
        UpdateState::UpToDate => Some(format!("{app_name} v{current_version} is up to date")),
        UpdateState::NoRelease { repository } => Some(format!(
            "No {app_name} releases have been published to {repository} yet"
        )),
        UpdateState::Available(release) => {
            Some(format!("Version {} is available", release.version))
        }
        UpdateState::Installing(_) => Some("Preparing update…".into()),
        UpdateState::Installed(installed) => Some(format!(
            "Installed v{}. Quit and restart {app_name} to use it.",
            installed.version
        )),
        UpdateState::Failed(error) => Some(error.clone()),
    }
}

pub(super) fn progress_text(progress: &UpdateProgress) -> String {
    match progress {
        UpdateProgress::Preparing => "Preparing update…".into(),
        UpdateProgress::Downloading {
            received,
            total: Some(total),
        } if *total > 0 => format!(
            "Downloading update… {:.1} of {:.1} MiB",
            *received as f64 / BYTES_PER_MIB,
            *total as f64 / BYTES_PER_MIB
        ),
        UpdateProgress::Downloading { received, .. } => format!(
            "Downloading update… {:.1} MiB",
            *received as f64 / BYTES_PER_MIB
        ),
        UpdateProgress::DownloadingChecksum => "Downloading checksum…".into(),
        UpdateProgress::VerifyingChecksum => "Verifying download checksum…".into(),
        UpdateProgress::Extracting => "Extracting update…".into(),
        UpdateProgress::VerifyingSignature => "Verifying app signature…".into(),
        UpdateProgress::Installing => "Replacing installed app…".into(),
    }
}

pub(super) fn download_percentage(progress: &UpdateProgress) -> Option<f32> {
    match progress {
        UpdateProgress::Downloading {
            received,
            total: Some(total),
        } if *total > 0 => Some((*received as f64 / *total as f64 * PERCENT).min(PERCENT) as f32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Found, ReleaseInfo, UpdateEvent, UpdateState, should_check_at_startup, startup_release,
        status_text,
    };
    use subspace_update::{CheckOutcome, Installed};

    #[test]
    fn download_percentages_are_only_shown_for_known_positive_totals() {
        use subspace_update::UpdateProgress;
        assert_eq!(
            super::download_percentage(&UpdateProgress::Downloading {
                received: 5,
                total: Some(10)
            }),
            Some(50.)
        );
        assert_eq!(
            super::download_percentage(&UpdateProgress::Downloading {
                received: 5,
                total: None
            }),
            None
        );
        assert_eq!(
            super::download_percentage(&UpdateProgress::Downloading {
                received: 5,
                total: Some(0)
            }),
            None
        );
        assert_eq!(
            super::download_percentage(&UpdateProgress::VerifyingChecksum),
            None
        );
        assert_eq!(
            super::progress_text(&UpdateProgress::Downloading {
                received: 1_048_576,
                total: None
            }),
            "Downloading update… 1.0 MiB"
        );
    }

    fn release(version: &str) -> ReleaseInfo {
        ReleaseInfo {
            version: version.to_owned(),
            html_url: format!("https://example.test/{version}"),
        }
    }

    fn no_release() -> Found {
        Found::NoRelease {
            repository: "owner/name".into(),
        }
    }

    #[test]
    fn a_check_moves_through_checking_to_its_result() {
        let state = UpdateState::Idle.apply(UpdateEvent::CheckRequested);
        assert_eq!(state, UpdateState::Checking);
        assert_eq!(
            state
                .clone()
                .apply(UpdateEvent::CheckResult(Ok(Found::UpToDate))),
            UpdateState::UpToDate
        );
        assert_eq!(
            state
                .clone()
                .apply(UpdateEvent::CheckResult(Ok(Found::Newer(release("1.2.0"))))),
            UpdateState::Available(release("1.2.0"))
        );
        assert_eq!(
            state
                .clone()
                .apply(UpdateEvent::CheckResult(Ok(no_release()))),
            UpdateState::NoRelease {
                repository: "owner/name".into()
            }
        );
        let failed = state.apply(UpdateEvent::CheckResult(Err("offline".into())));
        assert_eq!(failed, UpdateState::Failed("offline".into()));
    }

    #[test]
    fn the_updater_outcome_maps_to_what_the_panel_shows() {
        assert_eq!(Found::from(&CheckOutcome::UpToDate), Found::UpToDate);
        assert_eq!(
            Found::from(&CheckOutcome::NoRelease {
                repository: "owner/name".into()
            }),
            no_release()
        );
    }

    #[test]
    fn install_only_starts_from_an_available_release() {
        let idle = UpdateState::Idle.apply(UpdateEvent::InstallRequested);
        assert_eq!(idle, UpdateState::Idle);
        let available = UpdateState::Available(release("1.2.0"));
        let installing = available.apply(UpdateEvent::InstallRequested);
        assert_eq!(installing, UpdateState::Installing(release("1.2.0")));
    }

    #[test]
    fn stale_results_are_ignored() {
        let installing = UpdateState::Installing(release("1.2.0"));
        // A check result arrives while installing: ignored.
        let unchanged = installing
            .clone()
            .apply(UpdateEvent::CheckResult(Ok(Found::UpToDate)));
        assert_eq!(unchanged, installing);
        // An install result arrives while idle (already handled): ignored.
        let idle = UpdateState::Idle;
        let still_idle = idle.apply(UpdateEvent::InstallResult(Ok(Installed {
            previous_version: "1.0.0".into(),
            version: "1.2.0".into(),
            path: "/bin/app".into(),
        })));
        assert_eq!(still_idle, UpdateState::Idle);
    }

    #[test]
    fn a_check_request_cannot_interrupt_an_install() {
        let installing = UpdateState::Installing(release("1.2.0"));
        assert_eq!(
            installing.clone().apply(UpdateEvent::CheckRequested),
            installing
        );
    }

    #[test]
    fn a_completed_install_reports_its_versions() {
        let installing = UpdateState::Installing(release("1.2.0"));
        let installed = installing.apply(UpdateEvent::InstallResult(Ok(Installed {
            previous_version: "1.0.0".into(),
            version: "1.2.0".into(),
            path: "/bin/app".into(),
        })));
        assert_eq!(
            installed,
            UpdateState::Installed(Installed {
                previous_version: "1.0.0".into(),
                version: "1.2.0".into(),
                path: "/bin/app".into(),
            })
        );
    }

    #[test]
    fn the_button_checks_installs_or_restarts_by_state() {
        use super::UpdateAction;
        assert_eq!(UpdateAction::of(&UpdateState::Idle), UpdateAction::Check);
        assert_eq!(
            UpdateAction::of(&UpdateState::UpToDate),
            UpdateAction::Check
        );
        assert_eq!(
            UpdateAction::of(&UpdateState::Available(release("1.2.0"))),
            UpdateAction::Install
        );
        let installed = UpdateState::Installed(Installed {
            previous_version: "1.0.0".into(),
            version: "1.2.0".into(),
            path: "/bin/app".into(),
        });
        assert_eq!(UpdateAction::of(&installed), UpdateAction::Restart);
        assert_eq!(UpdateAction::Restart.label(), "Quit and restart");
    }

    #[test]
    fn the_startup_check_only_runs_with_an_updater_and_the_setting_on() {
        assert!(should_check_at_startup(true, true));
        assert!(!should_check_at_startup(false, true));
        assert!(!should_check_at_startup(true, false));
        assert!(!should_check_at_startup(false, false));
    }

    #[test]
    fn startup_only_surfaces_a_newer_release() {
        assert_eq!(
            startup_release(&Ok(Found::Newer(release("1.2.0")))),
            Some(&release("1.2.0"))
        );
        assert_eq!(startup_release(&Ok(Found::UpToDate)), None);
        assert_eq!(startup_release(&Ok(no_release())), None);
        assert_eq!(startup_release(&Err("offline".into())), None);
    }

    #[test]
    fn status_text_names_the_app_and_version_for_each_state() {
        assert_eq!(status_text(&UpdateState::Idle, "Subspace", "1.0.0"), None);
        assert_eq!(
            status_text(&UpdateState::Checking, "Subspace", "1.0.0").as_deref(),
            Some("Checking for updates…")
        );
        assert_eq!(
            status_text(&UpdateState::UpToDate, "Subspace", "1.0.0").as_deref(),
            Some("Subspace v1.0.0 is up to date")
        );
        assert_eq!(
            status_text(
                &UpdateState::NoRelease {
                    repository: "owner/name".into()
                },
                "Subspace",
                "1.0.0"
            )
            .as_deref(),
            Some("No Subspace releases have been published to owner/name yet")
        );
        assert_eq!(
            status_text(
                &UpdateState::Available(release("1.2.0")),
                "Subspace",
                "1.0.0"
            )
            .as_deref(),
            Some("Version 1.2.0 is available")
        );
        assert_eq!(
            status_text(
                &UpdateState::Installing(release("1.2.0")),
                "Subspace",
                "1.0.0"
            )
            .as_deref(),
            Some("Preparing update…")
        );
        let installed = UpdateState::Installed(Installed {
            previous_version: "1.0.0".into(),
            version: "1.2.0".into(),
            path: "/bin/app".into(),
        });
        let text = status_text(&installed, "Subspace", "1.0.0").unwrap();
        assert_eq!(
            text,
            "Installed v1.2.0. Quit and restart Subspace to use it."
        );
        assert_eq!(
            status_text(&UpdateState::Failed("offline".into()), "Subspace", "1.0.0").as_deref(),
            Some("offline")
        );
    }
}
