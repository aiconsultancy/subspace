//! Offline HTTP fixtures, available only through the explicit test-support feature.
// The fixture implementation remains test-owned, including for source-quality analysis.

#[cfg(feature = "test-support")]
#[path = "../tests/support/http.rs"]
mod http;
#[cfg(feature = "test-support")]
pub use http::{Mock, Request};
