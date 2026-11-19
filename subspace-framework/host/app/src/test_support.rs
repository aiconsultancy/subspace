//! Product tests use the host's parser and renderer without accessing its UI
//! state. Available only in tests or with the `test-support` feature.

use gpui_kit::TestAppContext;
use subspace_agent_tools::BoardDoc;

mod offline;
pub use crate::app::SubspaceApp as FixtureShell;
pub use crate::app::copy_details_fixture::copy_details_shell_fixture;
pub use offline::{install_offline_fixture_services, offline_shell_fixture};

pub use crate::assets::Assets;
pub use gpui_kit::AssetSource;

/// Assert that a bundled guide has useful contents and loses no source text.
// qual:api (product crates check their registered guides with the host parser)
pub fn assert_help_contents(markdown: &str) {
    crate::app::test_support::assert_help_contents(markdown);
}

/// Render a guide and check that its first image reaches the native cache.
// qual:api (product crates check registered images with the host renderer)
pub fn check_help_image_cache(board: BoardDoc) -> Result<(), String> {
    let mut cx = TestAppContext::single();
    let result = crate::app::test_support::check_help_image_cache(board, &mut cx);
    cx.run_until_parked();
    cx.quit();
    cx.run_until_parked();
    result
}
