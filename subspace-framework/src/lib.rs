//! The Subspace Framework: the crates an app is composed from, re-exported as
//! modules named after them without the `subspace_` prefix.
//!
//! The default `space` feature is what a Space needs: the services, the board
//! kit and the GitHub client. `host` adds the shell and the other host crates,
//! for app builders only. `agent`, `report-issue`, `search-onnx`,
//! `component-gallery`, `test-support` and `capture` add the rest.

pub use ::subspace_product as product;

#[cfg(feature = "space")]
pub use ::subspace_account as account;
#[cfg(feature = "space")]
pub use ::subspace_agent_model as agent_model;
#[cfg(feature = "space")]
pub use ::subspace_agent_tools as agent_tools;
#[cfg(feature = "space")]
pub use ::subspace_charts as charts;
#[cfg(feature = "space")]
pub use ::subspace_client_github as client_github;
#[cfg(feature = "space")]
pub use ::subspace_contracts as contracts;
#[cfg(feature = "space")]
pub use ::subspace_core as core;
#[cfg(feature = "space")]
pub use ::subspace_dashboard as dashboard;
#[cfg(feature = "space")]
pub use ::subspace_github_auth as github_auth;
#[cfg(feature = "space")]
pub use ::subspace_lucide as lucide;
#[cfg(feature = "space")]
pub use ::subspace_search as search;
#[cfg(feature = "space")]
pub use ::subspace_web as web;

#[cfg(feature = "agent")]
pub use ::subspace_agent as agent;

#[cfg(feature = "host")]
pub use ::subspace_board_command as board_command;
#[cfg(feature = "host")]
pub use ::subspace_chat as chat;
#[cfg(feature = "host")]
pub use ::subspace_entra_auth as entra_auth;
#[cfg(feature = "host")]
pub use ::subspace_mcp as mcp;
#[cfg(feature = "host")]
pub use ::subspace_search_ui as search_ui;
#[cfg(feature = "host")]
pub use ::subspace_shell as shell;
#[cfg(feature = "host")]
pub use ::subspace_update as update;
#[cfg(feature = "host")]
pub use ::subspace_workos_auth as workos_auth;

#[cfg(feature = "report-issue")]
pub use ::subspace_report_issue as report_issue;

#[cfg(feature = "search-onnx")]
pub use ::subspace_search_onnx as search_onnx;

#[cfg(feature = "component-gallery")]
pub use ::subspace_component_gallery_board as component_gallery;

#[cfg(feature = "test-support")]
pub use ::subspace_client_test_support as client_test_support;

/// The README's examples, compiled as doctests with the `report-issue`
/// feature (which includes `host`).
#[cfg(all(doctest, feature = "report-issue"))]
#[doc = include_str!("../README.md")]
// Exists only for rustdoc to test the README; nothing names it.
#[allow(dead_code)]
pub struct ReadmeDoctests;

/// The command-line guide's examples, compiled as doctests like the README's.
#[cfg(all(doctest, feature = "report-issue"))]
#[doc = include_str!("../docs/cli.md")]
// Exists only for rustdoc to test the guide; nothing names it.
#[allow(dead_code)]
pub struct CliDoctests;
