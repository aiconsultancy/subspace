//! The completion list above a chat input. The owner works out what the
//! word at the cursor could become, such as a `/command` or a person's
//! name, and hands the choices to the list; the list shows them, keeps the
//! highlight and reports the chosen row.
//!
//! The chat input's own keys drive it. While the owner marks the input as
//! completing ([`super::ChatInput::set_completing`]), Up and Down emit
//! `Previous` and `Next`, Enter and Tab emit `Complete` and Escape emits
//! `Cancel`; the owner answers with [`CompletionList::step`] and
//! [`CompletionList::active`], and may read which key it was with
//! [`super::ChatInput::completion_key`]. A click on a row emits
//! [`CompletionEvent::Accept`]. The list never takes focus from the input.
//!
//! Rows use the GPUI Kit `Command` list with filtering off, because the
//! query lives in the chat input rather than in a search field of its own.

use std::ops::Range;

use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{IndexPath, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AppContext as _, Context, Div, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div,
};

use crate::{Tokens, avatar};

/// How much a dimmed row, such as an away member, fades.
const DIMMED_OPACITY: f32 = 0.55;

/// One choice in the list. A plain row shows only its label; a rich row
/// adds an avatar, an argument hint, a description or a trailing mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    label: SharedString,
    spoken: Option<SharedString>,
    mark: Option<SharedString>,
    hint: Option<SharedString>,
    detail: Option<SharedString>,
    avatar: bool,
    dimmed: bool,
}

impl CompletionItem {
    /// A plain row that shows only `label`.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            spoken: None,
            mark: None,
            hint: None,
            detail: None,
            avatar: false,
            dimmed: false,
        }
    }

    /// A command: its name, its arguments muted after it and a one-line
    /// description at the end of the row.
    pub fn command(
        label: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        detail: impl Into<SharedString>,
    ) -> Self {
        Self {
            hint: present(hint.into()),
            detail: present(detail.into()),
            ..Self::new(label)
        }
    }

    /// A person: initials and name, a muted mark at the end of the row such
    /// as a channel operator's `@` (where member lists show it), and faded
    /// while `away`.
    pub fn person(
        name: impl Into<SharedString>,
        mark: impl Into<SharedString>,
        away: bool,
    ) -> Self {
        Self {
            mark: present(mark.into()),
            avatar: true,
            dimmed: away,
            ..Self::new(name)
        }
    }

    /// The accessible name, when it says more than the label.
    pub fn spoken(mut self, spoken: impl Into<SharedString>) -> Self {
        self.spoken = Some(spoken.into());
        self
    }

    pub fn label(&self) -> &SharedString {
        &self.label
    }

    pub fn spoken_label(&self) -> &SharedString {
        self.spoken.as_ref().unwrap_or(&self.label)
    }

    fn is_rich(&self) -> bool {
        self.avatar || self.mark.is_some() || self.hint.is_some() || self.detail.is_some()
    }
}

/// The byte range of the word that ends at `cursor`: from the last
/// whitespace before it, or the start of the text.
// qual:api
pub fn word_before(text: &str, cursor: usize) -> Range<usize> {
    let cursor = floor_boundary(text, cursor.min(text.len()));
    let start = text[..cursor]
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map_or(0, |(at, character)| at + character.len_utf8());
    start..cursor
}

