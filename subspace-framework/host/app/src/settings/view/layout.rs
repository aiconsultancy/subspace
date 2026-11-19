//! Layout controls share the existing Settings draft and Save/Cancel boundary.

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, InteractiveElement as _, IntoElement as _,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::toggle::toggle_row;
use subspace_dashboard::{Tokens, anchored};
use subspace_lucide::IconName;

use super::{Settings, labelled, note, page_frame};
use crate::settings::board_order::{BoardDrag, DragPreview};
use crate::toolbar_labels::ToolbarControl;

pub(super) fn render(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let body = v_flex()
        .gap_4()
        .child(board_order(settings, tokens, cx))
        .child(board_navigation(settings, tokens, cx))
        .children(terminal_layout(settings, tokens, cx))
        .child(toolbar_menu_row(settings, tokens, cx))
        .child(toolbar_label_group(settings, tokens, cx));
    page_frame(
        "Arrange Spaces, the terminal and application controls.",
        body,
        tokens,
        cx,
    )
    .into_any_element()
}

/// The docked terminal's placement, bottom width and button text, for a
/// product with a terminal.
fn terminal_layout(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Option<Div> {
    let terminal = settings.terminal.as_ref()?;
    Some(
        v_flex()
            .gap_2()
            .child(labelled(
                "Terminal placement",
                anchored::select(&terminal.placement)
                    .id("terminal-placement")
                    .accessibility_label("Terminal placement"),
            ))
            .child(labelled(
                "Bottom terminal width",
                anchored::select(&terminal.span)
                    .id("terminal-bottom-span")
                    .accessibility_label("Bottom terminal width"),
            ))
            .child(toggle_row(
                "Show text on terminal layout buttons",
                subspace_dashboard::toggle::switch("terminal-button-labels", cx)
                    .checked(terminal.labels)
                    .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                        settings.set_terminal_labels(*enabled, cx);
                    })),
            ))
            .child(note(tokens, "A bottom terminal sits beside the chat, which keeps the full window height, or spans the full window width below it. The buttons in the terminal's header move it at once; right-click the header to show their text.")),
    )
}

fn board_navigation(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    labelled(
        "Space navigation",
        anchored::select(&settings.board_navigation.mode)
            .id("board-navigation-mode")
            .accessibility_label("Space navigation")
            .search_placeholder("Find a navigation layout…"),
    )
    .gap_2()
    .child(
        toggle_row("Use icon-only Space navigation", subspace_dashboard::toggle::switch("collapsed-board-navigation", cx)
            .checked(settings.board_navigation.collapsed)
            .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                settings.board_navigation.collapsed = *enabled;
                cx.notify();
            }))),
    )
    .child(note(tokens, "Choose horizontal tabs or a vertical Space navigation rail. The icon-only option applies to the vertical rail."))
}

fn board_order(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let rows = (0..settings.boards.len()).map(|index| board_order_row(settings, index, tokens, cx));
    v_flex()
        .gap_1()
        .child("Space order")
        .children(rows)
        .child(note(tokens, "Drag a Space or use its move buttons, then Save. You can also drag the top tabs to save their order immediately."))
}

fn move_button(
    settings: &Settings,
    index: usize,
    up: bool,
    cx: &mut Context<Settings>,
) -> IconButton {
    let board = &settings.boards[index];
    let id = board.id;
    let direction = if up { "up" } else { "down" };
    IconButton::new(format!("board-order-{direction}-{id}"))
        .ghost()
        .icon(if up {
            IconName::ChevronUp
        } else {
            IconName::ChevronDown
        })
        .accessibility_label(format!("Move {} {direction}", board.title))
        .tooltip(format!("Move {direction}"))
        .disabled(if up {
            index == 0
        } else {
            index + 1 == settings.boards.len()
        })
        .on_click(cx.listener(move |settings, _, _, cx| {
            settings.move_board(id, if up { -1 } else { 1 }, cx);
        }))
}

fn board_order_row(
    settings: &Settings,
    index: usize,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let board = &settings.boards[index];
    let id = board.id;
    let order: Vec<_> = settings.boards.iter().map(|board| board.id).collect();
    let owner = cx.entity_id();
    let accent = tokens.accent;
    h_flex()
        .id(format!("board-order-row-{id}"))
        .test_support()
        .gap_2()
        .px_2()
        .py_1()
        .bg(tokens.pane)
        .border_y_1()
        .border_color(tokens.pane)
        .child(IconName::GripVertical)
        .child(div().flex_1().min_w_0().child(board.title.clone()))
        .child(move_button(settings, index, true, cx))
        .child(move_button(settings, index, false, cx))
        .on_drag(
            BoardDrag {
                owner,
                id,
                title: board.title.clone().into(),
            },
            |drag, _, _, cx| cx.new(|_| DragPreview(drag.title.clone())),
        )
        .drag_over::<BoardDrag>(move |style, drag, _, _| {
            if drag.owner != owner || drag.id == id {
                return style;
            }
            match order.iter().position(|id| *id == drag.id) {
                Some(source) if source > index => {
                    style.border_t_2().border_b_0().border_color(accent)
                }
                Some(_) => style.border_t_0().border_b_2().border_color(accent),
                None => style,
            }
        })
        .on_drop(cx.listener(move |settings, drag: &BoardDrag, _, cx| {
            settings.drop_board(drag, id, cx);
        }))
        .into_any_element()
}

fn toolbar_menu_row(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    v_flex()
        .gap_1()
        .child(toggle_row(
            "Collapse application toolbar into a menu",
            subspace_dashboard::toggle::switch("collapse-toolbar", cx)
                .checked(settings.toolbar.menu)
                .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                    settings.set_toolbar_menu(*enabled, cx);
                })),
        ))
        .child(note(tokens, "Replaces the toolbar controls at the top right with one menu that lists each control by name. The status bar is unchanged."))
}

fn toolbar_label_group(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let collapsed = settings.toolbar.menu;
    let terminal = settings.terminal.is_some();
    let rows = ToolbarControl::ALL
        .into_iter()
        .enumerate()
        .filter(|(_, control)| control.offered(terminal, cx))
        .map(|(index, control)| {
            toggle_row(
                control.label(),
                subspace_dashboard::toggle::switch(("toolbar-label", index), cx)
                    .checked(settings.toolbar.labels[index])
                    .disabled(collapsed)
                    .on_click(cx.listener(move |settings, enabled: &bool, _, cx| {
                        settings.set_toolbar_label(index, *enabled, cx);
                    })),
            )
        });
    let guidance = if collapsed {
        "Labels don't apply while the toolbar is collapsed into a menu. The menu always shows each control's name."
    } else {
        "Shows the label next to each toolbar icon. Right-click a toolbar control to toggle its label without opening Settings."
    };
    v_flex()
        .gap_1()
        .child("Application toolbar labels")
        .child(
            v_flex()
                .gap_1()
                .when(collapsed, |rows| rows.text_color(tokens.muted))
                .children(rows),
        )
        .child(note(tokens, guidance))
}
