//! The pull request diff view (REQ-63).

pub mod highlight;
pub mod model;
pub mod view;

pub(crate) use view::{DiffEvent, DiffView, OpenDiff};
