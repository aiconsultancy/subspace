//! Keeps the picker off GPUI Kit's window layers.
//!
//! While picking, GPUI gives every element a hitbox and picks the topmost one.
//! The Kit `Root` mounts transparent, window-sized containers for its plugins,
//! dialogs, sheets and notifications above the application, so the topmost
//! element is always one of them. GPUI's own way to reach an element
//! underneath is the scroll wheel, one layer per notch. This Root plugin takes
//! those notches before the window lays out, so the frame after a hover
//! already shows the element under the layers.
//!
//! The Kit roots' own containers are known by their source file. Any other
//! transparent, window-sized Kit element is learnt the first time it is
//! picked and skipped from then on.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::{
    App, Bounds, Context, DivInspectorState, Empty, Global, InspectorElementId,
    InspectorElementPath, IntoElement, Pixels, PlatformInput, Render, ScrollDelta, ScrollEvent,
    ScrollSource, TouchPhase, Window, point, px, rems,
};

/// One layer of GPUI's inspector scroll depth.
const LAYER_DELTA: f32 = 36.;
/// More than the Kit's layers, so a wrong guess cannot loop.
const MAX_LAYERS: usize = 8;
/// GPUI gives its inspector the window's last 30rem.
const INSPECTOR_WIDTH_REM: f32 = 30.;
/// Rounding slack when comparing a layer with the window.
const COVER_SLACK: f32 = 1.;
const KIT_CRATES: [&str; 2] = ["gpui-component-", "gpui-base-"];

/// Transparent, window-sized Kit elements seen so far.
#[derive(Default)]
struct PassThrough(HashSet<Rc<InspectorElementPath>>);
impl Global for PassThrough {}

pub(super) fn register(cx: &mut App) {
    cx.set_global(PassThrough::default());
    Root::register_plugin::<SkipWindowLayers>(cx, |_, _| SkipWindowLayers);
}

struct SkipWindowLayers;

impl Render for SkipWindowLayers {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl RootPlugin for SkipWindowLayers {
    fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_inspector_picking(cx) {
            return;
        }
        let Some(inspector) = super::inspector_of(window, cx) else {
            return;
        };
        for _ in 0..MAX_LAYERS {
            let on_layer = inspector
                .read(cx)
                .active_element_id()
                .is_some_and(|id| is_layer(id, cx));
            if !on_layer {
                return;
            }
            window.dispatch_event(one_layer_down(window), cx);
        }
    }
}

fn one_layer_down(window: &Window) -> PlatformInput {
    PlatformInput::Scroll(ScrollEvent {
        position: window.mouse_position(),
        delta: ScrollDelta::Pixels(point(px(0.), px(LAYER_DELTA))),
        modifiers: window.modifiers(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    })
}

fn is_layer(id: &InspectorElementId, cx: &App) -> bool {
    is_root_container(id.path.source_location.file())
        || cx
            .try_global::<PassThrough>()
            .is_some_and(|seen| seen.0.contains(&id.path))
}

/// Records `id` when it is a transparent Kit element as large as the
/// window, and says whether it was one.
pub(super) fn learn(
    id: &InspectorElementId,
    state: &DivInspectorState,
    window: &Window,
    cx: &mut App,
) -> bool {
    let layer = is_kit_source(id.path.source_location.file())
        && state.base_style.background.is_none()
        && covers_window(state.bounds, window);
    if layer {
        cx.default_global::<PassThrough>().0.insert(id.path.clone());
    }
    layer || is_layer(id, cx)
}

fn covers_window(bounds: Bounds<Pixels>, window: &Window) -> bool {
    let viewport = window.viewport_size();
    let width = viewport.width - rems(INSPECTOR_WIDTH_REM).to_pixels(window.rem_size());
    let slack = px(COVER_SLACK);
    bounds.origin.x <= slack
        && bounds.origin.y <= slack
        && bounds.size.width + slack >= width
        && bounds.size.height + slack >= viewport.height
}

/// A registry checkout or the in-repository GPUI Base patch.
fn kit_relative(file: &str) -> Option<&str> {
    if let Some((_, relative)) = file.rsplit_once("vendor/gpui-base/") {
        return Some(relative);
    }
    KIT_CRATES.iter().find_map(|name| {
        let (_, rest) = file.rsplit_once(name)?;
        let (version, path) = rest.split_once('/')?;
        version
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_digit)
            .then_some(path)
    })
}

fn is_kit_source(file: &str) -> bool {
    kit_relative(file).is_some()
}

/// The containers the Kit roots mount above the application.
pub(super) fn is_root_container(file: &str) -> bool {
    kit_relative(file) == Some("src/root.rs")
}
