//! Subspace core: the config file (REQ-20), credential sources (REQ-21 to
//! REQ-24, REQ-31), the shared HTTP agent, the state file (REQ-08), the item
//! search index and the palette's command scoring.
//! No GPUI and no service requests: the `clients/*` crates build on this.

pub mod config;
pub mod copilot_agent_tasks;
pub mod credentials;
pub mod diagnostics;
#[cfg(test)]
mod executable_tests;
pub mod features;
pub mod file_id;
pub mod http;
pub mod keychain;
pub mod keyed_request;
pub mod log;
pub mod mcp_token;
pub mod palette;
pub mod path;
pub mod private_file;
pub mod run_day_log;
pub mod search;
mod secret;
pub mod settings;
pub mod state;
pub mod task_state;
pub mod vault;

pub use secret::Secret;
