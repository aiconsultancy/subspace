use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, px, size};
use subspace_core::diagnostics::{ErrorDetails, LocalDiagnostics};
use subspace_core::log::{self, Logger};

use super::LogView;

#[gpui_kit::test]
fn copied_error_record_keeps_its_message_and_fields_inside_the_log_view(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let target = "copy-details-log-layout-fixture";
    LocalDiagnostics::default().log_error(
        &Logger::new(target),
        "fixture-update-error",
        &ErrorDetails::new(
            "Fixture update error",
            "Fixture service unavailable.",
            "Check updates",
        )
        .with_chain("Fixture request failed: fixture service unavailable."),
    );
    let record = log::recent()
        .into_iter()
        .find(|record| record.target == target)
        .unwrap();
    let (_, cx) = cx.add_window_view(LogView::new);
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.update(|window, cx| {
        window.render_frame(cx);
        let message = window.find(("log-message", record.seq)).bounds();
        let fields = window.find(("log-fields", record.seq)).bounds();
        let content = window.find("log-lines").bounds();
        assert!(
            message.size.width > px(100.),
            "message must remain readable: {message:?}"
        );
        assert!(
            message.right() <= content.right(),
            "{message:?} outside {content:?}"
        );
        assert!(
            fields.right() <= content.right(),
            "{fields:?} outside {content:?}"
        );
        assert!(
            fields.size.height > px(20.),
            "structured fields must wrap: {fields:?}"
        );
    });
}
