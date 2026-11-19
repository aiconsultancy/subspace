//! The shared list view: group header rows, item rows and the detail panel.
//!
//! [`ListState`] is plain data: the groups, the selection, the folded groups,
//! whether the detail panel is open and the load status. [`render`] draws it
//! for any [`ListDashboard`], with the loading, empty and error states
//! (REQ-13), the wrap setting (REQ-09) and the density tokens (REQ-10).

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{AnyElement, App, Context, Entity, ScrollHandle, SharedString, Window};

use crate::row_menu::{self, MenuChat, RowMenu};
use crate::tokens::Tone;
use crate::{Callback, Dashboard, DashboardEvent};

/// Blank space under the last line, so it clears the window edge.
const LIST_TAIL_HEIGHT_PX: f32 = 48.;
/// Vertical padding of a wrapped row, which has no fixed height.
const WRAPPED_ROW_PADDING_Y_PX: f32 = 4.;
/// How much taller a comfortable row is than the theme's row height.
const COMFORTABLE_ROW_SCALE: f32 = 1.125;
/// Space above and below an inset row's rounded fill.
const ROW_FILL_GAP_PX: f32 = 1.;
/// Left padding of a row, before the group indent.
const ROW_PADDING_LEFT_PX: f32 = 6.;
/// Width of the fold arrow in a group header.
const FOLD_ARROW_WIDTH_PX: f32 = 14.;
/// Least width of a row's short id, so the titles line up.
const ROW_ID_MIN_WIDTH_PX: f32 = 56.;
/// Least height of one property line in the detail panel.
const PROP_ROW_MIN_HEIGHT_PX: f32 = 28.;
/// Width of the property label column.
const PROP_LABEL_WIDTH_PX: f32 = 96.;
/// Space above a panel section, and above the extra content under them.
const SECTION_TOP_MARGIN_PX: f32 = 18.;
const SECTION_TOP_PADDING_PX: f32 = 12.;
/// The panel title.
const PANEL_TITLE_SIZE_PX: f32 = 16.;
const PANEL_TITLE_MARGIN_PX: f32 = 14.;

/// A short coloured glyph: a row's type icon or a trailing state mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub text: SharedString,
    pub tone: Tone,
    /// An exact colour as `0xRRGGBB`, such as a GitHub label's; it wins over
    /// `tone`.
    pub rgb: Option<u32>,
}

impl Glyph {
    pub fn new(text: impl Into<SharedString>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
            rgb: None,
        }
    }

    /// A glyph in the exact colour `rgb` (`0xRRGGBB`).
    pub fn rgb(text: impl Into<SharedString>, rgb: u32) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Fg,
            rgb: Some(rgb),
        }
    }
}

/// One list row: icon, muted id, title, then trailing glyphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Unique in the list; the dashboard looks its details up by this key.
    /// Also the row's item search document id, by convention.
    pub key: SharedString,
    pub icon: Glyph,
    /// Short identifier, such as `4958950` or `repo#123`.
    pub id: Option<SharedString>,
    pub title: SharedString,
    pub trailing: Vec<Glyph>,
    /// Finished items show a muted title.
    pub done: bool,
    /// Extra indent levels under the group header, for a nested row such as
    /// a pull request under its project's main branch.
    pub indent: u8,
}

/// A group header (a repository, project or parent item) and its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub key: SharedString,
    pub icon: Glyph,
    pub name: SharedString,
    /// Shown at the right of the header.
    pub count: Option<SharedString>,
    pub rows: Vec<Row>,
}

/// A quiet line under a group's rows that is not a row: what the group is
/// waiting for, or why it has nothing to show. It takes no selection and no
/// keyboard stop, and folding the group hides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupNote {
    /// The group's rows are still on their way, such as "Loading runs…".
    Loading(SharedString),
    /// The group loaded with nothing to show, such as "No failed runs".
    Empty(SharedString),
}

impl GroupNote {
    /// The accessible name of group `group`'s note line, such as
    /// "acme/api: loading runs": the group, then the message in lower case
    /// without its trailing ellipsis or full stop.
    pub fn label(&self, group: &str) -> SharedString {
        let (Self::Loading(message) | Self::Empty(message)) = self;
        let message = message.trim_end_matches(['…', '.']);
        let mut chars = message.chars();
        let first = chars.next().map(|c| c.to_lowercase().to_string());
        format!("{group}: {}{}", first.unwrap_or_default(), chars.as_str()).into()
    }
}

