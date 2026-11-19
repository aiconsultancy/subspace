//! Offline seed for the real retained updater. Never starts an update request.

#[cfg(any(test, feature = "test-support"))]
pub(in crate::app) fn seed_failure(cx: &mut gpui_kit::App) {
    if let Some(panel) = super::panel(cx) {
        panel.update(cx, |panel, cx| {
            panel.apply(super::updates::UpdateEvent::CheckRequested, cx);
            panel.apply_check(
                Err("Could not check for updates. Fixture service unavailable.".into()),
                cx,
            );
        });
    }
}

/// The state after an update to `version` installed, for capture evidence of
/// Quit and restart. Installs nothing.
#[cfg(any(test, feature = "capture"))]
pub(in crate::app) fn seed_installed(version: &str, cx: &mut gpui_kit::App) {
    if let Some(panel) = super::panel(cx) {
        panel.update(cx, |panel, cx| {
            let installed = subspace_update::Installed {
                previous_version: panel.current_version().to_owned(),
                version: version.to_owned(),
                path: std::path::PathBuf::new(),
            };
            panel.state = super::updates::UpdateState::Installed(installed);
            panel.error_details = None;
            cx.notify();
        });
    }
}
