//! Dock collapse controls through Kit's public layout and toggle APIs.

use super::Group;
use crate::icon_button::IconButton;
use gpui_kit::component::{
    dock::{DockPlacement, NodeId, PaneNode, PaneRef, TabGroupContext},
    h_flex,
};
use gpui_kit::{AnyElement, App, Axis, IntoElement as _, ParentElement as _, Styled as _};
use subspace_lucide::IconName;

pub(super) fn leading(
    renderer: &Group,
    group: &TabGroupContext,
    cx: &mut App,
) -> Option<AnyElement> {
    let left = button(renderer, group, DockPlacement::Left, cx);
    let bottom = button(renderer, group, DockPlacement::Bottom, cx);
    if left.is_none() && bottom.is_none() {
        return None;
    }
    let tokens = crate::Tokens::current(cx);
    Some(
        h_flex()
            .h_full()
            .px_2()
            .gap_1()
            .border_r_1()
            .border_b_1()
            .border_color(tokens.hair)
            .bg(tokens.surface)
            .children(left)
            .children(bottom)
            .into_any_element(),
    )
}

pub(super) fn button(
    renderer: &Group,
    group: &TabGroupContext,
    placement: DockPlacement,
    cx: &mut App,
) -> Option<IconButton> {
    if group.is_zoomed() || !renderer.skin.is_toggle_button_visible() {
        return None;
    }
    let area = renderer.area.upgrade()?;
    let dock = area.read(cx);
    if !dock.is_dock_collapsible(placement) {
        return None;
    }
    let (container, right) = match placement {
        DockPlacement::Left => (DockPlacement::Center, false),
        DockPlacement::Right => (DockPlacement::Center, true),
        DockPlacement::Bottom => (DockPlacement::Bottom, false),
        DockPlacement::Center => return None,
    };
    let designated = dock
        .layout(container)
        .and_then(|tree| corner(tree.root(), right));
    if designated != Some(group.node()) {
        return None;
    }
    let open = dock.is_dock_open(placement);
    let icon = match (placement, open) {
        (DockPlacement::Left, true) => IconName::PanelLeft,
        (DockPlacement::Left, false) => IconName::PanelLeftOpen,
        (DockPlacement::Right, true) => IconName::PanelRight,
        (DockPlacement::Right, false) => IconName::PanelRightOpen,
        (DockPlacement::Bottom, true) => IconName::PanelBottom,
        (DockPlacement::Bottom, false) => IconName::PanelBottomOpen,
        _ => return None,
    };
    let label = format!(
        "{} {placement:?} dock",
        if open { "Collapse" } else { "Expand" }
    );
    Some(
        IconButton::new(gpui_kit::SharedString::from(format!(
            "dock-toggle-{placement:?}"
        )))
        .compact()
        .icon(icon)
        .accessibility_label(label.clone())
        .tooltip(label)
        .on_click(move |_, window, cx| {
            // Kit's toggle_dock_by_user and DockToggleEvent constructor are private.
            // The public toggle preserves layout events and collapse constraints.
            area.update(cx, |area, cx| area.toggle_dock(placement, window, cx));
        }),
    )
}

fn corner(node: &PaneNode, right: bool) -> Option<NodeId> {
    match node.kind() {
        PaneRef::Tabs { .. } => Some(node.id()),
        PaneRef::Split { axis, children, .. } => {
            let child = if right && axis == Axis::Horizontal {
                children.last()
            } else {
                children.first()
            };
            child.and_then(|node| corner(node, right))
        }
    }
}
