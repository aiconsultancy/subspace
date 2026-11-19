//! Appearance only; native tabs and buttons retain activation and focus.

use super::{CONTENT_GAP, Tab};
use crate::{Tokens, icon_button::IconButton, tokens::ButtonFinish};
use gpui_kit::component::{Size, h_flex, tab::Tab as KitTab};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use std::rc::Rc;
use subspace_lucide::IconName;

pub(super) type CloseHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
const TRANSPARENT: f32 = 0.;
const OPAQUE: f32 = 1.;
const DISABLED_OPACITY: f32 = 0.5;
/// Kit inserts gap_1 after the decorative prefix (in rems).
const PREFIX_GAP: f32 = 0.25;
const ACCENT_EDGE: f32 = 2.;
const DOT_SIZE: f32 = 5.;
const GROUP: &str = "subspace-resource-tab";
// Kit's TabVariant::Tab heights are private. Keep this mapping aligned with
// its public Sizable API; render tests check the small and default heights.
const SMALL_HEIGHT: f32 = 24.;
const DEFAULT_HEIGHT: f32 = 32.;
const XSMALL_HEIGHT: f32 = 20.;
const LARGE_HEIGHT: f32 = 36.;
/// Wider than any tab; the tab's own `overflow_hidden` clips the layers.
const LAYER_REACH: f32 = 4096.;

#[derive(Debug, PartialEq)]
pub(super) struct State {
    pub(super) foreground: gpui_kit::Hsla,
    pub(super) background: gpui_kit::Hsla,
    pub(super) edge: gpui_kit::Hsla,
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
        edge: if selected && !disabled {
            tokens.accent
        } else {
            tokens.accent.opacity(TRANSPARENT)
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

pub(super) fn tab(
    tab: Tab,
    selected: bool,
    tokens: &Tokens,
    surface: gpui_kit::Hsla,
    size: Size,
) -> KitTab {
    let disabled = tab.spec.disabled;
    let normal = state(selected, false, disabled, tokens, surface);
    let hover = state(selected, true, disabled, tokens, surface);
    let glass_hover = tokens.controls.toolbar_look.hover == ButtonFinish::Glass;
    let close_label = tab_label(&tab.spec.label);
    let content = h_flex()
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
        .when(tab.spec.unread, |row| row.child(unread_dot(tokens)))
        .when_some(tab.spec.icon, |row, icon| row.child(icon.size_3p5()))
        .map(|row| match tab.spec.content {
            Some(content) => row.child(content),
            None => row.child(tab.spec.label.clone()),
        })
        .when_some(tab.spec.status, |row, status| {
            row.child(
                div()
                    .text_color(if disabled {
                        tokens.muted
                    } else {
                        status.colour
                    })
                    .child(status.visible),
            )
        });
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
    tab.native
        .group(GROUP)
        .prefix(layers(
            fill,
            (selected && !disabled).then_some(normal.edge),
            size,
        ))
        .child(content)
        .when_some(tab.close, |tab, handler| {
            tab.suffix(close_button(handler, selected, disabled, &close_label))
        })
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
/// element against its direct parent, so the layers hang from a zero-width
/// column as tall as the tab. Kit paints the tab's borders and suffix after it.
fn layers(
    fill: gpui_kit::Div,
    edge: Option<gpui_kit::Hsla>,
    size: Size,
) -> impl gpui_kit::IntoElement {
    let height = match size {
        Size::XSmall => XSMALL_HEIGHT,
        Size::Small => SMALL_HEIGHT,
        Size::Large => LARGE_HEIGHT,
        _ => DEFAULT_HEIGHT,
    };
    div()
        .id("tab-layers")
        .test_support()
        .w_0()
        .h(px(height))
        .child(fill)
        .when_some(edge, |column, edge| {
            column.child(
                div()
                    .id("tab-selected-edge")
                    .test_support()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h(px(ACCENT_EDGE))
                    .w(px(LAYER_REACH))
                    .bg(edge),
            )
        })
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
