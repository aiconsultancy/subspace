//! Appearance only; native tabs and buttons retain activation and focus.

use super::{CONTENT_GAP, Tab, TabSpec};
use crate::{Tokens, icon_button::IconButton, tokens::ButtonFinish};
use gpui_kit::component::{Size, h_flex, tab::Tab as KitTab};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Pixels, SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};
use std::rc::Rc;
use subspace_lucide::IconName;

pub(super) type CloseHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
const TRANSPARENT: f32 = 0.;
const OPAQUE: f32 = 1.;
const DISABLED_OPACITY: f32 = 0.5;
/// Kit inserts gap_1 after the decorative prefix (in rems).
const PREFIX_GAP: f32 = 0.25;
const DOT_SIZE: f32 = 5.;
/// An opened item's key, at the default monospace setting.
const KEY_MONO_PX: f32 = 11.5;
const GROUP: &str = "subspace-resource-tab";
// Kit's TabVariant::Tab heights are private. Keep this mapping aligned with
// its public Sizable API; render tests check the small and default heights.
const SMALL_HEIGHT: f32 = 24.;
const DEFAULT_HEIGHT: f32 = 32.;
const XSMALL_HEIGHT: f32 = 20.;
const LARGE_HEIGHT: f32 = 36.;
/// Wider than any tab; the tab's own `overflow_hidden` clips the layers.
const LAYER_REACH: f32 = 4096.;
/// Kit's `TabVariant::Tab` border, on either side of a tab. A prefix-less
/// pane bar shifts its first tab by this much to clip that tab's leading
/// border (see [`Dress::at_edge`]).
const KIT_TAB_BORDER_PX: f32 = 1.;

#[derive(Debug, PartialEq)]
pub(super) struct State {
    pub(super) foreground: gpui_kit::Hsla,
    pub(super) background: gpui_kit::Hsla,
    pub(super) weight: FontWeight,
}

pub(super) fn state(
    selected: bool,
    hovered: bool,
    disabled: bool,
    tokens: &Tokens,
    surface: gpui_kit::Hsla,
) -> State {
    let hover = match tokens.controls.toolbar_look.hover {
        ButtonFinish::Flat => tokens.button_hover,
        ButtonFinish::Glass => tokens.glass,
    };
    State {
        foreground: if selected && !disabled {
            tokens.fg_strong
        } else {
            tokens.muted
        },
        background: if disabled {
            surface.opacity(TRANSPARENT)
        } else if selected {
            surface
        } else if hovered {
            hover
        } else {
            surface.opacity(TRANSPARENT)
        },
        weight: if selected && !disabled {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        },
    }
}

pub fn unread_dot(tokens: &Tokens) -> AnyElement {
    div()
        .size(px(DOT_SIZE))
        .flex_shrink_0()
        .rounded_full()
        .bg(tokens.accent)
        .into_any_element()
}

/// How a bar dresses its tabs.
#[derive(Clone)]
pub(super) struct Dress {
    /// The adjoining content or header surface the selected tab takes.
    pub(super) surface: gpui_kit::Hsla,
    pub(super) size: Size,
    /// A workspace pane's tab strip: each tab is as tall as the strip. In
    /// every bar the selected tab is told apart by its surface and its
    /// stronger, medium-weight text, never by a line on one edge.
    pub(super) pane: bool,
    /// The workspace's strip height, which a pane's tabs fill.
    pub(super) strip: Pixels,
    /// The first tab of a pane's tab strip, which starts at the pane's
    /// leading edge: the dock beside it, or the window, already draws that
    /// line, so the tab's own leading border sits just outside the bar,
    /// where the bar clips it.
    ///
    /// A workaround for GPUI Kit 0.9.1: Kit means to drop the first tab's
    /// leading border when the bar has no prefix, but `tab/tab_bar.rs:514`
    /// sets `tab_bar_prefix.unwrap_or(true)` for every tab, so it never
    /// does, and Kit paints borders over the tab's children. Remove this
    /// when Kit passes `tab_bar_prefix` correctly.
    pub(super) at_edge: bool,
    /// The monospace family, for an opened item's key.
    pub(super) mono: SharedString,
}

