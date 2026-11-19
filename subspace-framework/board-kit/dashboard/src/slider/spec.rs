//! Numeric-domain validation and conversion to native slider ticks.

/// The maximum integer that the native `f32` slider represents exactly.
const MAX_EXACT_TICKS: f64 = (1_u32 << f32::MANTISSA_DIGITS) as f64;
const FIRST_TICK: f64 = 1.;
const ZERO_STEP: f64 = 0.;

/// A finite numeric range and the step between values. The last tick is `max`.
#[derive(Clone, Copy, Debug)]
pub struct NumericSliderSpec {
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) step: f64,
    pub(super) ticks: u32,
}

/// The range cannot be represented by the native slider's integer tick domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidNumericSliderSpec;

impl NumericSliderSpec {
    /// Rejects non-finite, unordered, unresolvable or oversized ranges.
    pub fn new(min: f64, max: f64, step: f64) -> Result<Self, InvalidNumericSliderSpec> {
        let span = max - min;
        let mut count = (span / step).ceil();
        if count > FIRST_TICK && (min + (count - FIRST_TICK) * step) >= max {
            count -= FIRST_TICK;
        }
        if !valid_range(min, max, step, span) || !valid_tick_count(count) {
            return Err(InvalidNumericSliderSpec);
        }
        Ok(Self {
            min,
            max,
            step,
            ticks: count as u32,
        })
    }

    pub(super) fn value(self, tick: u32) -> f64 {
        if tick >= self.ticks {
            self.max
        } else {
            (self.min + f64::from(tick) * self.step).min(self.max)
        }
    }

    pub(super) fn tick(self, value: f64) -> u32 {
        let value = value.clamp(self.min, self.max);
        let lower = (((value - self.min) / self.step).floor() as u32).min(self.ticks);
        let upper = lower.saturating_add(1).min(self.ticks);
        if value - self.value(lower) < self.value(upper) - value {
            lower
        } else {
            upper
        }
    }
}

fn valid_range(min: f64, max: f64, step: f64, span: f64) -> bool {
    min.is_finite()
        && max.is_finite()
        && step.is_finite()
        && span.is_finite()
        && min < max
        && step > ZERO_STEP
        && min + step > min
        && max - step < max
}

fn valid_tick_count(count: f64) -> bool {
    count.is_finite() && (FIRST_TICK..=MAX_EXACT_TICKS).contains(&count)
}
