//! The Framework's terminal panel: local shell sessions in tabs, docked by
//! the shell beside every Space through the board-kit dock panel contract.

mod font_settings;
mod header;
mod keys;
mod panel;
mod view;
use subspace_terminal::close_policy as status;
use subspace_terminal::settings as reload;

pub use panel::{TerminalPanel, registration};
