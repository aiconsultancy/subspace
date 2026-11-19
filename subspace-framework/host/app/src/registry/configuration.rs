//! Provider-owned inputs, compared without serializing or exposing credentials.

use super::BoardRegistration;
use std::{any::Any, sync::Arc};

#[derive(Clone)]
pub struct BoardConfiguration {
    value: Arc<dyn Any + Send + Sync>,
    equal: fn(&dyn Any, &dyn Any) -> bool,
}

impl PartialEq for BoardConfiguration {
    fn eq(&self, other: &Self) -> bool {
        (self.equal)(self.value.as_ref(), other.value.as_ref())
    }
}

// qual:allow(coupling, deh) reason: "Configured providers own different concrete input types; this equality boundary uses checked Any downcasts and never formats credentials."
fn equal<T: PartialEq + 'static>(left: &dyn Any, right: &dyn Any) -> bool {
    match (left.downcast_ref::<T>(), right.downcast_ref::<T>()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

impl BoardRegistration {
    /// Captures configured inputs so unchanged instances survive live reloads.
    // qual:api
    pub fn with_configuration<T: PartialEq + Send + Sync + 'static>(mut self, value: T) -> Self {
        self.configuration = Some(BoardConfiguration {
            value: Arc::new(value),
            equal: equal::<T>,
        });
        self
    }
}
