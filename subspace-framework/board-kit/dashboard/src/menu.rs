//! The searchable menu body (REQ-14): type to filter, arrows to move, Enter to pick,
//! Escape, Tab or a click outside to close.
//!
//! A dashboard retains [`MenuAnchors`] beside its `Option<Entity<Menu>>`,
//! subscribes to [`MenuEvent`], and renders `MenuAnchors::view`. The shared
//! anchored host owns placement, Escape, outside dismissal and focus.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    SharedString, Subscription, Window, actions,
};

use crate::list::Glyph;

mod anchors;
pub use anchors::MenuAnchors;

actions!(
    subspace_menu,
    [
        MenuNext,
        MenuPrevious,
        MenuPick,
        MenuClose,
        MenuPick1,
        MenuPick2,
        MenuPick3,
        MenuPick4,
        MenuPick5,
        MenuPick6,
        MenuPick7,
        MenuPick8,
        MenuPick9
    ]
);

pub const MENU_CONTEXT: &str = "SubspaceMenu";

/// The first nine options carry a number badge and answer that digit key.
const DIGIT_PICKS: usize = 9;
/// Height of one option row.
const ROW_HEIGHT_PX: f32 = 26.;
/// Width of the icon slot on the left of every row.
const ICON_SLOT_WIDTH_PX: f32 = 22.;
const POPUP_WIDTH_PX: f32 = 260.;
const POPUP_MAX_HEIGHT_PX: f32 = 320.;

pub(crate) fn bind_keys(cx: &mut App) {
    // In the filter field, ahead of the field's own bindings for these keys.
    let context = Some("SubspaceMenu > Input");
    cx.bind_keys([
        KeyBinding::new("down", MenuNext, context),
        KeyBinding::new("ctrl-n", MenuNext, context),
        KeyBinding::new("up", MenuPrevious, context),
        KeyBinding::new("ctrl-p", MenuPrevious, context),
        KeyBinding::new("enter", MenuPick, context),
        KeyBinding::new("tab", MenuClose, context),
        // Ahead of an enclosing dialog's Escape, such as Settings'.
        KeyBinding::new("escape", MenuClose, context),
        // Digits pick directly while the filter is empty; else they type.
        KeyBinding::new("1", MenuPick1, context),
        KeyBinding::new("2", MenuPick2, context),
        KeyBinding::new("3", MenuPick3, context),
        KeyBinding::new("4", MenuPick4, context),
        KeyBinding::new("5", MenuPick5, context),
        KeyBinding::new("6", MenuPick6, context),
        KeyBinding::new("7", MenuPick7, context),
        KeyBinding::new("8", MenuPick8, context),
        KeyBinding::new("9", MenuPick9, context),
    ]);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuOption {
    pub value: SharedString,
    pub label: SharedString,
    pub icon: Option<Glyph>,
    pub checked: bool,
    /// The heading of the run of options this one belongs to. A heading
    /// shows above the first option of each run; it is not an option.
    pub section: Option<SharedString>,
    /// A favourite mark in the icon slot: a filled star when true, an
    /// outlined one when false. Picking the option toggles it.
    pub starred: Option<bool>,
}

impl MenuOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: None,
            checked: false,
            section: None,
            starred: None,
        }
    }

    /// What assistive technology reads: the label, and the favourite state.
    pub fn accessible_label(&self) -> SharedString {
        match self.starred {
            Some(true) => format!("{}, favourite", self.label).into(),
            Some(false) => format!("{}, not a favourite", self.label).into(),
            None => self.label.clone(),
        }
    }

    pub fn icon(mut self, icon: Glyph) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// The filterable rows a menu shows: its accessible name, the search
/// placeholder, and the options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuContents {
    pub label: SharedString,
    pub placeholder: SharedString,
    pub options: Vec<MenuOption>,
    /// A pick leaves the menu open, for toggles such as favourites. Escape,
    /// Tab or a click outside still close it.
    pub keep_open: bool,
    /// The owner answers each typed query with [`Menu::set_options`] after
    /// [`MenuEvent::Query`]; the menu shows the options it is given as they
    /// are, without filtering them itself.
    pub owner_filtered: bool,
}

