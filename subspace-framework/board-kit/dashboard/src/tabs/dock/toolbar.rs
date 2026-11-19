//! Kit's toolbar policy, composed with shared actions and the shared menu host.

use super::{Group, toggles};
use crate::{Tokens, anchored, icon_button::IconButton};
use gpui_kit::component::{
    Sizable as _,
    button::{Button, ButtonVariants as _},
    dock::{PanelControl, PanelHandle, PanelView, TabGroupContext},
    h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Styled as _, Window};
use subspace_lucide::IconName;

pub(super) fn zoom_control(group: &TabGroupContext, cx: &App) -> Option<PanelControl> {
    let panel = group.active_panel()?;
    panel
        .zoomable(cx)
        .then(|| PanelHandle::of(panel).and_then(|handle| handle.zoom_control(cx)))
        .flatten()
}

pub(super) fn trailing(
    renderer: &Group,
    group: &TabGroupContext,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let handle = group.active_panel().and_then(PanelHandle::of);
    let suffix = handle.and_then(|handle| handle.title_suffix(window, cx));
    let buttons = handle
        .and_then(|handle| handle.toolbar_buttons(window, cx))
        .unwrap_or_default();
    let control = zoom_control(group, cx);
    let zoomed = group.is_zoomed();
    let toolbar_zoom = control.is_some_and(|control| control.toolbar_visible());
    let zoom = group.clone();
    let right = toggles::button(
        renderer,
        group,
        gpui_kit::component::dock::DockPlacement::Right,
        cx,
    );
    h_flex()
        .h_full()
        .flex_shrink_0()
        .px_2()
        .gap_1()
        .border_l_1()
        .border_b_1()
        .border_color(tokens.hair)
        .bg(tokens.surface)
        .children(suffix)
        // Panel::toolbar_buttons returns opaque, already-composed Buttons.
        // Kit exposes no icon/label getters; retain their native content and callbacks.
        .children(
            buttons
                .into_iter()
                .map(|button| button.xsmall().ghost().tab_stop(false)),
        )
        .when(zoomed || toolbar_zoom, |row| {
            row.child(
                // Like Kit, the toolbar stays off the keyboard tab order.
                IconButton::from_button(Button::new("dock-zoom").tab_stop(false))
                    .compact()
                    .icon(if zoomed {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .accessibility_label(if zoomed { "Zoom Out" } else { "Zoom In" })
                    .tooltip(if zoomed { "Zoom Out" } else { "Zoom In" })
                    .on_click(move |_, window, cx| zoom.toggle_zoom(window, cx)),
            )
        })
        .child(menu(group, control))
        .children(right)
        .into_any_element()
}

fn menu(group: &TabGroupContext, control: Option<PanelControl>) -> impl gpui_kit::IntoElement {
    let group = group.clone();
    let panel = group
        .active_panel()
        .and_then(PanelHandle::of)
        .map(|handle| handle.panel());
    let zoomed = group.is_zoomed();
    let menu_zoom = zoomed || control.is_some_and(|control| control.menu_visible());
    anchored::dropdown_menu(
        IconButton::from_button(Button::new("dock-menu").tab_stop(false))
            .compact()
            .icon(IconName::Ellipsis)
            .accessibility_label("Panel menu"),
        move |menu, window, cx| {
            let zoom = group.clone();
            let close = group.clone();
            let active = group.active_panel().map(|panel| panel.panel_id(cx));
            menu.when_some(panel.clone(), |menu, panel| {
                PanelView::dropdown_menu(panel.as_ref(), menu, window, cx)
            })
            .separator()
            .item(
                gpui_kit::component::menu::PopupMenuItem::new(if zoomed {
                    "Zoom Out"
                } else {
                    "Zoom In"
                })
                .disabled(!menu_zoom)
                .on_click(move |_, window, cx| zoom.toggle_zoom(window, cx)),
            )
            .when(group.is_closable(), |menu| {
                menu.separator().item(
                    gpui_kit::component::menu::PopupMenuItem::new("Close").on_click(
                        move |_, window, cx| {
                            if let Some(active) = active {
                                close.close(active, window, cx);
                            }
                        },
                    ),
                )
            })
        },
    )
}
