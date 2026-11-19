//! One Space's Enabled choice changed outside the Settings dialog, such as
//! from its switcher tab's menu. It is saved by the rule the Enabled switch
//! and Save follow and applied the way Save applies it, so both paths write
//! the same config and show the same tabs.

use std::sync::Arc;

use gpui_kit::{App, Window};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::Services;

use super::form::switched_board_values;

/// When a saved Enabled choice takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Applied {
    /// The Space's tab was added or removed at once.
    Now,
    /// The board set could not be rebuilt; the choice applies after a restart.
    AfterRestart,
}

/// Saves Space `id`'s Enabled choice to the config file, changing nothing
/// else, then shows or hides the Space as Save does.
pub(crate) fn save_board_enabled(
    id: &str,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> Result<Applied, String> {
    let default_enabled = board_default(id, cx);
    write_board_enabled(
        &crate::product::gui_config_file(cx),
        id,
        enabled,
        default_enabled,
    )?;
    mirror_saved_boards(cx);
    Ok(applied(apply_saved_boards(window, cx)))
}

fn write_board_enabled(
    file: &std::path::Path,
    id: &str,
    enabled: bool,
    default_enabled: bool,
) -> Result<(), String> {
    let mut settings = SettingsFile::open(file).map_err(|error| error.to_string())?;
    let boards = switched_board_values(&settings.values().boards, id, enabled, default_enabled);
    settings
        .save_board_choices(&boards)
        .map_err(|error| error.to_string())
}

/// The product default of Space `id`, as Settings resolves it.
fn board_default(id: &str, cx: &App) -> bool {
    super::included_board_defaults(&crate::registry::included_registrations(cx), cx)
        .into_iter()
        .find_map(|(board, default_enabled)| (board == id).then_some(default_enabled))
        .unwrap_or(true)
}

/// Copies the saved board choices into the running config, as Save does.
fn mirror_saved_boards(cx: &mut App) {
    if !cx.has_global::<Services>() {
        return;
    }
    let file = crate::product::gui_config_file(cx);
    let services = cx.global_mut::<Services>();
    let mut config = (*services.config).clone();
    super::load_saved_boards(&mut config, &file);
    services.config = Arc::new(config);
}

/// Shows and hides the saved board choices without a restart, as Save does.
/// Returns false, after logging why, when they apply only after a restart.
pub(super) fn apply_saved_boards(window: &mut Window, cx: &mut App) -> bool {
    if !cx.has_global::<Services>() {
        return false;
    }
    let result = crate::registry::apply_running_config(window, cx);
    log_restart_needed(result.as_ref().err());
    result.is_ok()
}

fn log_restart_needed(error: Option<&String>) {
    if let Some(error) = error {
        subspace_core::log::Logger::new("settings").record(
            subspace_core::log::Level::Warn,
            format!("Board choice applies after a restart: {error}"),
            &[],
        );
    }
}

fn applied(now: bool) -> Applied {
    if now {
        Applied::Now
    } else {
        Applied::AfterRestart
    }
}
