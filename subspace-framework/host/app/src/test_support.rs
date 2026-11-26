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

/// Validate an app's own theme documents (`ProductBuild.product_themes`) as
/// startup does, apart from the product's default, and install them so the
/// shell fixtures load them. Call it before [`offline_shell_fixture`].
// qual:api (product crates load their own theme documents into the shell fixtures)
pub fn install_product_themes(
    themes: crate::ProductThemes,
    cx: &mut gpui_kit::App,
) -> Result<(), String> {
    crate::theme::validate_product_theme_documents(themes)?;
    crate::theme::install_product_themes(themes, cx);
    Ok(())
}

/// Register an app's embedded fonts as startup does, and return the families
/// that then count as installed, or the notice startup would show. The test
/// text system loads no font data, so this checks the list, not the files.
// qual:api (product crates check their embedded font list with the host's registration)
pub fn registered_font_families(
    fonts: &'static [crate::BundledFont],
) -> Result<Vec<String>, String> {
    let cx = TestAppContext::single();
    let result = cx.update(|cx| {
        crate::bundled_fonts::register(fonts, cx)
            .map_or_else(|| Ok(crate::bundled_fonts::registered(cx)), Err)
    });
    cx.quit();
    cx.run_until_parked();
    result
}
