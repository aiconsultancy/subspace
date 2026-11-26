//! The issue profile: what Report issue may include about this build.

use subspace_core::diagnostics::IssueDiagnostics;
use subspace_product::ProductContext;
use subspace_report_issue::Diagnostics;

use super::build_identity;
use crate::theme::{ProductThemes, bundled_theme_id};

/// Inputs are descriptor IDs or embedded appearance identities, never paths.
/// `product_themes` are the app's own theme documents, whose ids `theme_name`
/// may resolve to.
pub(crate) fn collect(
    context: &ProductContext,
    active_board: Option<&str>,
    theme_name: &str,
    icon_size: &str,
    product_themes: ProductThemes,
) -> Diagnostics {
    let facts = IssueDiagnostics::from(build_identity(context))
        .with_appearance(bundled_theme_id(theme_name, product_themes), icon_size);
    let board = active_board.filter(|id| {
        context.product.boards.len() > 1
            && context
                .product
                .boards
                .iter()
                .any(|board| board.id.as_str() == *id)
    });
    match board {
        Some(id) => facts.with_active_board(id),
        None => facts,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};
    use subspace_report_issue::{Draft, prepare};

    use super::{bundled_theme_id, collect};
    use crate::app::report_issue::target;

    fn context() -> ProductContext {
        let known = fixture::products().unwrap();
        let sentinel = PathBuf::from("/test-only-private-home/config-and-logs");
        let env = PlatformEnv {
            os: OsKind::Linux,
            home: sentinel.clone(),
            xdg_config_home: Some(sentinel.clone()),
            xdg_state_home: Some(sentinel.clone()),
            app_data: Some(sentinel.clone()),
            local_app_data: Some(sentinel.clone()),
        };
        ProductContext::new(known[0].clone(), env)
    }

    #[test]
    fn diagnostic_allowlist_excludes_private_context_and_custom_names() {
        let mut context = context();
        context.product.display_name = "test-only-private-hostname".into();
        context.product.icon_paths = vec!["/test-only-private-image-path".into()];
        let diagnostics = collect(
            &context,
            Some("test-only-private-tunnel-name"),
            "test-only-private-theme-and-token",
            "medium",
            &[],
        );
        let repository = target(Some("example-org/example-app")).unwrap().unwrap();
        let report = prepare(&Draft::new("A test-only report"), &diagnostics, &repository).unwrap();
        assert!(!report.body().contains("test-only-private"));
        assert!(!report.body().contains("config-and-logs"));
        assert_eq!(diagnostics.product_id(), context.product.id.as_str());
        assert_eq!(diagnostics.app_version(), context.version);
        assert_eq!(diagnostics.os(), Some("linux"));
        assert_eq!(diagnostics.active_board(), None);
        assert_eq!(diagnostics.theme_id(), Some("custom"));
        assert_eq!(diagnostics.icon_size(), Some("medium"));
    }

    #[test]
    fn registered_board_and_embedded_theme_keep_safe_identities() {
        let mut context = context();
        context.build.commit = "abc1234";
        context.build.dirty = true;
        let board = context.product.boards[0].id.as_str();
        let diagnostics = collect(
            &context,
            Some(board),
            crate::theme::THEME_NAME,
            "small",
            &[],
        );
        assert_eq!(diagnostics.active_board(), Some(board));
        assert_eq!(diagnostics.theme_id(), Some("one-dark"));
        assert_eq!(bundled_theme_id("not-an-embedded-theme", &[]), "custom");
        assert_eq!(diagnostics.commit(), Some("abc1234-dirty"));
    }
}
