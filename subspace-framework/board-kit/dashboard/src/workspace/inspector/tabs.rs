//! The inspector's optional tabs: a segmented tab list in place of the
//! header's title, each tab switching what the inspector says about the same
//! selection, such as a channel's ship, its thread and its people.

use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Role, SharedString, StatefulInteractiveElement as _, TestSupportExt as _, Window,
};

use super::super::row_parts::count_mark;
use crate::segmented;
use crate::tokens::Tokens;

/// The most tabs an inspector shows. More would not fit a narrow dock, and
/// an inspector with more subjects than this is a second navigator.
pub const INSPECTOR_MAX_TABS: usize = 3;

/// The tab list's id; each tab is `("workspace-inspector-tab", index)`.
pub(super) const TAB_LIST_ID: &str = "workspace-inspector-tabs";
pub(super) const TAB_ID: &str = "workspace-inspector-tab";

pub(super) type SelectHandler = Rc<dyn Fn(usize, &ClickEvent, &mut Window, &mut App)>;

/// One of the inspector's tabs: a label and an optional count, such as
/// "People" and "5".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorTab {
    pub label: SharedString,
    pub count: Option<SharedString>,
}

impl InspectorTab {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            count: None,
        }
    }

    /// A count after the label, such as the number of people.
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// The tab's accessible name: its label, then its count.
    pub fn accessible_label(&self) -> SharedString {
        match &self.count {
            Some(count) => format!("{}, {count}", self.label).into(),
            None => self.label.clone(),
        }
    }
}

/// The tabs an inspector shows, the selected one and what a choice does.
pub(super) struct InspectorTabs {
    pub(super) tabs: Vec<InspectorTab>,
    pub(super) selected: usize,
    pub(super) on_select: SelectHandler,
}

/// The tabs kept: the first [`INSPECTOR_MAX_TABS`], and the selected index
/// when it names one of them.
pub(super) fn kept(tabs: Vec<InspectorTab>, selected: usize) -> (Vec<InspectorTab>, Option<usize>) {
    let tabs: Vec<_> = tabs.into_iter().take(INSPECTOR_MAX_TABS).collect();
    let selected = (selected < tabs.len()).then_some(selected);
    (tabs, selected)
}

/// The tab list: one segmented unit named after the inspector, each tab a
/// focusable button with the tab role that Enter or Space chooses.
pub(super) fn tab_list(
    name: SharedString,
    tabs: InspectorTabs,
    tokens: &Tokens,
    cx: &App,
) -> AnyElement {
    let (items, selected) = kept(tabs.tabs, tabs.selected);
    let mono = cx.theme().mono_font_family.clone();
    let on_select = tabs.on_select;
    segmented::frame(tokens)
        .id(TAB_LIST_ID)
        .test_support()
        .role(Role::TabList)
        .aria_label(name)
        .children(items.into_iter().enumerate().map(|(index, tab)| {
            let chosen = selected == Some(index);
            let callback = on_select.clone();
            let template = Button::new((TAB_ID, index)).role(Role::Tab);
            let name = tab.accessible_label();
            segmented::segment(template, tab.label, chosen, tokens)
                .accessibility_label(name)
                .aria_selected(chosen)
                .children(
                    tab.count
                        .map(|count| count_mark(count, mono.clone(), tokens)),
                )
                .on_click(move |event, window, cx| callback(index, event, window, cx))
        }))
        .into_any_element()
}

/// The tab panel's accessible name: the selected tab's label.
pub(super) fn panel_label(tabs: &InspectorTabs) -> Option<SharedString> {
    tabs.tabs
        .get(tabs.selected)
        .filter(|_| tabs.selected < INSPECTOR_MAX_TABS)
        .map(|tab| tab.label.clone())
}
