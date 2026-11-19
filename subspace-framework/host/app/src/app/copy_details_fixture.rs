//! Actual shell surfaces with synthetic diagnostics and no external services.

use std::path::Path;

use gpui_kit::{App, Entity, Window};
use subspace_core::diagnostics::{
    BuildIdentity, LaunchKind, LocalDiagnostics, RedactionContext, RuntimeDetails,
};
use subspace_dashboard::copy_details::CopyDiagnostics;
use subspace_product::ProductContext;

use super::{OpenAbout, OpenLogView, OpenSettings, SubspaceApp};

pub fn copy_details_shell_fixture(
    context: ProductContext,
    root: &Path,
    state: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SubspaceApp> {
    let text = crate::build_metadata::about_text_for(&context);
    let local = fixture_diagnostics(&context);
    let app = crate::test_support::offline_shell_fixture(context, root, window, cx);
    cx.set_global(CopyDiagnostics::new(local));
    cx.set_global(super::about_dialog::FixtureAboutText(text));
    let state = state.to_owned();
    let weak = app.downgrade();
    window.on_next_frame(move |window, cx| {
        _ = weak.update(cx, |app, cx| open_state(app, &state, window, cx));
    });
    app
}

fn open_state(
    app: &mut SubspaceApp,
    state: &str,
    window: &mut Window,
    cx: &mut gpui_kit::Context<SubspaceApp>,
) {
    if state.starts_with("update-") {
        super::about_dialog::update_panel::seed_failure(cx);
    }
    if state == "update-log" {
        app.open_log_view(&OpenLogView, window, cx);
    } else if state == "update-compact" {
        app.open_settings(&OpenSettings, window, cx);
    } else {
        app.open_about(&OpenAbout, window, cx);
    }
}

fn fixture_diagnostics(context: &ProductContext) -> LocalDiagnostics {
    LocalDiagnostics::new(
        BuildIdentity::new(context.product.id.as_str(), &context.version)
            .with_build("dev", "fixture-commit")
            .with_platform("macos", "aarch64"),
        &context.subspace_version,
    )
    .with_os_version("15.0")
    .with_runtime(RuntimeDetails::new(
        "/Users/fixture/Applications/Fixture.app/Contents/MacOS/fixture",
        LaunchKind::Desktop,
        "login shell /bin/zsh",
        None,
    ))
    .with_redaction(RedactionContext::new(
        Some("/Users/fixture".into()),
        Vec::new(),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use gpui_kit::component::WindowExt as _;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, ElementId, TestAppContext, WindowOptions};
    use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};

    fn context(root: &Path) -> ProductContext {
        let product = fixture::products()
            .unwrap()
            .into_iter()
            .find(|product| product.id.as_str() == "subspace")
            .unwrap();
        ProductContext::new(
            product,
            PlatformEnv {
                os: OsKind::Macos,
                home: root.join("home"),
                xdg_config_home: Some(root.join("config")),
                xdg_state_home: Some(root.join("state")),
                app_data: None,
                local_app_data: None,
            },
        )
    }

    #[gpui_kit::test]
    fn show_log_from_the_real_about_error_reaches_the_shell_view(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (window, app) = cx.update(|cx| {
            gpui_kit::init(cx);
            // Pointer delivery renders multiple frames; keep the dialog target at rest.
            cx.set_reduce_motion(true);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let context = context(root.path());
                let local = super::fixture_diagnostics(&context);
                let app =
                    crate::test_support::offline_shell_fixture(context, root.path(), window, cx);
                cx.set_global(subspace_dashboard::copy_details::CopyDiagnostics::new(
                    local,
                ));
                app
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                super::open_state(app, "update-about", window, cx)
            });
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));
            let show_log =
                ElementId::NamedChild(Arc::new("update-copy-details".into()), "show-log".into());
            assert!(window.find(show_log.clone()).visible());
            assert_eq!(window.find(show_log.clone()).label(), Some("Show log"));
            window.click(show_log, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                matches!(app.read(cx).main, crate::app::MainSurface::Logs { .. }),
                "Show log must reach the shell's existing command"
            );
            assert!(
                !window.has_active_dialog(cx),
                "the error dialog must not cover the log view"
            );
            assert!(window.find("log-view").visible());
        })
        .unwrap();
        assert!(
            subspace_core::log::recent()
                .iter()
                .any(|record| record.fields.iter().any(|(key, value)| {
                    key == "message"
                        && value == "Could not check for updates. Fixture service unavailable."
                }))
        );
    }
}
