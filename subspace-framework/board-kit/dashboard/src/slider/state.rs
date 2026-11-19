//! Retained native state and domain commit notifications.

use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Window};

use super::NumericSliderSpec;

const TICK_ORIGIN: f32 = 0.;
const TICK_STEP: f32 = 1.;

/// A user committed a domain value. External synchronization emits no event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumericSliderEvent {
    Commit(f64),
}

/// Retains the native slider and translates its integer ticks into domain values.
pub struct NumericSliderState {
    pub(super) spec: NumericSliderSpec,
    pub(super) inner: Entity<SliderState>,
    pub(super) tick: u32,
    pub(super) disabled: bool,
    _subscription: Subscription,
}

impl EventEmitter<NumericSliderEvent> for NumericSliderState {}

impl NumericSliderState {
    /// Creates the retained native state. Non-finite initial values use the minimum.
    pub fn new(
        spec: NumericSliderSpec,
        value: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tick = spec.tick(if value.is_finite() { value } else { spec.min });
        let inner = cx.new(|_| {
            SliderState::new()
                .min(TICK_ORIGIN)
                .max(spec.ticks as f32)
                .step(TICK_STEP)
                .default_value(tick as f32)
        });
        let subscription = cx.subscribe_in(&inner, window, Self::native_event);
        Self {
            spec,
            inner,
            tick,
            disabled: false,
            _subscription: subscription,
        }
    }

    /// Returns the current, quantized domain value.
    pub fn value(&self) -> f64 {
        self.spec.value(self.tick)
    }

    /// Synchronizes an external value without emitting a user commit. Non-finite values are ignored.
    pub fn set_value(&mut self, value: f64, window: &mut Window, cx: &mut Context<Self>) {
        if value.is_finite() {
            self.set_tick(self.spec.tick(value), window, cx);
        }
    }

    /// Enables or disables user interaction without emitting a user commit.
    pub fn set_disabled(&mut self, disabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.disabled = disabled;
        self.set_tick(self.tick, window, cx);
    }

    fn set_tick(&mut self, tick: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.tick = tick.min(self.spec.ticks);
        self.inner.update(cx, |inner, cx| {
            inner.set_value(self.tick as f32, window, cx)
        });
        cx.notify();
    }

    pub(super) fn user_tick(&mut self, tick: u32, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let tick = tick.min(self.spec.ticks);
        if tick != self.tick {
            self.set_tick(tick, window, cx);
            cx.emit(NumericSliderEvent::Commit(self.value()));
        }
    }

    fn native_event(
        &mut self,
        _: &Entity<SliderState>,
        event: &SliderEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let SliderEvent::Change(_) = event {
            cx.notify();
        }
        if let SliderEvent::Release(value) = event {
            let current = self.inner.read(cx).value().end();
            if self.disabled {
                self.set_tick(self.tick, window, cx);
            } else if current == value.end() {
                self.user_tick(value.end() as u32, window, cx);
            }
        }
    }
}
