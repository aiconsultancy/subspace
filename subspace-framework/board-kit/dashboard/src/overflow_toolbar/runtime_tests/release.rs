//! What happens to the menu, and to focus, as the row changes: focus left on
//! the menu's button moves to the toolbar when the button goes, and a long
//! menu scrolls.

use super::{Harness, anchor_window, settle};
use crate::overflow_toolbar::view::TOOLBAR_LABEL;
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use gpui_kit::component::{button::Button, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Role, ScrollDelta, Styled as _, TestAppContext, Window, point, px, size,
};
use std::{cell::Cell, rc::Rc};

/// `count` 30 px buttons, each with a menu entry named after it that
/// records its index in `ran` when chosen.
fn buttons(count: usize, ran: Rc<Cell<Option<usize>>>) -> Vec<OverflowUnit> {
    (0..count)
        .map(|index| {
            let ran = ran.clone();
            OverflowUnit::inline(
                format!("unit-{index}"),
                OverflowUnit::UNMEASURED_WIDTH,
                move |_, _| Button::new(format!("inline-{index}")).w(px(30.)).label("X"),
                vec![PopupEntry::new(move || {
                    let ran = ran.clone();
                    PopupMenuItem::new(format!("Action {index}"))
                        .on_click(move |_, _, _| ran.set(Some(index)))
                })],
            )
        })
        .collect()
}

/// The labels of the open menu's entries.
fn menu_labels(window: &mut Window) -> Vec<String> {
    let menu = window.within("popup-menu");
    (0usize..64)
        .filter_map(|index| menu.try_find(index))
        .filter_map(|item| item.label().map(str::to_owned))
        .collect()
}

#[gpui_kit::test]
fn focus_moves_to_the_toolbar_when_its_menu_closes_because_everything_fits(
    cx: &mut TestAppContext,
) {
    let (probe, cx) = anchor_window(cx);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    settle(cx);
    probe.root.update(cx, |root, cx| {
        root.width = 150.;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(window.try_find("overflow-toolbar-more").is_none());
        let toolbar = probe.root.read(cx).toolbar.clone();
        let focus = toolbar.read(cx).focus.clone();
        assert!(
            focus.is_some_and(|focus| focus.is_focused(window)),
            "focus is on the toolbar, not on the button that went"
        );
        window.render_frame(cx);
        let toolbar = window.find("overflow-toolbar-clip");
        assert_eq!(toolbar.focused(), Some(true), "the container holds focus");
        assert_eq!(
            toolbar.role(),
            Some(Role::Toolbar),
            "it says it is a toolbar"
        );
        assert_eq!(toolbar.label(), Some(TOOLBAR_LABEL));
        let toolbars: Vec<_> = gpui_kit::base::test_support::snapshots(window)
            .into_iter()
            .filter(|element| element.role() == Some(Role::Toolbar))
            .collect();
        assert_eq!(
            toolbars.len(),
            1,
            "one toolbar node, the container: {toolbars:?}"
        );
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("inline-0").focused(),
            Some(true),
            "Tab goes on to the toolbar's first control"
        );
    });
}

#[gpui_kit::test]
fn a_long_overflow_menu_scrolls_within_the_window(cx: &mut TestAppContext) {
    const WINDOW_HEIGHT: f32 = 400.;
    cx.update(gpui_kit::init);
    let ran = Rc::new(Cell::new(None));
    let toolbar = cx.new(|_| OverflowToolbar::new(buttons(40, ran.clone())));
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 70.,
        left: 0.,
    });
    cx.simulate_resize(size(px(400.), px(WINDOW_HEIGHT)));
    settle(cx);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    settle(cx);
    cx.update(|window, cx| {
        let menu = window.find("popup-menu").bounds();
        assert!(
            menu.bottom() <= px(WINDOW_HEIGHT),
            "the menu {menu:?} ends inside the window"
        );
        let labels = menu_labels(window);
        assert_eq!(labels.len(), 39, "every hidden unit has its entry");
        assert_eq!(labels.last().map(String::as_str), Some("Action 39"));
        // Scrolled to its end, the last entry can be chosen: a menu that
        // only clipped its entries would leave it out of sight.
        let down = ScrollDelta::Pixels(point(px(0.), px(-10_000.)));
        window.scroll("popup-menu", down, cx);
        window.within("popup-menu").click(labels.len() - 1, cx);
    });
    assert_eq!(ran.get(), Some(39), "the last entry's command ran");
}
