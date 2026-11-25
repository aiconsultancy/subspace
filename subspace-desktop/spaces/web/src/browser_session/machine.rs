//! The session's lifecycle as a state machine with no view and no clock of
//! its own. Each transition takes what happened, the time and where the
//! view is, and returns the effects for the board to apply, so the tests
//! drive it directly.

use std::time::Duration;

use super::SessionScope;
use super::grant::{CookieProblem, Now, SessionFailure, SessionGrant};
use crate::page::SiteCookie;

mod phase;

use phase::{Base, Live, Phase};

/// A signal this soon after the page went back for the last one ends the
/// session rather than renew again.
const SIGNAL_GUARD: Duration = Duration::from_secs(30);
pub(super) const NOT_INSTALLED: &str =
    "Cannot install the private session for this page. Try again.";
pub(super) const INVALID: &str = "The site returned a session this Space cannot use. Try again.";
const EXPIRED: &str = "The site returned an expired session. Try again.";
const MISCONFIGURED: &str =
    "This Space cannot sign in: the app's session settings are wrong. Open it in your browser.";

/// What the view's cookie store said about an install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stored {
    Confirmed,
    /// The store answered, but does not hold the cookie as installed.
    Refused,
    /// The store did not answer in time.
    TimedOut,
}

/// A transition's effects, and whether it changed what the Space shows or
/// does, so the board redraws only then.
pub(crate) struct Step {
    pub effects: Vec<Effect>,
    pub changed: bool,
}

/// Where the view is when a transition runs: a page in the scope, or `None`
/// when it has no page there. A session that ends reopens there on Retry.
pub(crate) type Here = Option<String>;

/// What the board does for the session.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// Ask the product's adapter for a grant, off the UI thread.
    Acquire(u64),
    /// Open an isolated view, blank, and install the cookie in it.
    Open(u64, SiteCookie),
    /// Install a renewed cookie in the view.
    Install(u64, SiteCookie),
    /// Show the view opened for this attempt, loading this address.
    Show(u64, String),
    /// Drop the view opened for this attempt, which never showed, with its
    /// store.
    Discard(u64),
    /// Tell the page its session was renewed.
    Renewed,
    /// Load this address again, the page the view showed, after a renewal
    /// the site's signal asked for.
    Return(String),
    /// Close the view, which drops its store.
    Close,
    /// Say this in the status bar.
    Notice(String),
    /// Record a mistake in the product's composition. No credential.
    Log(&'static str),
}

/// What the Space shows while it has no page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Waiting {
    SignIn,
    Opening,
    Ended(String),
}

pub(crate) struct Session {
    scope: SessionScope,
    /// The message when the session ends at the site or expires.
    ended: String,
    /// The account the session belongs to.
    generation: Option<u64>,
    attempts: u64,
    phase: Phase,
}

impl Session {
    pub(crate) fn new(scope: SessionScope, title: &str) -> Self {
        Self {
            scope,
            ended: format!(
                "Your {title} session has ended. Unsaved changes may be lost when you retry."
            ),
            generation: None,
            attempts: 0,
            phase: Phase::SignedOut,
        }
    }

    pub(crate) fn scope(&self) -> &SessionScope {
        &self.scope
    }

    /// Runs `transition` at `now`, with the view at `here`.
    pub(crate) fn step(
        &mut self,
        now: Now,
        here: Here,
        transition: impl FnOnce(&mut Self, Now, Here) -> Vec<Effect>,
    ) -> Step {
        let before = self.waiting();
        let effects = transition(self, now, here);
        let changed = !effects.is_empty() || self.waiting() != before;
        Step { effects, changed }
    }

    /// The account signed in now, by generation. Another account, or
    /// none, ends the session, which revokes it, and closes the view.
    pub(crate) fn account(&mut self, generation: Option<u64>) -> Vec<Effect> {
        if generation == self.generation {
            return Vec::new();
        }
        self.generation = generation;
        self.phase = match generation {
            Some(_) => Phase::Ready,
            None => Phase::SignedOut,
        };
        vec![Effect::Close]
    }

