//! Geometry and clock time live with the owner, independent of drawn elements.
use super::{CLOSED_PROGRESS, DISCLOSURE_DURATION, OPEN_PROGRESS};
use gpui_kit::{App, Pixels, ease_in_out};
use std::{cell::Cell, rc::Rc, time::Instant};

#[derive(Clone, Copy, Default)]
pub(super) struct Sample {
    pub(super) height: Pixels,
    pub(super) opacity: f32,
}
#[derive(Clone, Copy, Default)]
struct Transition {
    from: Sample,
    current: Sample,
    target: Sample,
    measured: Pixels,
    open: bool,
    started: Option<Instant>,
    reported_height: Pixels,
}
impl Transition {
    fn advance(&mut self, now: Instant, reduced: bool) {
        let Some(started) = self.started else { return };
        let progress = now.saturating_duration_since(started).as_secs_f32()
            / DISCLOSURE_DURATION.as_secs_f32();
        if reduced || progress >= OPEN_PROGRESS {
            self.current = self.target;
            self.started = None;
        } else {
            let delta = ease_in_out(progress);
            self.current = Sample {
                height: self.from.height + (self.target.height - self.from.height) * delta,
                opacity: self.from.opacity + (self.target.opacity - self.from.opacity) * delta,
            };
        }
    }
    fn retarget(&mut self, now: Instant) {
        self.from = self.current;
        self.target = if self.open {
            Sample {
                height: self.measured,
                opacity: OPEN_PROGRESS,
            }
        } else {
            Sample::default()
        };
        self.started = Some(now);
    }
}

/// Retain this in the owning view and notify that view after changing intent.
#[derive(Clone, Default)]
pub struct Disclosure(Rc<Cell<Transition>>);
impl Disclosure {
    pub fn set_open(&mut self, open: bool, cx: &App) {
        let mut transition = self.advanced(cx);
        transition.open = open;
        transition.retarget(cx.background_executor().now());
        self.0.set(transition);
    }
    pub fn is_open(&self) -> bool {
        self.0.get().open
    }
    fn advanced(&self, cx: &App) -> Transition {
        let mut transition = self.0.get();
        transition.advance(cx.background_executor().now(), cx.reduce_motion());
        self.0.set(transition);
        transition
    }
    pub(super) fn sample(&self, cx: &App) -> Sample {
        self.advanced(cx).current
    }
    pub(super) fn animating(&self) -> bool {
        self.0.get().started.is_some()
    }
    pub(super) fn report_height(&self, height: Pixels) -> bool {
        let mut transition = self.0.get();
        let changed = transition.reported_height != height;
        transition.reported_height = height;
        self.0.set(transition);
        changed
    }
    pub(super) fn measure(&self, height: Pixels, cx: &App) -> bool {
        let mut transition = self.advanced(cx);
        if transition.measured == height {
            return false;
        }
        let first = transition.measured == Pixels::ZERO;
        transition.measured = height;
        if transition.open {
            if transition.started.is_none() {
                // Reflow of settled content follows a live resize immediately.
                transition.target.height = height;
                transition.current = transition.target;
                transition.from = transition.target;
            } else if first {
                transition.target.height = height;
            } else {
                transition.retarget(cx.background_executor().now());
            }
        }
        self.0.set(transition);
        true
    }
    pub(super) fn preview(&self, progress: f32) -> Sample {
        Sample {
            height: self.0.get().measured * progress,
            opacity: progress,
        }
    }
    /// Hidden summaries must not contribute content or accessibility labels.
    pub fn is_visible(&self, cx: &App) -> bool {
        let transition = self.advanced(cx);
        transition.open
            || transition.started.is_some()
            || transition.current.opacity > CLOSED_PROGRESS
    }
}
