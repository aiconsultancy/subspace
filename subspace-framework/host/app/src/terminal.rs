//! The product's docked terminal panel: its registration and the stored
//! choices the shell and Settings share. The shell stays free of the
//! terminal crates; it reaches the panel only through the board-kit dock
//! panel contract.

use gpui_kit::{App, Global, KeyBinding};
use subspace_dashboard::dock_panel::{BottomSpan, DockPanelRegistration, PanelLayout, Placement};
use subspace_dashboard::prefs;

use crate::app::ToggleTerminal;

pub(crate) const OPEN_KEY: &str = "terminal.open";
pub(crate) const PLACEMENT_KEY: &str = "terminal.placement";
pub(crate) const SPAN_KEY: &str = "terminal.bottom-span";
pub(crate) const LABELS_KEY: &str = "terminal.button-labels";

/// Ctrl+` on every platform, also while the terminal has focus.
const TOGGLE_KEY: &str = "ctrl-`";

struct Registration(DockPanelRegistration);
impl Global for Registration {}

/// Keeps the product's panel for the window and Settings. Without one the
/// shell offers no terminal.
pub(crate) fn install(registration: Option<DockPanelRegistration>, cx: &mut App) {
    if let Some(registration) = registration {
        cx.set_global(Registration(registration));
    }
}

pub(crate) fn registration(cx: &App) -> Option<DockPanelRegistration> {
    cx.try_global::<Registration>()
        .map(|registration| registration.0)
}

pub(crate) fn bind_keys(cx: &mut App) {
    let Some(registration) = registration(cx) else {
        return;
    };
    cx.bind_keys([KeyBinding::new(TOGGLE_KEY, ToggleTerminal, None)]);
    (registration.bind_keys)(cx);
}

/// The terminal's toggle key as menus, tooltips and the palette show it.
pub(crate) fn toggle_key() -> &'static str {
    "Ctrl+`"
}

/// The stored open state and layout. A missing or unknown value is the
/// default: hidden, at the bottom, beside the chat, without button text.
/// Maximize is never stored.
pub(crate) fn stored(cx: &App) -> (bool, PanelLayout) {
    let open = prefs::get(cx, OPEN_KEY).as_deref() == Some("1");
    let layout = PanelLayout {
        placement: stored_placement(prefs::get(cx, PLACEMENT_KEY).as_deref()),
        span: stored_span(prefs::get(cx, SPAN_KEY).as_deref()),
        maximized: false,
        labels: prefs::get(cx, LABELS_KEY).as_deref() == Some("1"),
    };
    (open, layout)
}

fn stored_placement(value: Option<&str>) -> Placement {
    value.and_then(Placement::parse).unwrap_or_default()
}

fn stored_span(value: Option<&str>) -> BottomSpan {
    value.and_then(BottomSpan::parse).unwrap_or_default()
}

/// Stores the open state and layout in memory; callers save the state file.
pub(crate) fn store(open: bool, layout: PanelLayout, cx: &mut App) {
    prefs::set(cx, OPEN_KEY, flag(open));
    store_layout(layout, cx);
}

/// Stores the placement, bottom span and label choice in memory.
pub(crate) fn store_layout(layout: PanelLayout, cx: &mut App) {
    prefs::set(cx, PLACEMENT_KEY, layout.placement.key());
    prefs::set(cx, SPAN_KEY, layout.span.key());
    prefs::set(cx, LABELS_KEY, flag(layout.labels));
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use subspace_dashboard::dock_panel::{BottomSpan, PanelLayout, Placement};
    use subspace_dashboard::prefs;

    use super::{LABELS_KEY, OPEN_KEY, PLACEMENT_KEY, SPAN_KEY, store, stored};

    #[gpui_kit::test]
    fn the_terminal_layout_round_trips_and_unknown_values_fall_back(cx: &mut TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            assert_eq!(stored(cx), (false, PanelLayout::default()));
            let layout = PanelLayout {
                placement: Placement::Right,
                span: BottomSpan::FullWidth,
                maximized: true,
                labels: true,
            };
            store(true, layout, cx);
            assert_eq!(prefs::get(cx, OPEN_KEY).as_deref(), Some("1"));
            assert_eq!(prefs::get(cx, PLACEMENT_KEY).as_deref(), Some("right"));
            assert_eq!(prefs::get(cx, SPAN_KEY).as_deref(), Some("full-width"));
            assert_eq!(prefs::get(cx, LABELS_KEY).as_deref(), Some("1"));
            let restored = PanelLayout {
                maximized: false,
                ..layout
            };
            assert_eq!(stored(cx), (true, restored));
            prefs::set(cx, PLACEMENT_KEY, "log-tab");
            assert_eq!(stored(cx).1.placement, Placement::Bottom);
        });
    }
}