/// A small, non-interactive status mark shown at the right of a group
/// header, such as a repository's scanning status. Presentation only: it
/// carries no click behaviour of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupIndicator {
    pub icon: subspace_lucide::SvgIcon,
    pub tone: crate::tokens::Tone,
    /// Read by assistive technology; also the row's accessible name.
    pub label: SharedString,
    /// Shown in a hover tooltip.
    pub tooltip: SharedString,
}

/// The text of a panel section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Text(SharedString),
    Markdown(SharedString),
    /// Service HTML, such as an Azure DevOps description; sanitised for display.
    Html(SharedString),
}

impl<T: Into<SharedString>> From<T> for Body {
    fn from(text: T) -> Self {
        Self::Text(text.into())
    }
}

/// A labelled block of text in the detail panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub heading: SharedString,
    pub body: Body,
}

impl Section {
    pub fn text(heading: impl Into<SharedString>, body: impl Into<SharedString>) -> Self {
        Self {
            heading: heading.into(),
            body: Body::Text(body.into()),
        }
    }

    pub fn markdown(heading: impl Into<SharedString>, body: impl Into<SharedString>) -> Self {
        Self {
            heading: heading.into(),
            body: Body::Markdown(body.into()),
        }
    }

    pub fn html(heading: impl Into<SharedString>, body: impl Into<SharedString>) -> Self {
        Self {
            heading: heading.into(),
            body: Body::Html(body.into()),
        }
    }
}

/// A link to the item's page in its service, such as "GitHub ↗".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLink {
    pub label: SharedString,
    pub url: SharedString,
}

impl ServiceLink {
    pub fn new(label: impl Into<SharedString>, url: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            url: url.into(),
        }
    }
}

/// What the detail panel shows for one row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Detail {
    /// The panel bar's type label, such as `Product Backlog Item`.
    pub kind: SharedString,
    pub id: SharedString,
    pub title: SharedString,
    /// The item's page in its service.
    pub link: Option<ServiceLink>,
    /// Label and value pairs, in order.
    pub props: Vec<(SharedString, SharedString)>,
    pub sections: Vec<Section>,
}

/// Whether the list has loaded (REQ-13).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoadStatus {
    #[default]
    Ready,
    Loading,
    Failed {
        title: SharedString,
        message: SharedString,
    },
}

/// The list's groups, selection, folded groups, panel and load status, and
/// its scroll position.
#[derive(Debug, Clone, Default)]
pub struct ListState {
    groups: Vec<Group>,
    folded: HashSet<SharedString>,
    selected: Option<SharedString>,
    detail_open: bool,
    status: LoadStatus,
    scroll: ScrollHandle,
    /// The next render scrolls the selected line into view.
    scroll_pending: Cell<bool>,
}

impl ListState {
    pub fn new(groups: Vec<Group>) -> Self {
        Self {
            groups,
            ..Self::default()
        }
    }

    // qual:allow(dry, boilerplate) reason: "Read-only groups keep mutation behind set_groups, which reconciles selection and the open panel."
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Replace the groups. The selection stays when its key is still shown.
    // qual:api
    pub fn set_groups(&mut self, groups: Vec<Group>) {
        self.groups = groups;
        if self
            .selected
            .as_ref()
            .is_some_and(|key| !self.visible_keys().contains(key))
        {
            self.selected = None;
            self.detail_open = false;
        }
    }

    // qual:allow(dry, boilerplate) reason: "The shared renderer borrows load status through this public reader; the behavioral state remains private and cannot be replaced directly by callers."
    pub fn status(&self) -> &LoadStatus {
        &self.status
    }

    // qual:api
    // qual:allow(dry, boilerplate) reason: "This setter is the public write boundary for private load state; exposing the field would make its representation part of the API."
    pub fn set_status(&mut self, status: LoadStatus) {
        self.status = status;
    }

