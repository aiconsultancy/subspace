//! Labelled native slider backed by an integer tick domain.

mod control;
mod spec;
mod state;

pub use control::{SliderControl, control};
pub use spec::{InvalidNumericSliderSpec, NumericSliderSpec};
pub use state::{NumericSliderEvent, NumericSliderState};

#[cfg(test)]
mod tests;