    /// Time passed. The first attempt waits until the Space is `shown`.
    pub(crate) fn tick(&mut self, now: Now, shown: bool, here: Here) -> Vec<Effect> {
        let expired = self
            .phase
            .renewing()
            .is_some_and(|live| live.grant.ends.passed(now));
        match self.take() {
            _ if expired => self.end(here),
            Phase::Ready if shown => self.acquire(Base::Fresh { resume: None }),
            Phase::Live(live) if live.renew.is_some_and(|renew| renew.passed(now)) => {
                self.acquire(live.renewal(false))
            }
            phase => self.keep(phase),
        }
    }

    /// The adapter answered `attempt`. An answer for an attempt that no
    /// longer applies is dropped, which revokes its grant.
    pub(crate) fn acquired(
        &mut self,
        attempt: u64,
        result: Result<SessionGrant, SessionFailure>,
        now: Now,
    ) -> Vec<Effect> {
        match self.take() {
            Phase::Acquiring {
                attempt: current,
                base,
            } if current == attempt => match result {
                Ok(grant) => self.install(attempt, grant, base, now),
                Err(failure) => self.failed(base, failure, now),
            },
            phase => self.keep(phase),
        }
    }

    /// The view's store answered `attempt`'s install. The grant goes live
    /// only once the store confirms its cookie. A renewal the store
    /// refused, or did not confirm in time, ends the session and closes the
    /// view: the store may hold, or still write, a cookie whose grant would
    /// otherwise be revoked while the page uses it. A new view the store
    /// failed is dropped, with its store, before it ever shows.
    pub(crate) fn installed(
        &mut self,
        attempt: u64,
        stored: Stored,
        now: Now,
        here: Here,
    ) -> Vec<Effect> {
        let (grant, base) = match self.take() {
            Phase::Installing {
                attempt: current,
                grant,
                base,
            } if current == attempt => (grant, base),
            phase => return self.keep(phase),
        };
        match (stored, base) {
            (Stored::Confirmed, base) => self.go_live(attempt, grant, base, now, here),
            (Stored::Refused | Stored::TimedOut, Base::Renewal { .. }) => {
                self.ended_with(NOT_INSTALLED.into(), here)
            }
            (_, base @ Base::Fresh { .. }) => {
                let mut effects = vec![Effect::Discard(attempt)];
                effects.extend(self.failed(base, SessionFailure::action(NOT_INSTALLED), now));
                effects
            }
        }
    }

    /// Whether `attempt` still waits for a new view to install its cookie
    /// in, so a view that opens for it now may.
    pub(crate) fn opening(&self, attempt: u64) -> bool {
        match &self.phase {
            Phase::Installing {
                attempt: current,
                base: Base::Fresh { .. },
                ..
            } => *current == attempt,
            _ => false,
        }
    }

    /// The session went live at `url` for an attempt whose view had gone:
    /// end it rather than wait on a page that never comes. Retry opens
    /// `url`.
    pub(crate) fn not_shown(&mut self, url: String) -> Vec<Effect> {
        self.ended_with(NOT_INSTALLED.into(), Some(url))
    }

    /// The site sent the view to its signal: renew now and return to the
    /// page. Another signal soon after the page went back, or after
    /// renewing failed for good, ends the session; while a failed renewal
    /// waits to try again, the signal waits with it, and the page returns
    /// once a renewal succeeds.
    pub(crate) fn signal(&mut self, now: Now, here: Here) -> Vec<Effect> {
        match self.take() {
            Phase::Live(live)
                if live.renew.is_none()
                    || live.returned.is_some_and(|until| !until.passed(now)) =>
            {
                self.end(here)
            }
            Phase::Live(live) if live.retrying() => self.keep(Phase::Live(live.signal_waits())),
            Phase::Live(live) => self.acquire(live.renewal(true)),
            Phase::Acquiring { attempt, base } => self.keep(Phase::Acquiring {
                attempt,
                base: base.signalled(),
            }),
            Phase::Installing {
                attempt,
                grant,
                base,
            } => self.keep(Phase::Installing {
                attempt,
                grant,
                base: base.signalled(),
            }),
            phase => self.keep(phase),
        }
    }

    /// Whether Reload means Retry: the session ended, or renewing it
    /// failed for good.
    pub(crate) fn needs_retry(&self) -> bool {
        match &self.phase {
            Phase::Ended { .. } => true,
            Phase::Live(live) => live.renew.is_none(),
            _ => false,
        }
    }

