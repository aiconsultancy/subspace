use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::{Client, EventQueue, lock};
use super::fence::Activity;
use super::{Activities, Timing, Watch, spawn};
use crate::failure::TurnContext;
use crate::{ChatEvent, Event, FailureCategory};

fn activity(busy: bool, last_event: Instant) -> Activity {
    Activity {
        busy,
        last_event,
        turn: TurnContext {
            attachments: 1,
            ..TurnContext::default()
        },
        generation: 1,
        expired: None,
    }
}

/// The runtime takes the ping and never answers, as a hung process does.
/// The watchdog task reports the busy chat on its own, with no actor.
#[tokio::test]
async fn a_runtime_that_never_answers_the_ping_ends_the_busy_turn() {
    let directory = tempfile::tempdir().unwrap();
    let (local, remote) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(local);
    let client = Client::from_streams(read, write, directory.path().into()).unwrap();
    let activities: Activities = Default::default();
    lock(&activities).insert(3, activity(true, Instant::now()));
    lock(&activities).insert(5, activity(false, Instant::now()));
    let events = Arc::new(EventQueue::default());
    let watchdog = spawn(Watch {
        client: client.clone(),
        activities: activities.clone(),
        events: events.clone(),
        runtime_version: Some("1.0.105".into()),
        timing: Timing {
            stall_after: Duration::from_millis(20),
            ping_timeout: Duration::from_millis(50),
            check_every: Duration::from_millis(10),
        },
    });
    let reported = tokio::time::timeout(Duration::from_secs(5), async {
        while lock(&events.items).len() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(reported.is_ok(), "the watchdog reported the stalled turn");
    let queued: Vec<Event> = lock(&events.items).drain(..).collect();
    assert!(
        matches!(
            queued.as_slice(),
            [
                Event::Chat { chat: 3, event: ChatEvent::Failure(failure) },
                Event::Chat { chat: 3, event: ChatEvent::TurnDone },
            ] if failure.category == FailureCategory::NotResponding
                && failure.runtime_version.as_deref() == Some("1.0.105")
                && failure.attachments == 1
        ),
        "{queued:?}"
    );
    assert!(!lock(&activities)[&3].busy);
    drop(watchdog);
    drop(remote);
    client.force_stop();
}
