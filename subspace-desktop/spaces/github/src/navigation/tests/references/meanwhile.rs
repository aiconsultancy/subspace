//! A read that starts while a reference read's request is in flight.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use gpui_kit::{Entity, VisualTestContext};
use subspace_framework::client_test_support::Mock;

use crate::Github;

type Ticker = Arc<OnceLock<Box<dyn Fn() -> u64 + Send + Sync>>>;

/// A mock serving `body` to every request, while each is in flight another
/// read starts on the board's clock once [`Meanwhile::arm`] has run: the
/// tick a read takes when it starts between a reference read's request
/// starting and landing.
pub(super) struct Meanwhile {
    pub(super) mock: Mock,
    ticker: Ticker,
    started: Arc<AtomicU64>,
}

impl Meanwhile {
    pub(super) fn new(body: fn() -> String) -> Self {
        let ticker = Ticker::default();
        let started = Arc::new(AtomicU64::new(0));
        let (clock, tick) = (ticker.clone(), started.clone());
        let mock = Mock::new(move |_| {
            if let Some(clock) = clock.get() {
                tick.store(clock(), Ordering::SeqCst);
            }
            (200, body())
        });
        Self {
            mock,
            ticker,
            started,
        }
    }

    /// Ties the reads meanwhile to `board`'s clock.
    pub(super) fn arm(&self, board: &Entity<Github>, cx: &mut VisualTestContext) {
        let clock = board.update(cx, |board, _| crate::references::clock(board));
        let tick = move || clock.tick();
        assert!(self.ticker.set(Box::new(tick)).is_ok(), "armed once");
    }

    /// The tick of the latest read started meanwhile.
    pub(super) fn started(&self) -> u64 {
        self.started.load(Ordering::SeqCst)
    }
}
