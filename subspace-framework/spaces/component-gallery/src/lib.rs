//! Component gallery: a developer board that shows every shared Subspace
//! component, every shell surface and every GPUI Kit component in its
//! states, with fixture data only.
//!
//! The board reads no configuration or credentials and starts no network,
//! background or service work. Every sample calls the public constructor the
//! boards call; the gallery does not wrap or restyle a shared component.

mod browse;
mod catalog;
#[cfg(test)]
mod kit_usage;
mod samples;
mod state;
mod view;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, Render, ScrollHandle, SharedString, Subscription, Window,
};
use subspace_dashboard::menu::MenuAnchors;
use subspace_dashboard::settings::BoardSettings;
use subspace_dashboard::{
    BoardArtwork, Dashboard, DashboardEvent, Glyph, ItemSearch, KeyHelp, Row, Search, SearchDoc,
    Tone, prefs,
};

use crate::browse::Browse;
use crate::catalog::{Item, SECTIONS};

/// The named anchor of the `menu::MenuAnchors` sample.
pub(crate) const MENU_ANCHOR: &str = "gallery-sample:menu-anchors";
const SECTION_KEY: &str = "component-gallery.section";
const HIDE_UNUSED_KEY: &str = "component-gallery.hide-unused";
const SEARCH_PLACEHOLDER: &str = "Search the component gallery";

/// The gallery view. Retained entities are created once in [`Dashboard::new`].
// qual:allow(srp, god_struct) reason: "The board coordinates navigation, retained samples and fixture picker state. The samples implement their own interactions."
pub struct ComponentGallery {
    focus: FocusHandle,
    retained: state::Retained,
    menu_anchors: MenuAnchors,
    /// The section shown while the filter is empty.
    section: &'static str,
    /// Whether GPUI Kit components that Subspace does not use are hidden.
    hide_unused: bool,
    filter: Entity<InputState>,
    scroll: ScrollHandle,
    search: Rc<ItemSearch>,
    /// The repository picker sample's fixture favourites and open picker.
    repositories: samples::menus::owned::Repositories,
    trackers: samples::menus::trackers::Trackers,
    _filter_changes: Subscription,
}

impl ComponentGallery {
    /// Render one actual catalogue card for isolated fixture evidence.
    #[cfg(feature = "test-support")]
    pub fn render_test_item(
        &self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        view::test_item(self, name, window, cx)
    }

    /// Opens the fixture menu of the `menu::MenuAnchors` sample. The anchor
    /// retains the menu and closes it on a pick, Escape or an outside press.
    pub(crate) fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = self.menu_anchors.anchor(MENU_ANCHOR, cx);
        let _menu = self
            .menu_anchors
            .open(anchor, samples::menus::contents(), window, cx);
        cx.notify();
    }

    /// Close the sample menus and popovers before the board hides or the
    /// palette opens.
    fn dismiss_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        self.retained.popover.dismiss(window, cx);
        self.retained.kit.layout.popover.dismiss(window, cx);
        let toolbar = self.retained.toolbar.clone();
        toolbar.update(cx, |toolbar, cx| toolbar.dismiss(window, cx));
    }

    /// The filter text, trimmed.
    pub(crate) fn query(&self, cx: &App) -> SharedString {
        self.filter.read(cx).value().trim().to_owned().into()
    }

    /// Show `section` and store the choice.
    pub(crate) fn select_section(
        &mut self,
        section: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.section = section;
        prefs::set(cx, SECTION_KEY, section);
        self.filter
            .update(cx, |filter, cx| filter.set_value("", window, cx));
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// Hide or show the GPUI Kit components that Subspace does not use.
    pub(crate) fn set_hide_unused(&mut self, hide: bool, cx: &mut Context<Self>) {
        self.hide_unused = hide;
        prefs::set(cx, HIDE_UNUSED_KEY, if hide { "1" } else { "0" });
        cx.notify();
    }

    /// Show the card called `name`: its section, unfiltered, scrolled to it.
    /// A hidden unused GPUI Kit component makes the toggle show them again.
    pub(crate) fn reveal(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((section, item)) = catalog::find(name) else {
            return;
        };
        if self.hide_unused && item.is_unused_kit() {
            self.set_hide_unused(false, cx);
        }
        self.select_section(section.id, window, cx);
        let browse = Browse {
            query: "",
            hide_unused: self.hide_unused,
        };
        if let Some(at) = browse
            .items(section)
            .iter()
            .position(|shown| shown.name == name)
        {
            // The section heading is the first child of the scrolling page.
            self.scroll.scroll_to_item(at + 1);
        }
    }

    fn item_search() -> ItemSearch {
        ItemSearch::new(SECTIONS.iter().flat_map(|section| {
            section
                .items
                .iter()
                .map(move |item| search_entry(section.title, item))
        }))
    }
}

