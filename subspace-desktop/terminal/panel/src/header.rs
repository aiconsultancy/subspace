//! The panel header's layout buttons: move left, to the bottom or right,
//! full window width, maximize or restore, and hide. Each layout button is a
//! glyph whose text shows only while the shell's label choice is on.

use gpui_kit::component::h_flex;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Styled as _, div,
};
use subspace_framework::dashboard::action::{self, Intent};
use subspace_framework::dashboard::anchored::AnchoredContextMenuExt as _;
use subspace_framework::dashboard::dock_panel::{
    BottomSpan, DockPanelEvent, PanelLayout, Placement,
};
use subspace_framework::lucide::IconName;

use crate::panel::TerminalPanel;

/// One layout button: what it shows, whether it is latched and what it asks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayoutButton {
    pub(crate) id: &'static str,
    pub(crate) icon: IconName,
    pub(crate) name: &'static str,
    pub(crate) latched: bool,
    pub(crate) event: DockPanelEvent,
}

/// The layout buttons for `layout`, in header order. Full window width is
/// offered only at the bottom.
pub(crate) fn layout_buttons(layout: PanelLayout) -> Vec<LayoutButton> {
    let full_width = layout.span == BottomSpan::FullWidth;
    let mut buttons: Vec<_> = Placement::ALL
        .into_iter()
        .map(|placement| placement_button(placement, layout.placement == placement))
        .collect();
    if layout.placement == Placement::Bottom {
        buttons.push(LayoutButton {
            id: "terminal-full-width",
            icon: IconName::MoveHorizontal,
            name: "Full window width",
            latched: full_width,
            event: DockPanelEvent::SetBottomSpan(if full_width {
                BottomSpan::BesideChat
            } else {
                BottomSpan::FullWidth
            }),
        });
    }
    buttons.push(maximize_button(layout.maximized));
    buttons
}

// qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
fn placement_button(placement: Placement, latched: bool) -> LayoutButton {
    let (id, icon, name) = match placement {
        Placement::Left => ("terminal-move-left", IconName::PanelLeft, "Move left"),
        Placement::Bottom => (
            "terminal-move-bottom",
            IconName::PanelBottom,
            "Move to bottom",
        ),
        Placement::Right => ("terminal-move-right", IconName::PanelRight, "Move right"),
    };
    LayoutButton {
        id,
        icon,
        name,
        latched,
        event: DockPanelEvent::Move(placement),
    }
}

fn maximize_button(maximized: bool) -> LayoutButton {
    let (icon, name) = if maximized {
        (IconName::Minimize2, "Restore")
    } else {
        (IconName::Maximize2, "Maximize")
    };
    LayoutButton {
        id: "terminal-maximize",
        icon,
        name,
        latched: maximized,
        event: DockPanelEvent::ToggleMaximized,
    }
}

impl TerminalPanel {
    /// The tab bar, with the label choice on its context menu.
    pub(crate) fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let labels = self.layout.labels;
        let panel = cx.weak_entity();
        div()
            .id("terminal-header")
            .child(self.render_tabs(cx))
            .anchored_context_menu(move |menu, _, _| {
                let panel = panel.clone();
                menu.item(
                    PopupMenuItem::new("Show button labels")
                        .checked(labels)
                        .on_click(move |_, _, cx| {
                            _ = panel.update(cx, |_, cx| {
                                cx.emit(DockPanelEvent::SetLabels(!labels));
                            });
                        }),
                )
            })
            .into_any_element()
    }

    pub(crate) fn layout_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let labels = self.layout.labels;
        let buttons = layout_buttons(self.layout)
            .into_iter()
            .map(|button| layout_button(button, labels, cx));
        h_flex()
            .gap_1()
            .children(buttons)
            .child(
                action::icon_button("terminal-hide", Intent::Quiet, IconName::X, hide_name())
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DockPanelEvent::Hide))),
            )
            .into_any_element()
    }
}

fn layout_button(
    button: LayoutButton,
    labels: bool,
    cx: &mut Context<TerminalPanel>,
) -> AnyElement {
    let event = button.event;
    action::toolbar_button(
        button.id,
        button.icon,
        button.name,
        (Intent::Quiet, labels),
        cx,
    )
    .latched(button.latched)
    .on_click(cx.listener(move |_, _, _, cx| cx.emit(event)))
    .into_any_element()
}

fn hide_name() -> &'static str {
    "Hide terminal (Ctrl+`)"
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt as _;
    use subspace_framework::dashboard::dock_panel::{
        BottomSpan, DockPanel as _, DockPanelEvent, PanelLayout, Placement,
    };

    use super::layout_buttons;
    use crate::panel::TerminalPanel;

    #[gpui_kit::test]
    fn the_label_choice_shows_text_on_every_layout_button(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (panel, cx) = cx.add_window_view(|_, cx| TerminalPanel::new(cx));
        for labels in [false, true] {
            panel.update(cx, |panel, cx| {
                panel.set_layout(
                    PanelLayout {
                        labels,
                        ..PanelLayout::default()
                    },
                    cx,
                );
            });
            cx.update(|window, cx| {
                window.render_frame(cx);
                for id in [
                    "terminal-move-left",
                    "terminal-move-bottom",
                    "terminal-move-right",
                    "terminal-full-width",
                    "terminal-maximize",
                ] {
                    let text = window.within(id).try_find("icon-button-label").is_some();
                    assert_eq!(text, labels, "{id}");
                }
                assert!(window.try_find("terminal-hide").is_some());
            });
        }
    }

    #[test]
    fn the_current_placement_is_latched_and_full_width_shows_only_at_the_bottom() {
        let bottom = layout_buttons(PanelLayout::default());
        let ids: Vec<_> = bottom.iter().map(|button| button.id).collect();
        assert_eq!(
            ids,
            [
                "terminal-move-left",
                "terminal-move-bottom",
                "terminal-move-right",
                "terminal-full-width",
                "terminal-maximize"
            ]
        );
        let latched: Vec<_> = bottom.iter().map(|button| button.latched).collect();
        assert_eq!(latched, [false, true, false, false, false]);
        let left = layout_buttons(PanelLayout {
            placement: Placement::Left,
            ..PanelLayout::default()
        });
        assert!(left.iter().all(|button| button.id != "terminal-full-width"));
        assert!(left[0].latched);
    }

    #[test]
    fn full_width_and_maximize_toggle_and_name_what_happens() {
        let layout = PanelLayout {
            span: BottomSpan::FullWidth,
            maximized: true,
            ..PanelLayout::default()
        };
        let buttons = layout_buttons(layout);
        let full = buttons[3];
        assert!(full.latched);
        assert_eq!(
            full.event,
            DockPanelEvent::SetBottomSpan(BottomSpan::BesideChat)
        );
        let maximize = buttons[4];
        assert_eq!(maximize.name, "Restore");
        assert!(maximize.latched);
        assert_eq!(maximize.event, DockPanelEvent::ToggleMaximized);
        assert_eq!(layout_buttons(PanelLayout::default())[4].name, "Maximize");
    }
}
