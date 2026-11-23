//! Reading an unreadable token again when the person asks: at most once
//! per fleet every [`REREAD_INTERVAL`], a Retry asked sooner kept until
//! its interval ends, and one that a read started after it has answered
//! dropped. The interval runs from when a read actually starts.

use std::time::Instant;

use gpui_kit::Context;

use super::{Deferred, FleetApi, REREAD_INTERVAL};
use crate::directory::{self, TokenLoad};

impl FleetApi {
    /// Reads again each unreadable token not read within the interval. A
    /// fleet whose read is still in flight keeps this Retry waiting rather
    /// than losing it: it is read once the interval ends, if that read did
    /// not settle it.
    pub(super) fn read_due(&mut self, now: Instant, cx: &mut Context<Self>) {
        for fleet in
            directory::due_rereads(&self.tokens, &mut self.reads.last, now, REREAD_INTERVAL)
        {
            if self.reads.rereading.contains(&fleet) {
                self.defer_reread(fleet, now, now + REREAD_INTERVAL, cx);
            } else {
                self.reread(fleet, cx);
            }
        }
    }

    /// A read of `fleet`'s token starts at `now`: its interval runs from
    /// here, not from when it was asked for or scheduled, so a read that
    /// waited behind others, or a Retry whose timer ran late, still leaves
    /// the keychain alone for the whole interval after it.
    pub(super) fn read_starts(&mut self, fleet: &str, now: Instant) {
        if let Some(last) = self.reads.last.get_mut(fleet) {
            *last = (*last).max(now);
        }
        self.reads.started.insert(fleet.to_owned(), now);
    }

    /// Queues a re-read of `fleet`'s token unless one is queued and not
    /// landed, so one click never reads the keychain twice in a row.
    pub(super) fn reread(&mut self, fleet: String, cx: &mut Context<Self>) {
        if self.reads.rereading.insert(fleet.clone()) {
            self.load_token(fleet, cx);
        }
    }

    /// A Retry asked at `now` within `fleet`'s interval: reads it once at
    /// `at`, when the interval ends, rather than doing nothing. One waiting
    /// read per fleet however often the person asks meanwhile; none once
    /// the fleet was saved, cleared or removed since; and a keychain that
    /// stays locked is not read on a timer.
    pub(super) fn defer_reread(
        &mut self,
        fleet: String,
        now: Instant,
        at: Instant,
        cx: &mut Context<Self>,
    ) {
        if self.reads.deferred.contains_key(&fleet) {
            return;
        }
        self.reads.deferrals = self.reads.deferrals.wrapping_add(1);
        let asked = Deferred {
            generation: self.generation_of(&fleet),
            id: self.reads.deferrals,
            asked: now,
        };
        self.wait_for_interval(fleet, asked, now, at, cx);
    }

    /// Keeps `fleet`'s waiting Retry `asked` until `at`.
    pub(super) fn wait_for_interval(
        &mut self,
        fleet: String,
        asked: Deferred,
        now: Instant,
        at: Instant,
        cx: &mut Context<Self>,
    ) {
        self.reads.deferred.insert(fleet.clone(), asked);
        let wait = at.saturating_duration_since(now);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            _ = this.update(cx, |this, cx| this.deferred_due(&fleet, asked, at, cx));
        })
        .detach();
    }

    /// `fleet`'s waiting Retry `asked`, at `at`: still the fleet's waiting
    /// Retry and nothing changed it, it goes on to [`Self::retry_due`].
    pub(super) fn deferred_due(
        &mut self,
        fleet: &str,
        asked: Deferred,
        at: Instant,
        cx: &mut Context<Self>,
    ) {
        if self.reads.deferred.get(fleet) != Some(&asked) {
            return;
        }
        self.reads.deferred.remove(fleet);
        if asked.generation == self.generation_of(fleet) {
            self.retry_due(fleet, asked, at, cx);
        }
    }

    /// What `fleet`'s waiting Retry `asked` does at `now`: nothing once a
    /// read that started after it was asked has answered it; wait for a
    /// read still in flight to land; wait again while the interval from
    /// the last read's actual start runs; otherwise read, if the token is
    /// still unreadable. So a read never starts within the interval of
    /// another, however late either started.
    pub(super) fn retry_due(
        &mut self,
        fleet: &str,
        asked: Deferred,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        let answered = self
            .reads
            .started
            .get(fleet)
            .is_some_and(|started| *started >= asked.asked);
        if answered || self.tokens.get(fleet) != Some(&TokenLoad::Unavailable) {
            return;
        }
        if self.reads.rereading.contains(fleet) {
            self.reads.after_read.insert(fleet.to_owned(), asked);
            return;
        }
        let due = self
            .reads
            .last
            .get(fleet)
            .map_or(now, |last| *last + REREAD_INTERVAL);
        if now >= due {
            self.reads.last.insert(fleet.to_owned(), now);
            self.reread(fleet.to_owned(), cx);
        } else {
            self.wait_for_interval(fleet.to_owned(), asked, now, due, cx);
        }
    }

    /// A read of `fleet`'s token landed: another may be queued now, and a
    /// Retry that waited behind it goes on ([`Self::retry_due`]).
    pub(super) fn read_landed(&mut self, fleet: &str, cx: &mut Context<Self>) {
        self.reads.rereading.remove(fleet);
        if let Some(asked) = self.reads.after_read.remove(fleet) {
            let now = cx.background_executor().now();
            self.retry_due(fleet, asked, now, cx);
        }
    }
}
