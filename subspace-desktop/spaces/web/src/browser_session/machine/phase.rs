//! Where a session stands, and what an attempt builds on.

use std::time::Duration;

use super::super::grant::{Deadline, MIN_SESSION_LIFETIME, Now, SessionGrant};

/// Renew this long before the session ends, or at half its life if shorter.
const RENEW_SKEW: Duration = Duration::from_secs(MIN_SESSION_LIFETIME.as_secs() / 2);
const RENEW_RETRY: Duration = Duration::from_secs(30);
const MAX_RENEW_RETRIES: u8 = 3;

#[derive(Default)]
pub(super) enum Phase {
    #[default]
    SignedOut,
    /// Signed in; the first attempt waits until the Space is shown.
    Ready,
    Acquiring {
        attempt: u64,
        base: Base,
    },
    Installing {
        attempt: u64,
        grant: SessionGrant,
        base: Base,
    },
    Live(Live),
    /// Retry starts again, at `resume`.
    Ended {
        message: String,
        resume: Option<String>,
    },
}

/// What an attempt builds on: a new view, or the live session it renews,
/// which keeps working meanwhile.
pub(super) enum Base {
    Fresh { resume: Option<String> },
    Renewal { live: Live, signalled: bool },
}

pub(super) struct Live {
    pub(super) grant: SessionGrant,
    /// When to renew; `None` once renewing failed for good.
    pub(super) renew: Option<Deadline>,
    retries: u8,
    /// Until when another signal means the renewal did not help.
    pub(super) returned: Option<Deadline>,
    /// The site's signal came while renewing failed: the next renewal
    /// returns to the page once it succeeds.
    signal_waiting: bool,
}

impl Base {
    /// The site's signal arrived while this attempt was under way.
    pub(super) fn signalled(self) -> Self {
        match self {
            Self::Renewal { live, .. } => Self::Renewal {
                live,
                signalled: true,
            },
            fresh => fresh,
        }
    }

    pub(super) fn live(&self) -> Option<&Live> {
        match self {
            Self::Renewal { live, .. } => Some(live),
            Self::Fresh { .. } => None,
        }
    }
}

impl Live {
    pub(super) fn new(grant: SessionGrant, returned: Option<Deadline>, now: Now) -> Self {
        let skew = RENEW_SKEW.min(grant.ends.left(now) / 2);
        Self {
            renew: Some(grant.ends.before(skew, now)),
            grant,
            retries: 0,
            returned,
            signal_waiting: false,
        }
    }

    /// The renewal of this session, `signalled` when the site's signal
    /// asks for it now, or asked while renewing failed.
    pub(super) fn renewal(self, signalled: bool) -> Base {
        let signalled = signalled || self.signal_waiting;
        Base::Renewal {
            live: self,
            signalled,
        }
    }

    /// The site's signal came while a failed renewal waits to try again:
    /// the signal waits with it.
    pub(super) fn signal_waits(mut self) -> Self {
        self.signal_waiting = true;
        self
    }

    /// A failed renewal is waiting to try again.
    pub(super) fn retrying(&self) -> bool {
        self.retries > 0
    }

    /// A renewal failed: try again soon if it may pass, else not at all.
    /// A signal that asked for it waits for the renewal that succeeds.
    pub(super) fn retry_later(mut self, transient: bool, signalled: bool, now: Now) -> Self {
        let again = transient && self.retries < MAX_RENEW_RETRIES;
        self.renew = again.then(|| now.after(RENEW_RETRY));
        self.retries += u8::from(again);
        self.signal_waiting |= signalled;
        self
    }
}

impl Phase {
    /// The live session an attempt in flight renews.
    pub(super) fn renewing(&self) -> Option<&Live> {
        match self {
            Self::Live(live) => Some(live),
            Self::Acquiring { base, .. } | Self::Installing { base, .. } => base.live(),
            _ => None,
        }
    }
}
