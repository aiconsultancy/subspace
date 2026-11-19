//! The palette's lines: commands, items, sub-list options and the header.

use gpui_kit::component::command::{CommandGroup, CommandItem};
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Div, FontWeight, HighlightStyle, Hsla, ParentElement, SharedString, Styled, StyledText, div, px,
};
use subspace_core::palette::highlight_runs;
use subspace_dashboard::{CommandRun, Glyph, PaletteOption, Row, Tokens, truncated};

use crate::palette::{Entry, Section};

/// Line layout, in pixels.
const ICON_WIDTH: f32 = 14.;
const ID_MIN_WIDTH: f32 = 56.;
const SMALL_FONT: f32 = 12.;
const KEY_FONT: f32 = 11.;
const KEY_PAD_X: f32 = 5.;
const KEY_RADIUS: f32 = 3.;
const SUBJECT_MAX_WIDTH: f32 = 600.;
const SUBJECT_PAD_Y: f32 = 2.;
const SUBJECT_RADIUS: f32 = 4.;

pub(super) fn group_of(section: &Section, query: &str, tokens: &Tokens) -> CommandGroup {
    let items = section
        .entries
        .iter()
        .map(|entry| entry_item(entry, query, tokens));
    let group = CommandGroup::new().items(items);
    if section.title.is_empty() {
        group
    } else {
        group.label(section.title.clone())
    }
}

/// `text` with the query's matched word prefixes in the accent colour.
fn highlighted(text: &SharedString, query: &str, accent: Hsla) -> StyledText {
    let mut highlights = Vec::new();
    let mut at = 0;
    for run in highlight_runs(text, query) {
        let end = at + run.text.len();
        if run.hit {
            highlights.push((
                at..end,
                HighlightStyle {
                    color: Some(accent),
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
            ));
        }
        at = end;
    }
    StyledText::new(text.clone()).with_highlights(highlights)
}

/// The palette line for one entry.
fn entry_item(entry: &Entry, query: &str, tokens: &Tokens) -> CommandItem {
    let tokens = *tokens;
    let query = query.trim().to_string();
    match entry {
        Entry::Command(command) => {
            let label = command.label.clone();
            let shortcut = command.shortcut.clone();
            let icon = sub_marker(matches!(command.run, CommandRun::Sub(_)));
            CommandItem::new()
                .label(label.clone())
                .child(move |_, _| command_line(&label, shortcut.clone(), icon, &query, &tokens))
        }
        Entry::Item(_, row) => {
            let row = row.clone();
            CommandItem::new()
                .label(row.title.clone())
                .child(move |_, _| item_line(&row, &query, &tokens))
        }
        Entry::Option(option) => {
            let option = option.clone();
            CommandItem::new()
                .label(option.label.clone())
                .child(move |_, _| option_line(&option, &query, &tokens))
        }
    }
}

/// The leading glyph of a command line: a sub-list opens another list.
fn sub_marker(opens_sub_list: bool) -> &'static str {
    if opens_sub_list { "»" } else { "›" }
}

fn command_line(
    label: &SharedString,
    shortcut: Option<SharedString>,
    icon: &'static str,
    query: &str,
    tokens: &Tokens,
) -> Div {
    h_flex()
        .w_full()
        .gap_2()
        .child(div().w(px(ICON_WIDTH)).text_color(tokens.muted).child(icon))
        .child(
            truncated::styled(
                SharedString::from(format!("palette-command:{label}")),
                label.clone(),
                highlighted(label, query, tokens.accent),
            )
            .flex_1(),
        )
        .when_some(shortcut, |line, key| line.child(key_cap(key, tokens)))
}

fn option_line(option: &PaletteOption, query: &str, tokens: &Tokens) -> Div {
    h_flex()
        .w_full()
        .gap_2()
        .child(
            div()
                .w(px(ICON_WIDTH))
                .text_color(tokens.accent)
                .child(if option.checked { "✓" } else { "" }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(highlighted(&option.label, query, tokens.accent)),
        )
        .when_some(option.hint.clone(), |line, hint| {
            line.child(
                div()
                    .text_size(px(SMALL_FONT))
                    .text_color(tokens.muted)
                    .child(hint),
            )
        })
}

/// A shortcut shown as a key cap.
fn key_cap(key: SharedString, tokens: &Tokens) -> Div {
    div()
        .px(px(KEY_PAD_X))
        .rounded(px(KEY_RADIUS))
        .bg(tokens.hair)
        .text_size(px(KEY_FONT))
        .text_color(tokens.muted)
        .child(key)
}

/// An item's palette line: icon, id, title and trailing glyphs.
fn item_line(row: &Row, query: &str, tokens: &Tokens) -> Div {
    let glyph = |glyph: &Glyph| {
        div()
            .flex_shrink_0()
            .text_color(tokens.glyph(glyph))
            .child(glyph.text.clone())
    };
    h_flex()
        .w_full()
        .gap_2()
        .child(glyph(&row.icon))
        .when_some(row.id.clone(), |line, id| {
            line.child(
                div()
                    .flex_shrink_0()
                    .min_w(px(ID_MIN_WIDTH))
                    .text_size(px(SMALL_FONT))
                    .text_color(tokens.muted)
                    .child(highlighted(&id, query, tokens.accent)),
            )
        })
        .child(
            truncated::styled(
                SharedString::from(format!("palette-row:{}", row.key)),
                row.title.clone(),
                highlighted(&row.title, query, tokens.accent),
            )
            .flex_1(),
        )
        .children(row.trailing.iter().map(glyph))
}

/// The palette header: the subject row, or the way back from a sub-list.
pub(super) fn header_element(subject: Option<Row>, in_sub: bool, tokens: &Tokens) -> Option<Div> {
    if in_sub {
        return Some(
            div()
                .px_3()
                .pt_2()
                .text_size(px(SMALL_FONT))
                .text_color(tokens.muted)
                .child("← Esc or Backspace goes back"),
        );
    }
    let subject = subject?;
    Some(
        div().px_3().pt_2().child(
            item_line(&subject, "", tokens)
                .w_auto()
                .max_w(px(SUBJECT_MAX_WIDTH))
                .px_2()
                .py(px(SUBJECT_PAD_Y))
                .rounded(px(SUBJECT_RADIUS))
                .bg(tokens.hair)
                .text_size(px(SMALL_FONT)),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::sub_marker;

    #[test]
    pub(super) fn a_sub_list_command_is_marked_differently() {
        assert_eq!(sub_marker(true), "»");
        assert_eq!(sub_marker(false), "›");
    }
}
