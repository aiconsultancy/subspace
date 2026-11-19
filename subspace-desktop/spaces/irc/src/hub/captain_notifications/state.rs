//! Track the active poll and remember acknowledged IRC writes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use gpui_kit::Task;
use subspace_client_irc::SendResult;
use subspace_framework::dashboard::refresh::Schedule;

use super::{LOOKBACK, Scope, notice::NoticeKey};

pub(super) struct Active {
    pub(super) scope: Scope,
    pub(super) since: DateTime<Utc>,
    pub(super) started: Instant,
    pub(super) schedule: Schedule,
    pub(super) fetch: Option<Task<()>>,
}

#[derive(Default)]
pub(in crate::hub) struct Monitor {
    pub(super) active: Option<Active>,
    pub(super) last_check: Option<Instant>,
    pub(super) deliveries: HashMap<NoticeKey, Delivery>,
}

pub(super) struct Delivery {
    pub(super) time: DateTime<Utc>,
    pub(super) pending: Option<u64>,
}

impl Monitor {
    pub(in crate::hub) fn pause(&mut self) {
        self.active = None;
    }

    pub(super) fn sync(&mut self, scope: Option<Scope>, now: DateTime<Utc>) {
        self.deliveries
            .retain(|_, delivery| delivery.time > now - LOOKBACK);
        if self.active.as_ref().map(|active| &active.scope) == scope.as_ref() {
            return;
        }
        self.active = scope.map(|scope| Active {
            scope,
            since: now,
            started: Instant::now(),
            schedule: Schedule::new(Duration::ZERO),
            fetch: None,
        });
    }

    pub(in crate::hub) fn answered(&mut self, id: u64, result: &SendResult) -> bool {
        let mut matched = false;
        self.deliveries.retain(|_, delivery| {
            if delivery.pending != Some(id) {
                return true;
            }
            matched = true;
            delivery.pending = None;
            // Retry at the next poll only when the runtime wrote nothing.
            !matches!(result, SendResult::Refused(_))
        });
        matched
    }
}