impl MenuContents {
    pub fn new(
        label: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        options: Vec<MenuOption>,
    ) -> Self {
        Self {
            label: label.into(),
            placeholder: placeholder.into(),
            options,
            keep_open: false,
            owner_filtered: false,
        }
    }

    pub fn keep_open(mut self, keep_open: bool) -> Self {
        self.keep_open = keep_open;
        self
    }

    pub fn owner_filtered(mut self, owner_filtered: bool) -> Self {
        self.owner_filtered = owner_filtered;
        self
    }

    /// The first checked option, or the first row when none is checked.
    pub fn initial_active(&self) -> usize {
        self.options
            .iter()
            .position(|option| option.checked)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEvent {
    Pick(SharedString),
    /// The typed text changed, in an [`owner_filtered`](MenuContents::owner_filtered) menu.
    Query(SharedString),
    Close,
}

/// The positions of the options whose label contains `query`, ignoring case.
pub fn filter_options(options: &[MenuOption], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    options
        .iter()
        .enumerate()
        .filter(|(_, option)| query.is_empty() || option.label.to_lowercase().contains(&query))
        .map(|(index, _)| index)
        .collect()
}

/// The heading to show before each shown option: its section, where the
/// section differs from the previous shown option's.
pub(crate) fn headings(options: &[MenuOption], shown: &[usize]) -> Vec<Option<SharedString>> {
    let mut previous: Option<&SharedString> = None;
    let mut headings = Vec::with_capacity(shown.len());
    for &index in shown {
        let section = options
            .get(index)
            .and_then(|option| option.section.as_ref());
        headings.push(section.filter(|title| Some(*title) != previous).cloned());
        previous = section;
    }
    headings
}

/// The muted line under the options: the owner's note, else No match when
/// nothing shows.
pub(crate) fn note_text(note: Option<&SharedString>, empty: bool) -> Option<SharedString> {
    match (note, empty) {
        (Some(note), _) => Some(note.clone()),
        (None, true) => Some("No match".into()),
        (None, false) => None,
    }
}

pub struct Menu {
    label: SharedString,
    options: Vec<MenuOption>,
    input: Entity<InputState>,
    /// Position in the filtered list.
    active: usize,
    keep_open: bool,
    owner_filtered: bool,
    /// A muted line under the options, such as a search in progress.
    note: Option<SharedString>,
    _subscription: Subscription,
}

impl EventEmitter<MenuEvent> for Menu {}

impl Menu {
    /// A searchable body. MenuAnchors opens and focuses it in the shared host.
    pub fn new(contents: MenuContents, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let active = contents.initial_active();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(contents.placeholder));
        let subscription = cx.subscribe(&input, |menu: &mut Self, input, event, cx| {
            if matches!(event, InputEvent::Change) {
                menu.active = 0;
                if menu.owner_filtered {
                    cx.emit(MenuEvent::Query(input.read(cx).value()));
                }
                cx.notify();
            }
        });
        Self {
            label: contents.label,
            options: contents.options,
            input,
            active,
            keep_open: contents.keep_open,
            owner_filtered: contents.owner_filtered,
            note: None,
            _subscription: subscription,
        }
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Whether a pick leaves the menu open.
    pub fn is_kept_open(&self) -> bool {
        self.keep_open
    }

    /// Replaces the options, keeping the highlight on the same position
    /// where it still exists.
    pub fn set_options(&mut self, options: Vec<MenuOption>, cx: &mut Context<Self>) {
        self.options = options;
        cx.notify();
    }

    /// Shows `note` under the options, or nothing.
    pub fn set_note(&mut self, note: Option<SharedString>, cx: &mut Context<Self>) {
        self.note = note;
        cx.notify();
    }

    fn shown(&self, cx: &App) -> Vec<usize> {
        match self.owner_filtered {
            true => (0..self.options.len()).collect(),
            false => filter_options(&self.options, &self.input.read(cx).value()),
        }
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.shown(cx).len().max(1) as isize;
        self.active = (self.active as isize + step).rem_euclid(count) as usize;
        cx.notify();
    }

    fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let shown = self.shown(cx);
        if let Some(option) = shown.get(index).and_then(|&i| self.options.get(i)) {
            cx.emit(MenuEvent::Pick(option.value.clone()));
        }
    }

    /// Digit `n` (1 to 9): picks option `n` while the filter is empty,
    /// else types the digit.
    fn pick_digit(&mut self, n: usize, cx: &mut Context<Self>) {
        if self.input.read(cx).value().is_empty() {
            self.pick(n - 1, cx);
        } else {
            cx.propagate();
        }
    }

    /// Moves the highlight to the row the pointer is over.
    fn hover(&mut self, position: usize, cx: &mut Context<Self>) {
        if self.active != position {
            self.active = position;
            cx.notify();
        }
    }
}

impl Focusable for Menu {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

#[cfg(test)]
mod runtime_tests;
mod view;

/// Where one option sits while the menu draws it: its place in `options`,
/// its place in the filtered list, which place is highlighted, and whether
/// the digit badges show.
#[derive(Debug, Clone, Copy)]
struct RowAt {
    index: usize,
    position: usize,
    active: usize,
    numbered: bool,
}

impl RowAt {
    /// Whether this row carries the highlight.
    fn is_active(self) -> bool {
        self.position == self.active
    }

