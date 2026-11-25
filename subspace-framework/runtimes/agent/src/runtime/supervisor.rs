//! Stops a runtime that does not stop by itself. The handle holds a [`Stop`];
//! when the last handle is dropped, as on Reconnect or quit, the runtime gets
//! [`SHUTDOWN_GRACE`] to finish its shutdown command, then its task is
//! aborted. That ends an actor that waits on a call the Copilot runtime never
//! answers, and with it the watchdog and the client.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::oneshot;

/// How long a stopped runtime may take to shut down cleanly.
pub(super) const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// What a runtime whose task ended abnormally reports.
const ENDED: &str = "The chat runtime stopped unexpectedly. Click Reconnect to start it again.";

/// The handle's side: dropping it asks the runtime to stop.
pub(super) type Stop = oneshot::Sender<()>;
/// The runtime's side.
pub(super) type Stopped = oneshot::Receiver<()>;
/// A runtime's work, as one task.
pub(super) type Work = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

pub(super) fn stop_pair() -> (Stop, Stopped) {
    oneshot::channel()
}

/// Runs `work` until it ends, or until the handle is dropped and `grace`
/// has passed, when the work is aborted. A stopped runtime reports nothing.
pub(super) async fn supervised(
    work: Work,
    stopped: Stopped,
    grace: Duration,
) -> Result<(), String> {
    let mut task = tokio::spawn(work);
    tokio::select! {
        result = &mut task => result.unwrap_or_else(|_| Err(ENDED.to_owned())),
        _ = stopped => {
            let finished = tokio::time::timeout(grace, &mut task).await;
            task.abort();
            finished.ok().and_then(Result::ok).unwrap_or(Ok(()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Work, stop_pair, supervised};
    #[cfg(feature = "copilot")]
    use crate::runtime::Client;

    /// The actor waits on a call to a runtime that never answers. Dropping
    /// the handle's stop still ends it once the grace has passed.
    #[cfg(feature = "copilot")]
    #[tokio::test]
    async fn dropping_the_handle_ends_an_actor_stuck_on_a_call() {
        let directory = tempfile::tempdir().unwrap();
        let (local, _remote) = tokio::io::duplex(64 * 1024);
        let (read, write) = tokio::io::split(local);
        let client = Client::from_streams(read, write, directory.path().into()).unwrap();
        let hung: Work = Box::pin(async move {
            let _ = client.ping(None).await;
            Err("the call returned".to_owned())
        });
        let (stop, stopped) = stop_pair();
        let supervising = tokio::spawn(supervised(hung, stopped, Duration::from_millis(20)));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!supervising.is_finished(), "the call never returns");
        drop(stop);
        let ended = tokio::time::timeout(Duration::from_secs(2), supervising).await;
        assert_eq!(
            ended.ok().and_then(Result::ok),
            Some(Ok(())),
            "the stuck actor ended without a report"
        );
    }

    #[tokio::test]
    async fn a_runtime_that_ends_by_itself_reports_its_result() {
        let (_stop, stopped) = stop_pair();
        let failing: Work = Box::pin(async { Err("startup failed".to_owned()) });
        assert_eq!(
            supervised(failing, stopped, Duration::from_millis(20)).await,
            Err("startup failed".to_owned())
        );
    }
}
