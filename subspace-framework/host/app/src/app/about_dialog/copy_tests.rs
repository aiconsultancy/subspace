use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use subspace_core::diagnostics::{BuildIdentity, LocalDiagnostics};
use subspace_dashboard::copy_details::CopyDiagnostics;

#[gpui_kit::test]
fn about_copies_the_local_profile_without_an_error(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (handle, app) =
        crate::app::toolbar::tests::status::rooted_fixture(cx, root.path(), 1200., false);
    cx.update(|cx| {
        cx.set_reduce_motion(true);
        cx.set_global(CopyDiagnostics::new(LocalDiagnostics::new(
            BuildIdentity::new("subspace", "2.3.4")
                .with_build("dev", "abcdef0")
                .with_platform("macos", "aarch64"),
            "1.2.3",
        )));
    });
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.open_about(&crate::app::OpenAbout, window, cx)
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("about-copy-diagnostics").label(),
            Some("Copy diagnostics")
        );
        window.click("about-copy-diagnostics", cx);
    })
    .unwrap();
    let copied = cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap());
    assert!(copied.contains("2.3.4"));
    assert!(copied.contains("1.2.3"));
    assert!(!copied.contains("- Stage:"));
}
