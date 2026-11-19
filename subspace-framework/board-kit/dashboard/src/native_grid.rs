//! Retained, bounded native result grid. Exploration never calls a data provider.
//!
//! GPUI Kit 0.9.1 TableState exposes one selected cell and position-only selection
//! events, not modifier-aware rectangular selection. This grid uses uniform_list
//! for row virtualization and owns one identity-based range model instead.

mod cell;
mod model;
mod output;
mod selection;
mod view;

pub use cell::{GridCell, GridSnapshot};
pub use output::{GridFormat, GridScope};
pub use view::{GridExport, NativeGrid};

#[cfg(test)]
mod tests;