    /// Whether this row shows a digit badge: only the first nine, and only
    /// while no filter is typed.
    fn shows_badge(self) -> bool {
        self.numbered && self.position < DIGIT_PICKS
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuContents, MenuOption, RowAt, filter_options};

    #[test]
    fn the_filter_matches_label_substrings_without_case() {
        let options = [
            MenuOption::new("todo", "To Do"),
            MenuOption::new("doing", "In Progress"),
            MenuOption::new("done", "Done"),
        ];
        assert_eq!(filter_options(&options, ""), [0, 1, 2]);
        assert_eq!(filter_options(&options, "do"), [0, 2]);
        assert_eq!(filter_options(&options, " PROG "), [1]);
        assert!(filter_options(&options, "xyz").is_empty());
    }

    fn row(position: usize, active: usize, numbered: bool) -> RowAt {
        RowAt {
            index: 0,
            position,
            active,
            numbered,
        }
    }

    #[test]
    fn only_the_row_at_the_active_position_is_highlighted() {
        assert!(row(2, 2, true).is_active());
        assert!(!row(1, 2, true).is_active());
    }

    #[test]
    fn the_checked_option_is_the_initial_active_row() {
        let contents = MenuContents::new(
            "Severity",
            "Show issues of severity…",
            vec![
                MenuOption::new("all", "All"),
                MenuOption::new("high", "High").checked(true),
                MenuOption::new("low", "Low"),
            ],
        );
        assert_eq!(contents.initial_active(), 1);
        assert_eq!(
            MenuContents::new("x", "y", vec![MenuOption::new("a", "A")]).initial_active(),
            0
        );
    }

    #[test]
    fn a_heading_shows_above_the_first_option_of_each_section_and_notes_replace_no_match() {
        let in_section = |value: &str, section: Option<&str>| MenuOption {
            section: section.map(Into::into),
            ..MenuOption::new(value.to_owned(), value.to_owned())
        };
        let options = [
            in_section("a", Some("One")),
            in_section("b", Some("One")),
            in_section("c", None),
            in_section("d", Some("Two")),
        ];
        let headings = super::headings(&options, &[0, 1, 2, 3]);
        assert_eq!(
            headings,
            [Some("One".into()), None, None, Some("Two".into())]
        );
        assert_eq!(
            super::headings(&options, &[1, 3]),
            [Some("One".into()), Some("Two".into())]
        );
        assert_eq!(super::note_text(None, true).as_deref(), Some("No match"));
        assert_eq!(super::note_text(None, false), None);
        let searching = "Searching…".into();
        assert_eq!(super::note_text(Some(&searching), false), Some(searching));
    }

    #[test]
    fn the_first_nine_rows_show_a_digit_badge_while_no_filter_is_typed() {
        assert!(row(0, 0, true).shows_badge());
        assert!(row(8, 0, true).shows_badge());
        assert!(!row(9, 0, true).shows_badge(), "past the ninth row");
        assert!(!row(0, 0, false).shows_badge(), "a filter is typed");
    }
}
