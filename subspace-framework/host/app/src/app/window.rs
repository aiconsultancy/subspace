//! The window's stored bounds (REQ-08).

use gpui_kit::{App, Bounds, Pixels, WindowBounds, point, px, size};
use subspace_dashboard::prefs;

pub const WINDOW_KEY: &str = "app.window";

/// The default window when no stored bounds fit a display.
const DEFAULT_WIDTH: f32 = 1280.;
const DEFAULT_HEIGHT: f32 = 800.;
/// Stored bounds are the four numbers x, y, width and height.
const BOUNDS_FIELDS: usize = 4;

/// The window's stored bounds when they still fit a display, else the
/// default size, centred (REQ-08).
pub fn window_bounds(cx: &App) -> WindowBounds {
    let displays: Vec<Bounds<Pixels>> = cx.displays().iter().map(|d| d.bounds()).collect();
    let saved = prefs::get(cx, WINDOW_KEY).and_then(|text| parse_bounds(&text));
    match saved.filter(|bounds| fits(bounds, &displays)) {
        Some(bounds) => WindowBounds::Windowed(bounds),
        None => WindowBounds::centered(None, size(px(DEFAULT_WIDTH), px(DEFAULT_HEIGHT)), cx),
    }
}

pub fn parse_bounds(text: &str) -> Option<Bounds<Pixels>> {
    let [x, y, w, h]: [f32; BOUNDS_FIELDS] = serde_json::from_str(text).ok()?;
    (w > 0.0 && h > 0.0).then(|| Bounds::new(point(px(x), px(y)), size(px(w), px(h))))
}

pub fn format_bounds(bounds: &Bounds<Pixels>) -> String {
    let values = [
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    ];
    serde_json::to_string(&values).unwrap_or_default()
}

/// Whether `bounds` lies inside one display.
pub fn fits(bounds: &Bounds<Pixels>, displays: &[Bounds<Pixels>]) -> bool {
    displays.iter().any(|display| {
        display.contains(&bounds.origin)
            && bounds.right() <= display.right()
            && bounds.bottom() <= display.bottom()
    })
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{fits, format_bounds, parse_bounds};

    #[test]
    pub(super) fn stored_bounds_restore_only_inside_a_display() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let window = Bounds::new(point(px(100.), px(50.)), size(px(1280.), px(800.)));
        let text = format_bounds(&window);
        assert_eq!(parse_bounds(&text), Some(window));
        assert!(fits(&window, &[display]));

        let off_screen = Bounds::new(point(px(2500.), px(50.)), size(px(1280.), px(800.)));
        assert!(!fits(&off_screen, &[display]));
        let too_big = Bounds::new(point(px(0.), px(0.)), size(px(2560.), px(1440.)));
        assert!(!fits(&too_big, &[display]));
        assert_eq!(parse_bounds("broken"), None);
    }
}
