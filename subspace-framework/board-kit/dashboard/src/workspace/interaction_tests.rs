//! Navigator rows: tree items that say which is selected, chosen by pointer
//! or keyboard, and a menu that selects only a row not yet selected.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, Role,
    Styled as _, TestAppContext, VisualTestContext, Window, div,
};
use subspace_lucide::IconName;

use super::{NavRow, Navigator};
use crate::row_menu::{GroupedMenu, MenuCommand, MenuGroup};
use crate::{Activate, DASHBOARD_CONTEXT, SelectNext, SelectPrevious};

type Clicks = Rc<RefCell<Vec<&'static str>>>;

/// A navigator with Alpha selected above Beta, inside a list board's key
/// context, recording each row's clicks and each menu it builds.
struct Rows {
    clicks: Clicks,
    menus: Rc<Cell<usize>>,
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().key_context(DASHBOARD_CONTEXT).child(
            Navigator::new("Fixture")
                .child(self.row("nav-a", "Alpha", true))
                .child(self.row("nav-b", "Beta", false)),
        )
    }
}

impl Rows {
    fn row(&self, key: &'static str, label: &'static str, selected: bool) -> NavRow {
        let clicks = self.clicks.clone();
        let menus = self.menus.clone();
        NavRow::new(key, label)
            .selected(selected)
            .on_click(move |_, _, _| clicks.borrow_mut().push(key))
            .menu(move |_, _| {
                menus.set(menus.get() + 1);
                let open =
                    MenuCommand::new(MenuGroup::Open, "Open", IconName::ArrowRight, |_, _| {});
                GroupedMenu::new().with(open)
            })
    }
}

fn mount(cx: &mut TestAppContext) -> (Clicks, Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let context = Some(DASHBOARD_CONTEXT);
        cx.bind_keys([
            KeyBinding::new("down", SelectNext, context),
            KeyBinding::new("up", SelectPrevious, context),
            KeyBinding::new("enter", Activate, context),
        ]);
    });
    let clicks = Clicks::default();
    let menus = Rc::new(Cell::new(0));
    let rows = Rows {
        clicks: clicks.clone(),
        menus: menus.clone(),
    };
    let (_, cx) = cx.add_window_view(|_, _| rows);
    cx.update(|window, cx| window.render_frame(cx));
    (clicks, menus, cx)
}

#[gpui_kit::test]
fn rows_are_tree_items_that_say_which_is_selected(cx: &mut TestAppContext) {
    let (_, _, cx) = mount(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("workspace-navigator-body").role(),
            Some(Role::Tree)
        );
        let alpha = window.find("nav-a");
        assert_eq!(alpha.role(), Some(Role::TreeItem));
        assert_eq!(alpha.label(), Some("Alpha"));
        assert_eq!(alpha.selected(), Some(true));
        assert_eq!(window.find("nav-b").selected(), Some(false));
    });
}

#[gpui_kit::test]
fn the_list_keys_move_between_rows_and_enter_or_space_chooses_one(cx: &mut TestAppContext) {
    let (clicks, _, cx) = mount(cx);
    cx.update(|window, cx| {
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(window.find("nav-a").focused(), Some(true));
        window.press("down", cx);
        assert_eq!(window.find("nav-b").focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(
            *clicks.borrow(),
            vec!["nav-b"],
            "Enter chooses the row once"
        );
        window.press("space", cx);
        assert_eq!(*clicks.borrow(), vec!["nav-b", "nav-b"]);
        window.press("up", cx);
        assert_eq!(window.find("nav-a").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn a_click_chooses_a_row_without_taking_the_keyboard_focus(cx: &mut TestAppContext) {
    let (clicks, _, cx) = mount(cx);
    cx.update(|window, cx| {
        window.click("nav-b", cx);
        assert_eq!(*clicks.borrow(), vec!["nav-b"]);
        assert_eq!(window.find("nav-b").focused(), Some(false));
    });
}

#[gpui_kit::test]
fn the_menu_of_the_selected_row_does_not_choose_it_again(cx: &mut TestAppContext) {
    let (clicks, menus, cx) = mount(cx);
    cx.update(|window, cx| window.right_click("nav-a", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    assert!(menus.get() >= 1, "the menu opened");
    assert!(clicks.borrow().is_empty());
}

#[gpui_kit::test]
fn the_menu_of_another_row_chooses_it_first(cx: &mut TestAppContext) {
    let (clicks, menus, cx) = mount(cx);
    cx.update(|window, cx| window.right_click("nav-b", cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
    assert!(menus.get() >= 1, "the menu opened");
    let clicks = clicks.borrow();
    assert!(!clicks.is_empty() && clicks.iter().all(|key| *key == "nav-b"));
}