    /// The person chose Retry, or Reload while it means Retry.
    pub(crate) fn retry(&mut self) -> Vec<Effect> {
        match self.take() {
            Phase::Ended { resume, .. } => self.acquire(Base::Fresh { resume }),
            Phase::Live(live) if live.renew.is_none() => self.acquire(live.renewal(false)),
            phase => self.keep(phase),
        }
    }

    pub(crate) fn waiting(&self) -> Option<Waiting> {
        match &self.phase {
            Phase::SignedOut => Some(Waiting::SignIn),
            Phase::Ended { message, .. } => Some(Waiting::Ended(message.clone())),
            phase if phase.renewing().is_some() => None,
            _ => Some(Waiting::Opening),
        }
    }

    fn take(&mut self) -> Phase {
        std::mem::take(&mut self.phase)
    }

    fn keep(&mut self, phase: Phase) -> Vec<Effect> {
        self.phase = phase;
        Vec::new()
    }

    fn acquire(&mut self, base: Base) -> Vec<Effect> {
        self.attempts += 1;
        let attempt = self.attempts;
        self.phase = Phase::Acquiring { attempt, base };
        vec![Effect::Acquire(attempt)]
    }

    /// Ends the session, which revokes it, and closes the view, which was
    /// at `here`.
    fn end(&mut self, here: Here) -> Vec<Effect> {
        self.ended_with(self.ended.clone(), here)
    }

    /// Ends the session with `message`; Retry reopens at `resume`.
    fn ended_with(&mut self, message: String, resume: Here) -> Vec<Effect> {
        self.phase = Phase::Ended { message, resume };
        vec![Effect::Close]
    }

    /// Checks the grant's cookie, then installs it, in a new view or the
    /// one the session already has.
    fn install(&mut self, attempt: u64, grant: SessionGrant, base: Base, now: Now) -> Vec<Effect> {
        let cookie = match grant.site_cookie(&self.scope) {
            Ok(_) if grant.ends.passed(now) => {
                return self.failed(base, SessionFailure::action(EXPIRED), now);
            }
            Ok(cookie) => cookie,
            Err(CookieProblem::Site) => {
                return self.failed(base, SessionFailure::action(INVALID), now);
            }
            Err(CookieProblem::Product(problem)) => {
                let mut effects = self.failed(base, SessionFailure::action(MISCONFIGURED), now);
                effects.push(Effect::Log(problem));
                return effects;
            }
        };
        let effect = match &base {
            Base::Fresh { .. } => Effect::Open(attempt, cookie),
            Base::Renewal { .. } => Effect::Install(attempt, cookie),
        };
        self.phase = Phase::Installing {
            attempt,
            grant,
            base,
        };
        vec![effect]
    }

    /// Without a view the failure ends the attempt; a live session keeps
    /// working, says why, and tries again by itself if it may pass.
    fn failed(&mut self, base: Base, failure: SessionFailure, now: Now) -> Vec<Effect> {
        match base {
            Base::Fresh { resume } => {
                self.phase = Phase::Ended {
                    message: failure.message,
                    resume,
                };
                Vec::new()
            }
            Base::Renewal { live, signalled } => {
                self.phase = Phase::Live(live.retry_later(failure.transient, signalled, now));
                vec![Effect::Notice(failure.message)]
            }
        }
    }

    /// The new grant replaces the old one, which is then revoked. A new
    /// view shows where the session should resume, and one the signal sent
    /// back loads `here` again; either falls back to the scope's home.
    fn go_live(
        &mut self,
        attempt: u64,
        grant: SessionGrant,
        base: Base,
        now: Now,
        here: Here,
    ) -> Vec<Effect> {
        let home = self.scope.home();
        let (effect, returned) = match base {
            Base::Fresh { resume } => (
                Effect::Show(attempt, resume.unwrap_or_else(|| home.to_owned())),
                None,
            ),
            Base::Renewal {
                signalled: true, ..
            } => (
                Effect::Return(here.unwrap_or_else(|| home.to_owned())),
                Some(now.after(SIGNAL_GUARD)),
            ),
            Base::Renewal { .. } => (Effect::Renewed, None),
        };
        self.phase = Phase::Live(Live::new(grant, returned, now));
        vec![effect]
    }
}
