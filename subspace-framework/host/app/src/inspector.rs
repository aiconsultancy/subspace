//! The element inspector, in debug builds only. GPUI's built-in inspector
//! picks elements and keeps their source location and id path; Subspace
//! supplies the panel, the box overlay and the component names.

mod layers;
mod model;
mod overlay;
mod panel;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gpui_kit::{
    App, DivInspectorState, Empty, Global, Inspector, KeyBinding, KeystrokeEvent, Menu, MenuItem,
    WeakEntity, Window, WindowId, actions,
};
use subspace_dashboard::{PaletteCommand, secondary};

use model::ElementReport;

actions!(
    subspace_app,
    [
        /// Show or hide the inspector in the window.
        ToggleInspector,
        /// Open Settings at the Developer page.
        OpenDeveloperSettings
    ]
);

/// The inspector key as menus and help show it.
pub(crate) fn shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘F12"
    } else {
        "Ctrl+F12"
    }
}

/// GPUI Kit's own inspector key, which now opens this inspector.
const KIT_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-alt-i"
} else {
    "ctrl-shift-i"
};

/// Windows whose inspector is open, with the inspector once it has rendered.
#[derive(Default)]
struct OpenWindows(HashMap<WindowId, Option<WeakEntity<Inspector>>>);
impl Global for OpenWindows {}

/// The active element's report, which the `Div` state renderer leaves for
/// the panel in each frame.
#[derive(Default)]
struct Reported(Option<ElementReport>);
impl Global for Reported {}

/// Installs the panel and keys. Call after `gpui_kit::init`, whose own
/// inspector renderer this replaces.
pub(crate) fn init(cx: &mut App) {
    cx.set_global(OpenWindows::default());
    cx.bind_keys([
        KeyBinding::new(secondary!("f12"), ToggleInspector, None),
        KeyBinding::new(KIT_KEY, ToggleInspector, None),
    ]);
    cx.on_action(|_: &ToggleInspector, cx| toggle_target(cx));
    cx.set_global(Reported::default());
    layers::register(cx);
    register_renderers(cx);
    cx.intercept_keystrokes(close_on_escape).detach();
}

fn register_renderers(cx: &mut App) {
    cx.register_inspector_element(|_, _| {
        |id, state: &DivInspectorState, window: &mut Window, cx: &mut App| {
            // A Kit layer keeps the last element's report until it is skipped.
            if !layers::learn(&id, state, window, cx) {
                let report = ElementReport::new(&id, state, window.rem_size());
                cx.set_global(Reported(Some(report)));
            }
            Empty
        }
    });
    cx.set_inspector_renderer(Box::new(|inspector, window, cx| {
        // Updates the report; the Div renderer itself draws nothing.
        drop(inspector.render_inspector_states(window, cx));
        let report = cx.global::<Reported>().0.clone();
        let picking = inspector.is_picking();
        let entity = cx.entity();
        remember(window, &entity, cx);
        panel::render(report, (&entity, picking), window, cx)
    }));
}

/// Developer mode turns the inspector off; it can still close.
fn toggle_target(cx: &mut App) {
    let Some(target) = cx.active_window().or_else(|| cx.windows().first().copied()) else {
        return;
    };
    let allowed = crate::settings::developer_mode(cx);
    cx.defer(move |cx| {
        _ = target.update(cx, |_, window, cx| {
            if allowed || is_open(window, cx) {
                toggle(window, cx);
            }
        });
    });
}

/// The active element's component name and bounds, for shell tests.
#[cfg(test)]
pub(crate) fn reported(cx: &App) -> Option<(Option<String>, gpui_kit::Bounds<gpui_kit::Pixels>)> {
    let report = cx.try_global::<Reported>()?.0.as_ref()?;
    Some((report.name(), report.layout.bounds))
}

pub(crate) fn is_open(window: &Window, cx: &App) -> bool {
    cx.try_global::<OpenWindows>()
        .is_some_and(|open| open.0.contains_key(&window.window_handle().window_id()))
}

fn remember(window: &Window, inspector: &gpui_kit::Entity<Inspector>, cx: &mut App) {
    let id = window.window_handle().window_id();
    if let Some(slot) = cx.default_global::<OpenWindows>().0.get_mut(&id) {
        *slot = Some(inspector.downgrade());
    }
}

/// The window's inspector, once it has rendered.
fn inspector_of(window: &Window, cx: &App) -> Option<gpui_kit::Entity<Inspector>> {
    cx.try_global::<OpenWindows>()?
        .0
        .get(&window.window_handle().window_id())?
        .as_ref()?
        .upgrade()
}

/// Whether any window shows the inspector, for the palette's wording.
pub(crate) fn any_open(cx: &App) -> bool {
    cx.try_global::<OpenWindows>()
        .is_some_and(|open| !open.0.is_empty())
}

pub(crate) fn toggle(window: &mut Window, cx: &mut App) {
    cx.set_global(Reported::default());
    let id = window.window_handle().window_id();
    let open = &mut cx.default_global::<OpenWindows>().0;
    if open.remove(&id).is_none() {
        open.insert(id, None);
    }
    window.toggle_inspector(cx);
}

pub(crate) fn close(window: &mut Window, cx: &mut App) {
    if is_open(window, cx) {
        toggle(window, cx);
    }
}

/// Escape closes the inspector before anything under it sees the key.
fn close_on_escape(event: &KeystrokeEvent, window: &mut Window, cx: &mut App) {
    let keystroke = &event.keystroke;
    if keystroke.key == "escape"
        && keystroke.modifiers.number_of_modifiers() == 0
        && is_open(window, cx)
    {
        close(window, cx);
        cx.stop_propagation();
    }
}

/// The Developer menu, after View, as Safari places its Develop menu.
pub(crate) fn menu() -> Menu {
    Menu::new("Developer").items([
        MenuItem::action("Inspect Elements", ToggleInspector),
        MenuItem::separator(),
        MenuItem::action("Developer Settings…", OpenDeveloperSettings),
    ])
}

/// The palette's Developer commands; the inspector one says what it does next.
pub(crate) fn palette_commands(cx: &App) -> Vec<PaletteCommand> {
    if !crate::settings::developer_mode(cx) {
        return Vec::new();
    }
    let label = if any_open(cx) {
        "Close inspector"
    } else {
        "Inspect elements"
    };
    vec![
        PaletteCommand::new(label, ToggleInspector)
            .keywords("inspector devtools developer bounds component element debug")
            .shortcut(shortcut_label()),
        PaletteCommand::new("Developer settings", OpenDeveloperSettings)
            .keywords("developer mode inspector settings"),
    ]
}