/// The filter input, and the subscription that redraws the gallery as it changes.
fn filter_input(
    window: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> (Entity<InputState>, Subscription) {
    let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name"));
    let changes = cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
    });
    (filter, changes)
}

/// Whether a stored flag is on.
fn is_on(stored: Option<String>) -> bool {
    stored.as_deref() == Some("1")
}

/// The `/` search document and palette row of one card.
fn search_entry(section: &str, item: &Item) -> (SearchDoc, Row) {
    let doc = SearchDoc::new(item.name, item.name)
        .parent(section)
        .meta(format!("{} {}", item.source, item.used_in));
    let row = Row {
        key: item.name.into(),
        icon: Glyph::new("◆", Tone::Muted),
        id: None,
        title: item.name.into(),
        trailing: Vec::new(),
        done: false,
        indent: 0,
    };
    (doc, row)
}

impl EventEmitter<DashboardEvent> for ComponentGallery {}

impl Focusable for ComponentGallery {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ComponentGallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        view::page(self, window, cx)
    }
}

impl Dashboard for ComponentGallery {
    const ID: &'static str = "component-gallery";
    const TITLE: &'static str = "Component gallery";
    const DESCRIPTION: &'static str =
        "Shared components, shell surfaces and GPUI Kit components in their states (developer)";
    const ICON: Option<&'static str> = Some("icons/component.svg");
    const ARTWORK: BoardArtwork = BoardArtwork::new(&[(
        "icons/component.svg",
        include_bytes!("../assets/component.svg"),
    )])
    .with_order(13);
    const HELP_MARKDOWN: &'static str = include_str!("../help.md");

    fn settings() -> BoardSettings {
        BoardSettings {
            description: "A developer tool that shows the shared components, the shell surfaces and every GPUI Kit component in their states with fixture data. It reads no configuration or credentials and starts no network requests.",
            ..Default::default()
        }
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (filter, filter_changes) = filter_input(window, cx);
        Self {
            focus: cx.focus_handle(),
            retained: state::Retained::new(window, cx),
            menu_anchors: MenuAnchors::default(),
            section: browse::section(&prefs::get(cx, SECTION_KEY).unwrap_or_default()).id,
            hide_unused: is_on(prefs::get(cx, HIDE_UNUSED_KEY)),
            filter,
            scroll: ScrollHandle::new(),
            search: Rc::new(Self::item_search()),
            repositories: Default::default(),
            trackers: Default::default(),
            _filter_changes: filter_changes,
        }
    }

    fn deactivate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_popups(window, cx);
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_popups(window, cx);
    }

    fn render_toolbar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        Some(view::toolbar(self, cx))
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        let (total, used) = browse::kit_counts();
        Some(format!("{used} of {total} GPUI Kit components used").into())
    }

    fn search(&self, _cx: &App) -> Option<Search> {
        Some(Search {
            placeholder: SEARCH_PLACEHOLDER.into(),
            items: self.search.clone(),
        })
    }

    fn open_search_result(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal(id, window, cx);
    }

    /// The gallery has no list, so it lists no list keys.
    fn keys(&self) -> &'static [KeyHelp] {
        &[]
    }

    /// Fixture data does not load; a reload only draws the view again.
    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }
}
