//! The inspector's tabs: a tab list in place of the title, the tab role and
//! selection on each tab, Close kept, choices by pointer and keyboard, and
//! no change for an inspector without tabs.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, ElementId, IntoElement, ParentElement as _, Render, Role, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::tabs::{TAB_ID, TAB_LIST_ID, kept};
use super::{INSPECTOR_MAX_TABS, Inspector, InspectorTab};

type Chosen = Rc<RefCell<Vec<usize>>>;

fn tab_id(index: usize) -> ElementId {
    (TAB_ID, index).into()
}

fn channel_tabs() -> Vec<InspectorTab> {
    vec![
        InspectorTab::new("Ship"),
        InspectorTab::new("Thread"),
        InspectorTab::new("People").count("5"),
    ]
}

/// An inspector with the channel's three tabs, Thread open, recording each
/// choice; or, without tabs, the plain inspector.
struct Dock {
    tabbed: bool,
    chosen: Chosen,
}

impl Render for Dock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let inspector = Inspector::new("tabs-inspector", "Channel details")
            .on_close("Close inspector", |_, _, _| {})
            .child(div().child("Body"));
        let chosen = self.chosen.clone();
        let inspector = if self.tabbed {
            inspector.tabs(channel_tabs(), 1, move |index, _, _, _| {
                chosen.borrow_mut().push(index);
            })
        } else {
            inspector
        };
        div().w(px(320.)).h(px(400.)).child(inspector)
    }
}

fn mount(tabbed: bool, cx: &mut TestAppContext) -> (Chosen, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let chosen = Chosen::default();
    let recorded = chosen.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Dock {
        tabbed,
        chosen: recorded,
    });
    cx.update(|window, cx| window.render_frame(cx));
    (chosen, cx)
}

#[test]
fn an_inspector_keeps_at_most_its_tab_limit_and_only_a_real_selection() {
    let many: Vec<_> = ["A", "B", "C", "D", "E"].map(InspectorTab::new).into();
    let (shown, selected) = kept(many.clone(), 1);
    assert_eq!(shown.len(), INSPECTOR_MAX_TABS);
    assert_eq!(selected, Some(1));
    assert_eq!(kept(many, 4).1, None, "a tab past the limit is not shown");
    assert_eq!(kept(channel_tabs(), 9).1, None);
}

#[test]
fn a_tab_is_named_by_its_label_then_its_count() {
    assert_eq!(InspectorTab::new("Thread").accessible_label(), "Thread");
    assert_eq!(
        InspectorTab::new("People").count("5").accessible_label(),
        "People, 5"
    );
}

#[gpui_kit::test]
fn the_tabs_take_the_titles_place_and_say_which_is_open(cx: &mut TestAppContext) {
    let (_, cx) = mount(true, cx);
    cx.update(|window, _| {
        assert!(window.try_find("workspace-inspector-title").is_none());
        let list = window.find(TAB_LIST_ID);
        assert_eq!(list.role(), Some(Role::TabList));
        assert_eq!(list.label(), Some("Channel details"));
        let thread = window.find(tab_id(1));
        assert_eq!(thread.role(), Some(Role::Tab));
        assert_eq!(thread.selected(), Some(true));
        assert_eq!(window.find(tab_id(0)).selected(), Some(false));
        assert_eq!(window.find(tab_id(2)).label(), Some("People, 5"));
        assert_eq!(
            window.find("workspace-inspector-body").role(),
            Some(Role::TabPanel)
        );
        assert_eq!(
            window.find("workspace-inspector-close").role(),
            Some(Role::Button),
            "Close stays in the header"
        );
    });
}

#[gpui_kit::test]
fn a_click_or_the_keyboard_chooses_a_tab(cx: &mut TestAppContext) {
    let (chosen, cx) = mount(true, cx);
    cx.update(|window, cx| {
        window.click(tab_id(2), cx);
        assert_eq!(*chosen.borrow(), vec![2]);
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(window.find(tab_id(0)).focused(), Some(true));
        window.press("space", cx);
        assert_eq!(*chosen.borrow(), vec![2, 0]);
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(window.find(tab_id(1)).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(*chosen.borrow(), vec![2, 0, 1]);
    });
}

#[gpui_kit::test]
fn an_inspector_without_tabs_keeps_its_title(cx: &mut TestAppContext) {
    let (_, cx) = mount(false, cx);
    cx.update(|window, _| {
        let title = window.find("workspace-inspector-title");
        assert_eq!(title.role(), Some(Role::Heading));
        assert_eq!(title.label(), Some("Channel details"));
        assert!(window.try_find(TAB_LIST_ID).is_none());
        assert_ne!(
            window.find("workspace-inspector-body").role(),
            Some(Role::TabPanel)
        );
    });
}