    #[cfg(test)]
    pub fn row_count(&self) -> usize {
        self.groups.iter().map(|g| g.rows.len()).sum()
    }

    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.groups.iter().flat_map(|g| &g.rows)
    }

    pub fn selected(&self) -> Option<&SharedString> {
        self.selected.as_ref()
    }

    pub fn is_folded(&self, group: &str) -> bool {
        self.folded.contains(group)
    }

    /// The selected row, when a row (not a group header) is selected.
    pub fn selected_row(&self) -> Option<&Row> {
        let key = self.selected.as_ref()?;
        self.rows().find(|row| &row.key == key)
    }

    /// The row whose details are open.
    pub fn open_row(&self) -> Option<&Row> {
        self.detail_open.then(|| self.selected_row()).flatten()
    }

    /// The list's scroll handle, for a dashboard that draws the rows itself.
    // qual:allow(dry, boilerplate) reason: "ListState owns the same scroll handle used by rendering and selection navigation; custom dashboards may borrow it but must not replace its identity."
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// Scroll the selected line into view at the next render. Selection
    /// changes through [`ListState`] ask for this themselves.
    pub fn scroll_to_selected(&self) {
        self.scroll_pending.set(true);
    }

    /// Takes the pending scroll request: the selected line's position among
    /// the visible lines, when one is pending.
    pub fn take_scroll_request(&self) -> Option<usize> {
        if !self.scroll_pending.replace(false) {
            return None;
        }
        let key = self.selected.as_ref()?;
        self.visible_keys().iter().position(|k| k == key)
    }

    /// Keys in display order: each group header, then its rows unless folded.
    pub fn visible_keys(&self) -> Vec<SharedString> {
        let mut keys = Vec::new();
        for group in &self.groups {
            keys.push(group.key.clone());
            if !self.folded.contains(&group.key) {
                keys.extend(group.rows.iter().map(|row| row.key.clone()));
            }
        }
        keys
    }

    pub fn select(&mut self, key: SharedString) {
        self.selected = Some(key);
        self.scroll_to_selected();
        // The panel follows the selection while it is open.
        if self.selected_row().is_none() {
            self.detail_open = false;
        }
    }

    /// Move the selection by `step` visible lines; the first move selects
    /// the first line. Stops at either end.
    pub fn step(&mut self, step: isize) {
        let keys = self.visible_keys();
        let next = match self
            .selected
            .as_ref()
            .and_then(|key| keys.iter().position(|k| k == key))
        {
            Some(index) => index
                .saturating_add_signed(step)
                .min(keys.len().saturating_sub(1)),
            None => 0,
        };
        if let Some(key) = keys.get(next) {
            self.select(key.clone());
        }
    }

    /// Select the first (`last` false) or the last visible line.
    pub fn select_end(&mut self, last: bool) {
        let keys = self.visible_keys();
        let key = if last { keys.last() } else { keys.first() };
        if let Some(key) = key {
            self.select(key.clone());
        }
    }

    /// The panel's previous and next: move to the next visible row, skipping
    /// group headers, and keep the panel open. Returns the new row's key.
    pub fn step_row(&mut self, step: isize) -> Option<SharedString> {
        let rows: Vec<SharedString> = self
            .visible_keys()
            .into_iter()
            .filter(|key| self.rows().any(|row| &row.key == key))
            .collect();
        let current = self
            .selected
            .as_ref()
            .and_then(|key| rows.iter().position(|k| k == key))?;
        let next = current
            .saturating_add_signed(step)
            .min(rows.len().saturating_sub(1));
        let key = rows.get(next)?.clone();
        self.selected = Some(key.clone());
        self.detail_open = true;
        self.scroll_to_selected();
        Some(key)
    }

    /// Enter: fold or unfold a selected group, or open a selected row.
    /// Returns the key of the row whose details opened.
    pub fn activate(&mut self) -> Option<SharedString> {
        let key = self.selected.clone()?;
        if self.groups.iter().any(|g| g.key == key) {
            self.toggle(&key);
            None
        } else {
            self.detail_open = true;
            Some(key)
        }
    }

    /// Show the row `key` wherever it is: unfold its group, select it and
    /// open its details (REQ-05). False when no row has the key.
    pub fn reveal(&mut self, key: &str) -> bool {
        let Some(group) = self
            .groups
            .iter()
            .find(|g| g.rows.iter().any(|row| row.key == key))
        else {
            return false;
        };
        self.folded.remove(&group.key.clone());
        self.selected = Some(SharedString::from(key.to_string()));
        self.detail_open = true;
        self.scroll_to_selected();
        true
    }

    pub fn toggle(&mut self, group: &SharedString) {
        if !self.folded.remove(group) {
            self.folded.insert(group.clone());
        }
    }

    pub fn close_detail(&mut self) {
        self.detail_open = false;
    }
}

/// A dashboard drawn as grouped rows with a detail panel.
pub trait ListDashboard: Dashboard {
    /// A write dialog can prevent selection/panel changes until it finishes.
    fn can_navigate(&self, _cx: &App) -> bool {
        true
    }
    fn list(&self) -> &ListState;
    fn list_mut(&mut self) -> &mut ListState;
    /// The details of row `key`.
    fn detail(&self, key: &str) -> Option<Detail>;

