//! Shell-owned Help Dock. The center adapter retains the existing workspace,
//! including any board-local Dock, without moving the toolbar or status bar.

mod contents;
mod geometry;
mod host;
mod panel;
mod shortcuts;
mod state;
mod workspace;

#[cfg(test)]
mod fixtures;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use panel::test_support;

pub(super) use host::{HelpDock, HelpDockEvent};
pub(super) use panel::HelpContent;
pub(super) use shortcuts::FocusShortcutFilter;

#[cfg(test)]
mod runtime_tests;
