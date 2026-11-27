//! Scheduling the blocking diff-list read outside the UI executor.

use gpui_kit::{App, AppContext as _, Global, Task};
use subspace_client_fleet::{
    Error, FleetClient,
    diff::{DiffMode, DiffRead},
};

use super::source::{DiffSource, Loaded};

pub(super) enum LoadError {
    Message(String),
    Fleet(Error),
}

pub(super) type ReadResult = Result<Loaded, LoadError>;
pub(super) type ReadWork = Box<dyn FnOnce() -> ReadResult + Send>;

/// The diff-list worker. Tests can supply a runner that leaves the UI scheduler
/// free while an HTTP fixture holds the response.
pub(super) struct ReadExecutor(pub(super) fn(&mut App, ReadWork) -> Task<ReadResult>);

impl Global for ReadExecutor {}

impl ReadExecutor {
    pub(super) fn start(cx: &mut App, work: ReadWork) -> Task<ReadResult> {
        if let Some(&Self(spawn)) = cx.try_global() {
            spawn(cx, work)
        } else {
            cx.background_spawn(async move { work() })
        }
    }
}

pub(super) fn read_diff(
    source: DiffSource,
    mode: DiffMode,
    fleet_client: Option<Result<FleetClient, String>>,
    read: &DiffRead,
) -> ReadResult {
    match source {
        DiffSource::GitHub(source) => source
            .client
            .ok_or_else(|| LoadError::Message("GitHub is not configured.".to_owned()))?
            .diff(&source.login, &source.item.key)
            .map(Loaded::GitHub)
            .map_err(|error| LoadError::Message(error.to_string())),
        DiffSource::Fleet(source) => source
            .load(
                &fleet_client
                    .ok_or_else(|| {
                        LoadError::Message("Fleet connections are unavailable.".to_owned())
                    })?
                    .map_err(LoadError::Message)?,
                mode,
                read,
            )
            .map_err(LoadError::Fleet),
    }
}

pub(super) fn diff_error(error: Error) -> String {
    match error {
        Error::Busy(Some(1)) => "Stargate is busy. Wait 1 second, then select Refresh.".to_owned(),
        Error::Busy(Some(seconds)) => {
            format!("Stargate is busy. Wait {seconds} seconds, then select Refresh.")
        }
        Error::Busy(None) => "Stargate is busy. Select Refresh to try again shortly.".to_owned(),
        other => other.to_string(),
    }
}