fn present(text: SharedString) -> Option<SharedString> {
    (!text.is_empty()).then_some(text)
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The highlighted position after moving by `step`, wrapping at both ends.
pub fn step_index(active: usize, count: usize, step: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let count = count as isize;
    (active as isize + step).rem_euclid(count) as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionEvent {
    /// The row at this position was clicked.
    Accept(usize),
}

pub struct CompletionList {
    id: SharedString,
    items: Vec<CompletionItem>,
    active: usize,
    /// Interaction state of the GPUI Kit list.
    state: Entity<CommandState>,
}

impl EventEmitter<CompletionEvent> for CompletionList {}

impl CompletionList {
    /// An empty, hidden list; `id` names its element.
    pub fn new(id: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            active: 0,
            state: cx.new(|cx| CommandState::new(window, cx)),
        }
    }

    /// Shows `items`, keeping the highlight where it was when it still fits.
    /// No items hide the list.
    pub fn show(&mut self, items: Vec<CompletionItem>, cx: &mut Context<Self>) {
        self.active = self.active.min(items.len().saturating_sub(1));
        self.items = items;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.items.clear();
        self.active = 0;
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn items(&self) -> &[CompletionItem] {
        &self.items
    }

    pub fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        self.active = step_index(self.active, self.items.len(), step);
        cx.notify();
    }

    /// The highlighted position, if the list is open.
    pub fn active(&self) -> Option<usize> {
        self.is_open().then_some(self.active)
    }

    fn rows(&self, tokens: &Tokens) -> Vec<CommandItem> {
        self.items
            .iter()
            .map(|item| {
                if !item.is_rich() {
                    return CommandItem::new().label(item.label.clone());
                }
                let spoken = item.spoken_label().clone();
                let item = item.clone();
                let tokens = *tokens;
                CommandItem::new()
                    .label(spoken)
                    .child(move |_, _| rich_row(&item, &tokens))
            })
            .collect()
    }
}

fn rich_row(item: &CompletionItem, tokens: &Tokens) -> Div {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .when(item.dimmed, |row| row.opacity(DIMMED_OPACITY))
        .when(item.avatar, |row| {
            row.child(avatar::member(item.label.clone(), tokens))
        })
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.fg)
                .child(item.label.clone()),
        )
        .children(
            item.hint
                .clone()
                .map(|hint| div().flex_shrink_0().text_color(tokens.muted).child(hint)),
        )
        .children(item.detail.clone().map(|detail| {
            div()
                .flex_1()
                .min_w_0()
                .text_right()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(tokens.muted)
                .child(detail)
        }))
        .children(item.mark.clone().map(|mark| {
            div()
                .ml_auto()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(mark)
        }))
}

impl Render for CompletionList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let active = self.active;
        self.state.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::new(active)), window, cx);
        });
        let list = cx.entity().downgrade();
        v_flex()
            .id(self.id.clone())
            .w_full()
            .text_size(tokens.row_font)
            .when(!self.is_open(), |list| list.hidden())
            .child(
                Command::new(&self.state)
                    .searchable(false)
                    .filterable(false)
                    .items(self.rows(&tokens))
                    .on_confirm(move |path, _, cx| {
                        let _ = list.update(cx, |list, cx| {
                            list.active = path.row;
                            cx.emit(CompletionEvent::Accept(path.row));
                        });
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{CompletionItem, step_index, word_before};

    #[test]
    fn the_word_before_the_cursor_starts_after_the_last_space() {
        assert_eq!(word_before("@gal", 4), 0..4);
        assert_eq!(word_before("hi @gal there", 7), 3..7, "mid-text");
        assert_eq!(word_before("hi ", 3), 3..3, "nothing typed yet");
        assert_eq!(word_before("héllo", 3), 0..3, "a cursor inside a character");
        assert_eq!(word_before("ab", 9), 0..2, "a cursor past the end");
    }

    #[test]
    fn the_highlight_wraps_at_both_ends() {
        assert_eq!(step_index(0, 3, 1), 1);
        assert_eq!(step_index(2, 3, 1), 0);
        assert_eq!(step_index(0, 3, -1), 2);
        assert_eq!(step_index(0, 0, 1), 0, "an empty list stays put");
    }

    #[test]
    fn a_row_speaks_its_label_unless_told_more() {
        let plain = CompletionItem::new("/new");
        assert_eq!(plain.spoken_label().as_ref(), "/new");
        assert!(!plain.is_rich());
        let member = CompletionItem::person("galactica-captain", "@", false)
            .spoken("galactica-captain, operator");
        assert_eq!(
            member.spoken_label().as_ref(),
            "galactica-captain, operator"
        );
        assert!(member.is_rich());
        let bare = CompletionItem::command("/away", "", "");
        assert!(
            !bare.is_rich(),
            "empty arguments and description add nothing"
        );
    }
}