    /// Optional domain identity in place of the default glyph, such as chat avatars.
    fn row_leading(&self, _row: &Row, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// Optional lanes after the title in place of the row's trailing glyphs,
    /// such as labels, a grouping and an owner. `None`, the default,
    /// draws [`Row::trailing`].
    fn row_trailing(&self, _row: &Row, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// Optional mark in place of a group header's glyph, such as a workflow
    /// state's icon. `None`, the default, draws [`Group::icon`].
    fn group_leading(&self, _group: &Group, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// Optional board-specific content inset; the theme remains the default.
    fn list_side_inset(&self) -> Option<gpui_kit::Pixels> {
        None
    }

    /// Keep a group's count next to its name instead of at the trailing edge.
    fn group_count_after_name(&self) -> bool {
        false
    }

    /// Comfortable rows: a step taller than the theme's row height, and group
    /// headers set apart by space and a hairline rather than a filled band.
    /// The theme's density still sets the base size.
    fn comfortable_rows(&self) -> bool {
        false
    }

    /// Emphasize the title without changing the leading identity column.
    fn row_emphasis(&self, _row: &Row, _cx: &App) -> bool {
        false
    }

    /// Accessible row name, including board-owned attention state when needed.
    fn row_accessible_label(&self, row: &Row, _cx: &App) -> SharedString {
        row.title.clone()
    }

    /// Whether the list's own empty, error and banner text offers the chat
    /// in its menu. Rows say so through [`ListDashboard::row_menu`].
    fn menu_chat(&self) -> MenuChat {
        MenuChat::Offered
    }

    /// The title and hint when there is nothing to show.
    fn empty_message(&self) -> (SharedString, SharedString) {
        (
            "Nothing to show".into(),
            "Try another filter, or reload.".into(),
        )
    }

    /// The empty state's actions. `None`, the default, offers Configure,
    /// which opens the board's settings; override when the board itself
    /// owns the next step, such as choosing repositories.
    fn empty_actions(&self, _cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        None
    }

    /// More panel content under the sections of row `key`, such as a
    /// composer.
    fn panel_extra(&self, _key: &str, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// Content pinned below the scrolling details, such as a chat composer.
    fn panel_footer(&self, _key: &str, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// An optional handle for controlling the detail body's scroll position.
    fn panel_scroll_handle(&self, _key: &str) -> Option<ScrollHandle> {
        None
    }

    /// Let bounded panel content own scrolling, with the footer outside it.
    /// In this mode the panel ignores `panel_scroll_handle`.
    fn panel_content_owns_scroll(&self, _key: &str) -> bool {
        false
    }

    /// Room reserved for the list when the inspector or Copilot narrows the board.
    fn min_list_width(&self) -> f32 {
        0.0
    }

    /// The selection moved to `selected` (keys, clicks, the panel's steps,
    /// a search result). Changes made through [`update`] call this.
    fn on_selection_changed(&mut self, _selected: Option<SharedString>, _cx: &mut Context<Self>) {}

    /// Row `key`'s details opened, or the open panel moved to row `key`:
    /// the place to start loading what the panel shows.
    fn on_open(&mut self, _key: SharedString, _cx: &mut Context<Self>) {}

    /// The detail panel opened (`true`) or closed.
    fn on_panel_toggled(&mut self, _open: bool, _cx: &mut Context<Self>) {}

    /// Enter or a click on line `key`, before the list folds a group or
    /// opens a row. Return `true` when the dashboard handled it, such as a
    /// "load more" line.
    fn on_activate(&mut self, _key: &SharedString, _cx: &mut Context<Self>) -> bool {
        false
    }

    /// Whether a click on `row` activates it, as Enter does. `false` makes
    /// a click only select the row and a double click activate it, for a
    /// row whose action changes something, such as joining a channel.
    fn activates_on_click(&self, _row: &Row) -> bool {
        true
    }

    /// What the row's right-click menu offers. The default gives the row's
    /// title and id, with its details to open; add the item's link by
    /// overriding.
    fn row_menu(&self, row: &Row) -> RowMenu {
        RowMenu::from_row(row)
    }

    /// Append board-owned actions after the standard row menu.
    fn extend_row_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu
    }

    /// Append board-owned submenus after [`Self::extend_row_menu`]. The
    /// menu's own context builds them; `board` reads the dashboard and acts
    /// on it from the entries. The line was selected before this runs, so
    /// `board.read(cx).list().selected()` is the line the menu is for.
    fn row_submenus(
        _board: &Entity<Self>,
        menu: PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<PopupMenu>,
    ) -> PopupMenu
    where
        Self: Sized,
    {
        menu
    }

    /// Optional friendly header text. The underlying ID stays available to menus.
    fn panel_header_label(&self, _key: &str) -> Option<SharedString> {
        None
    }

    /// What a group header's right-click menu offers. `None`, the default,
    /// gives a header no menu: a repository or a section is not an item.
    /// Override where the header stands for one.
    fn group_menu(&self, _group: &Group) -> Option<RowMenu> {
        None
    }

    /// Status marks shown at the right of a group header, after its name
    /// and count, such as a repository's scanning status. Empty by default,
    /// which preserves every existing board's header.
    fn group_indicators(&self, _group: &Group, _cx: &App) -> Vec<GroupIndicator> {
        vec![]
    }

    /// A quiet line under a group's rows, such as "Loading runs…" while the
    /// group's records load or "No failed runs" when a filter matches none.
    /// `None`, the default, draws nothing, which preserves every existing
    /// board's groups.
    fn group_note(&self, _group: &Group, _cx: &App) -> Option<GroupNote> {
        None
    }
}

/// The menu a row or group header opens on a right click. Opening it selects
/// the line first, so the shell's Add to Copilot chat acts on the line the
/// menu was opened over, and so do the dashboard's own commands.
pub(crate) fn line_menu<D: ListDashboard>(
    key: SharedString,
    facts: RowMenu,
    cx: &mut Context<D>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let entity = cx.entity().downgrade();
    let opener = entity.clone();
    let open: Callback = Rc::new(move |_, cx| {
        let _ = opener.update(cx, |this: &mut D, cx| activate(this, cx));
    });
    move |menu, window, cx| {
        let key = key.clone();
        let _ = entity.update(cx, |this: &mut D, cx| {
            update(this, cx, |list| list.select(key));
        });
        let open = facts.opens_details.then(|| open.clone());
        let menu = row_menu::popup(menu, row_menu::row_menu_items(&facts), open);
        match entity.upgrade() {
            Some(entity) => {
                let menu = entity.read(cx).extend_row_menu(menu, cx);
                D::row_submenus(&entity, menu, window, cx)
            }
            None => menu,
        }
    }
}

/// Changes the list with `change`, then calls the [`ListDashboard`] hooks
/// for what changed, tells the shell which row opened, and redraws. Use it
/// for selection and panel changes made outside the list's own keys.
pub fn update<D: ListDashboard>(
    this: &mut D,
    cx: &mut Context<D>,
    change: impl FnOnce(&mut ListState),
) {
    if !this.can_navigate(cx) {
        return;
    }
    let list = this.list();
    let selected = list.selected().cloned();
    let open = list.open_row().map(|row| row.key.clone());
    change(this.list_mut());
    let list = this.list();
    let now_selected = list.selected().cloned();
    let now_open = list.open_row().map(|row| row.key.clone());
    if now_selected != selected {
        this.on_selection_changed(now_selected, cx);
    }
    if now_open.is_some() != open.is_some() {
        this.on_panel_toggled(now_open.is_some(), cx);
    }
    if let Some(key) = now_open.filter(|key| Some(key) != open.as_ref()) {
        cx.emit(DashboardEvent::Opened(key.clone()));
        this.on_open(key, cx);
    }
    cx.notify();
}

/// Enter: the dashboard's [`ListDashboard::on_activate`] first, then fold
/// the selected group or open the selected row.
pub(crate) fn activate<D: ListDashboard>(this: &mut D, cx: &mut Context<D>) {
    if !this.can_navigate(cx) {
        return;
    }
    if let Some(key) = this.list().selected().cloned()
        && this.on_activate(&key, cx)
    {
        cx.notify();
        return;
    }
    update(this, cx, |list| {
        list.activate();
    });
}

mod view;

pub use view::{header, render, render_body};

#[cfg(test)]
#[path = "list/header_tests.rs"]
mod header_tests;

#[cfg(test)]
#[path = "list/note_tests.rs"]
mod note_tests;

#[cfg(test)]
mod tests {
    use super::{Body, Glyph, Group, GroupIndicator, ListState, Row, Section};
    use crate::tokens::Tone;
    use gpui_kit::SharedString;

    #[test]
    fn string_conversions_keep_plain_text_distinct_from_explicit_markup() {
        let text = "<strong>Display as text</strong>";
        let expected = Body::Text(text.into());
        assert_eq!(Body::from(SharedString::from(text)), expected);
        assert_eq!(Body::from(text), expected);
        assert_eq!(Body::from(text.to_owned()), expected);
        assert_eq!(Section::text("Heading", text).body, expected);
        assert_eq!(
            Section::markdown("Heading", text).body,
            Body::Markdown(text.into())
        );
        assert_eq!(Section::html("Heading", text).body, Body::Html(text.into()));
    }

    #[test]
    fn group_indicators_are_an_independent_presentation_mark_per_group() {
        let a = GroupIndicator {
            icon: subspace_lucide::IconName::ShieldCheck,
            tone: Tone::Green,
            label: "Scanning enabled".into(),
            tooltip: "Code scanning is enabled".into(),
        };
        let b = a.clone();
        assert_eq!(a, b);

        let different_tone = GroupIndicator {
            tone: Tone::Red,
            ..a.clone()
        };
        assert_ne!(a, different_tone);
    }

    fn row(key: &'static str) -> Row {
        Row {
            key: key.into(),
            icon: Glyph::new("●", Tone::Yellow),
            id: None,
            title: key.into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        }
    }

    fn state() -> ListState {
        ListState::new(vec![
            Group {
                key: "g1".into(),
                icon: Glyph::new("◆", Tone::Blue),
                name: "Group 1".into(),
                count: None,
                rows: vec![row("a"), row("b")],
            },
            Group {
                key: "g2".into(),
                icon: Glyph::new("◆", Tone::Blue),
                name: "Group 2".into(),
                count: None,
                rows: vec![row("c")],
            },
        ])
    }

    fn keys(state: &ListState) -> Vec<String> {
        state
            .visible_keys()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn stepping_walks_headers_and_rows_and_stops_at_the_ends() {
        let mut list = state();
        list.step(1);
        assert_eq!(list.selected().map(AsRef::as_ref), Some("g1"));
        list.step(-1);
        assert_eq!(list.selected().map(AsRef::as_ref), Some("g1"));
        for _ in 0..10 {
            list.step(1);
        }
        assert_eq!(list.selected().map(AsRef::as_ref), Some("c"));
        assert_eq!(list.row_count(), 3);
        list.select_end(false);
        assert_eq!(list.selected().map(AsRef::as_ref), Some("g1"));
    }

    #[test]
    fn enter_folds_a_group_and_opens_a_row() {
        let mut list = state();
        list.select("g1".into());
        assert_eq!(list.activate(), None);
        assert_eq!(keys(&list), ["g1", "g2", "c"]);
        list.activate();
        assert_eq!(keys(&list), ["g1", "a", "b", "g2", "c"]);

        list.select("a".into());
        assert!(list.open_row().is_none());
        assert_eq!(list.activate().as_deref(), Some("a"));
        assert_eq!(list.open_row().map(|r| r.key.as_ref()), Some("a"));
        // The open panel follows the selection to the next row.
        list.step(1);
        assert_eq!(list.open_row().map(|r| r.key.as_ref()), Some("b"));
        // A group header has no details, so the panel closes.
        list.step(1);
        assert!(list.open_row().is_none());
        list.step(1);
        assert!(list.open_row().is_none(), "the panel stays closed");
        list.activate();
        list.close_detail();
        assert!(list.open_row().is_none());
    }

    #[test]
    fn panel_steps_skip_group_headers() {
        let mut list = state();
        list.select("b".into());
        assert_eq!(list.step_row(1).as_deref(), Some("c"));
        assert_eq!(list.open_row().map(|r| r.key.as_ref()), Some("c"));
        assert_eq!(list.step_row(1).as_deref(), Some("c"), "stops at the end");
        assert_eq!(list.step_row(-2).as_deref(), Some("a"));
    }

    #[test]
    fn reveal_unfolds_selects_and_opens() {
        let mut list = state();
        list.toggle(&"g2".into());
        assert!(list.reveal("c"));
        assert_eq!(keys(&list), ["g1", "a", "b", "g2", "c"]);
        assert_eq!(list.open_row().map(|r| r.key.as_ref()), Some("c"));
        assert!(!list.reveal("missing"));
    }

    #[test]
    fn replacing_groups_keeps_a_selection_that_is_still_shown() {
        let mut list = state();
        list.select("c".into());
        list.set_groups(state().groups().to_vec());
        assert_eq!(list.selected().map(AsRef::as_ref), Some("c"));
        list.set_groups(Vec::new());
        assert!(list.selected().is_none());
    }
}