pub(super) fn tab(tab: Tab, selected: bool, tokens: &Tokens, dress: Dress) -> KitTab {
    let disabled = tab.spec.disabled;
    let normal = state(selected, false, disabled, tokens, dress.surface);
    let hover = state(selected, true, disabled, tokens, dress.surface);
    let glass_hover = tokens.controls.toolbar_look.hover == ButtonFinish::Glass;
    let close_label = tab_label(&tab.spec.label);
    let height = tab_height(&dress);
    let fill = div()
        .absolute()
        .top_0()
        .left_0()
        .h_full()
        .w(px(LAYER_REACH))
        .bg(normal.background)
        .when(!disabled, |layer| {
            layer.group_hover(GROUP, |layer| layer.bg(hover.background))
        })
        .when(!selected && !disabled && glass_hover, |layer| {
            layer.child(hover_rim(tokens.glass_rim))
        });
    let content = content_row(tab.spec, &normal, tokens, dress.mono);
    tab.native
        .group(GROUP)
        .when(dress.pane, |tab| tab.min_h(height))
        .when(dress.at_edge, |tab| tab.ml(-px(KIT_TAB_BORDER_PX)))
        .prefix(layers(fill, height))
        .child(content)
        .when_some(tab.close, |tab, handler| {
            tab.suffix(close_button(handler, selected, disabled, &close_label))
        })
}

/// The tab's glyph, an opened item's key, its label and its status.
fn content_row(
    spec: TabSpec,
    normal: &State,
    tokens: &Tokens,
    mono: SharedString,
) -> impl gpui_kit::IntoElement + use<> {
    let disabled = spec.disabled;
    h_flex()
        .id("tab-content-row")
        .test_support()
        // Kit's anonymous prefix box clamps a negative child margin to zero.
        // Cancel its gap_1 here, also reducing the inner box's natural width.
        .ml(-gpui_kit::rems(PREFIX_GAP))
        .gap(px(CONTENT_GAP))
        .items_center()
        .text_color(normal.foreground)
        .font_weight(normal.weight)
        .when(disabled, |row| row.opacity(DISABLED_OPACITY))
        .when(spec.unread, |row| row.child(unread_dot(tokens)))
        .when_some(spec.icon, |row, icon| row.child(icon.size_3p5()))
        .when_some(spec.key, |row, key| row.child(key_mark(key, mono, tokens)))
        .map(|row| match spec.content {
            Some(content) => row.child(content),
            None => row.child(spec.label.clone()),
        })
        .when_some(spec.status, |row, status| {
            row.child(
                div()
                    .text_color(if disabled {
                        tokens.muted
                    } else {
                        status.colour
                    })
                    .child(status.visible),
            )
        })
}

/// An opened item's key: monospace and muted, before its title.
fn key_mark(key: SharedString, mono: SharedString, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .font_family(mono)
        .font_weight(FontWeight::NORMAL)
        .text_size(tokens.mono_size(KEY_MONO_PX))
        .text_color(tokens.muted)
        .child(key)
}

/// The tab's height: Kit's for the bar's size, or the strip's in a pane's
/// tab strip, which the tab's least height stretches it to.
fn tab_height(dress: &Dress) -> Pixels {
    let natural = match dress.size {
        Size::XSmall => XSMALL_HEIGHT,
        Size::Small => SMALL_HEIGHT,
        Size::Large => LARGE_HEIGHT,
        _ => DEFAULT_HEIGHT,
    };
    if dress.pane {
        px(natural).max(dress.strip)
    } else {
        px(natural)
    }
}

fn hover_rim(colour: gpui_kit::Hsla) -> impl gpui_kit::IntoElement {
    div()
        .absolute()
        .size_full()
        .opacity(TRANSPARENT)
        .group_hover(GROUP, |style| style.opacity(OPAQUE))
        .child(
            gpui_kit::canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    // Native Tab has no overlay slot or public hover style override.
                    // Its overflow mask sizes all four rim edges to the visible tab,
                    // rather than clipping a border on the 4096px background strip.
                    let bounds = window.content_mask().bounds;
                    window.paint_quad(gpui_kit::outline(
                        bounds,
                        colour,
                        gpui_kit::BorderStyle::default(),
                    ));
                },
            )
            .size_full(),
        )
}

/// Kit wraps a prefix in an unpositioned box, and GPUI places an absolute
/// element against its direct parent, so the fill hangs from a zero-width
/// column as tall as the tab. Kit paints the tab's borders and suffix after it.
fn layers(fill: gpui_kit::Div, height: Pixels) -> impl gpui_kit::IntoElement {
    div()
        .id("tab-layers")
        .test_support()
        .w_0()
        .h(height)
        .child(fill)
}

fn tab_label(label: &gpui_kit::SharedString) -> String {
    format!("Close {label}")
}

fn close_button(
    handler: CloseHandler,
    selected: bool,
    disabled: bool,
    label: &str,
) -> impl gpui_kit::IntoElement {
    IconButton::new("close-tab")
        .icon(IconName::X)
        .compact()
        .flat()
        .accessibility_label(label.to_owned())
        .map(|button| gpui_kit::component::Disableable::disabled(button, disabled))
        .map(|button| {
            if selected {
                button
            } else {
                button
                    .opacity(TRANSPARENT)
                    .group_hover(GROUP, |button| button.opacity(OPAQUE))
                    .focus_visible(|button| button.opacity(OPAQUE))
            }
        })
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            handler(event, window, cx);
        })
}
