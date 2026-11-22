//! The request status lines a conversation shows, and the timer that
//! redraws it once a second while one of them runs, so its clock ticks. The
//! timer stops as soon as no line runs, such as when every request settled
//! or the network went down.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{AsyncApp, Context, Task, WeakEntity};

use super::Conversation;
use crate::thread_status::{Shown, Statuses};

/// How often a running request's clock redraws.
const TICK: Duration = Duration::from_secs(1);

/// The status lines from the last sync and the timer while one runs.
#[derive(Default)]
pub(super) struct Lines {
    pub shown: Rc<Statuses>,
    ticker: Option<Task<()>>,
}

impl Lines {
    /// Whether a line is running and current, so its clock ticks.
    fn running(&self) -> bool {
        self.shown.values().any(Shown::ticking)
    }

    /// Drops the timer once no line runs. True when one runs and no timer
    /// does yet, so the caller starts one.
    fn needs_timer(&mut self) -> bool {
        let running = self.running();
        if !running {
            self.ticker = None;
        }
        running && self.ticker.is_none()
    }

    /// Keeps `ticker` as the timer.
    fn start(&mut self, ticker: Task<()>) {
        self.ticker = Some(ticker);
    }
}

/// Keeps `conversation`'s timer running while a status line runs, and only
/// then.
pub(super) fn follow(conversation: &mut Conversation, cx: &mut Context<Conversation>) {
    if conversation.statuses.needs_timer() {
        let ticker = cx.spawn(async |this, cx| tick(this, cx).await);
        conversation.statuses.start(ticker);
    }
}

/// Whether `conversation`'s timer runs, for tests.
#[cfg(test)]
pub(crate) fn ticking(conversation: &Conversation) -> bool {
    conversation.statuses.ticker.is_some()
}

/// Redraws `conversation` once a second while a line runs, until it goes.
async fn tick(conversation: WeakEntity<Conversation>, cx: &mut AsyncApp) {
    loop {
        cx.background_executor().timer(TICK).await;
        let redrawn = conversation.update(cx, |this, cx| {
            if this.statuses.running() {
                cx.notify();
            }
        });
        if redrawn.is_err() {
            break;
        }
    }
}
