//! Pointer activation of a pull request chip: a click selects it in the
//! running PRs Space, or opens the link as written with a toast; Copy keeps
//! the authored text and Copy link the original URL; a drag only selects.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::base::TextSelection;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ClipboardItem, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::{DashboardEvent, ToastTone};

use super::{Prs, URL, chips, open_board};
use crate::Irc;

/// What the board asked the shell: a navigation's URL, or a toast's text
/// and whether it is information.
#[derive(Default)]
pub(super) struct Asked {
    pub navigated: Option<String>,
    pub toast: Option<(String, bool)>,
}

pub(super) fn watch(
    board: &Entity<Irc>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Asked>>, gpui_kit::Subscription) {
    let asked = Rc::new(RefCell::new(Asked::default()));
    let received = asked.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(board, move |_, event: &DashboardEvent, _| match event {
            DashboardEvent::Navigate(link) => {
                received.borrow_mut().navigated = Some(link.url.to_string());
            }
            DashboardEvent::Toast(toast) => {
                received.borrow_mut().toast =
                    Some((toast.text.to_string(), toast.tone == ToastTone::Info));
            }
            _ => {}
        })
    });
    (asked, subscription)
}

fn click_chip(cx: &mut VisualTestContext) {
    let target = chips(cx).into_iter().next().expect("a pull request chip");
    cx.update(|window, cx| window.click(gpui_kit::SharedString::from(target), cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_click_selects_the_pull_request_in_the_running_prs_space(cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, Prs::Running, URL);
    let (asked, _subscription) = watch(&board, cx);
    click_chip(cx);
    assert_eq!(
        asked.borrow().navigated.as_deref(),
        Some("https://github.com/Octo-Org/API/pull/421")
    );
    assert!(asked.borrow().toast.is_none());
    assert!(cx.opened_url().is_none());
}

#[gpui_kit::test]
fn without_a_running_prs_space_a_click_opens_the_link_as_written(cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, Prs::Absent, URL);
    let (asked, _subscription) = watch(&board, cx);
    click_chip(cx);
    assert!(asked.borrow().navigated.is_none(), "nothing is enabled");
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    assert_eq!(
        asked.borrow().toast,
        Some(("PRs is unavailable. Opening in browser.".to_owned(), true))
    );
}

#[gpui_kit::test]
fn a_marker_without_prs_opens_its_canonical_url(cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, Prs::Absent, "[PR:octo-org/api#421]");
    let (_asked, _subscription) = watch(&board, cx);
    click_chip(cx);
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/octo-org/api/pull/421")
    );
}

/// Right-clicks the chip and chooses menu item `index`, `label`; returns
/// what the clipboard holds then.
fn copy_with(index: usize, label: &str, text: &str, cx: &mut TestAppContext) -> Option<String> {
    let (_, _, _, cx) = open_board(cx, Prs::Running, text);
    let target = chips(cx).into_iter().next().expect("a pull request chip");
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("unchanged".into()));
        window.right_click(gpui_kit::SharedString::from(target), cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert_eq!(menu.find(index).label(), Some(label));
        menu.click(index, cx);
    });
    assert!(cx.opened_url().is_none(), "a secondary click never opens");
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui_kit::test]
fn copy_keeps_the_text_as_written(cx: &mut TestAppContext) {
    assert_eq!(
        copy_with(0, "Copy", "[PR:Octo-Org/API#421]", cx).as_deref(),
        Some("[PR:Octo-Org/API#421]")
    );
}

#[gpui_kit::test]
fn copy_link_copies_the_original_url(cx: &mut TestAppContext) {
    assert_eq!(copy_with(1, "Copy link", URL, cx).as_deref(), Some(URL));
}

#[gpui_kit::test]
fn copy_link_of_a_marker_copies_its_canonical_url(cx: &mut TestAppContext) {
    assert_eq!(
        copy_with(1, "Copy link", "[PR:octo-org/api#421]", cx).as_deref(),
        Some("https://github.com/octo-org/api/pull/421")
    );
}

#[gpui_kit::test]
fn a_drag_onto_a_chip_selects_without_opening_it(cx: &mut TestAppContext) {
    let text = format!("alpha bravo {URL}");
    let (board, _, _, cx) = open_board(cx, Prs::Running, &text);
    let (asked, _subscription) = watch(&board, cx);
    let target = chips(cx).into_iter().next().expect("a pull request chip");
    let chip = cx.update(|window, _| window.find(gpui_kit::SharedString::from(target)).bounds());
    let from = chip.origin - gpui_kit::point(gpui_kit::px(60.), gpui_kit::px(0.))
        + gpui_kit::point(gpui_kit::px(0.), chip.size.height / 2.);
    let to = chip.center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    let selected = cx.update(|window, cx| {
        let _ = window.draw(cx);
        TextSelection::selected_text(window, cx)
    });
    assert!(!selected.is_empty(), "the drag selects text");
    assert!(asked.borrow().navigated.is_none());
    assert!(cx.opened_url().is_none());
}
