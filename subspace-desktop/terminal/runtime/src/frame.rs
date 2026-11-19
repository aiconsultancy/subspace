//! One painted frame: the cells of each row grouped into the few spans the
//! element actually draws (REQ-12, REQ-13).
//!
//! A row costs a handful of shaped lines rather than one per cell: adjacent
//! cells with the same colours and attributes join into one run. Backgrounds
//! group separately, because a cell can carry a background with no glyph on
//! it, and the painter needs a quad there.
//!
//! A cell whose whole cluster is one Powerline, box-drawing or block code
//! point leaves the runs as well: [`crate::sprite`] draws it from the cell's
//! own size, so it breaks the run the way a blank does and the text either
//! side of it still shapes as runs (REQ-13).
//!
//! The grouping here is pure, so it is tested without a window. Reading
//! libghostty's render state into these types is [`crate::snapshot`].

use gpui_kit::Hsla;
use libghostty_vt::style::{Style, Underline};

use crate::sprite;

/// A row never has more columns than a grid can hold, which is a `u16`.
const MAX_COLUMN: usize = u16::MAX as usize;

/// How much of its opacity a faint glyph keeps (REQ-13).
const FAINT_ALPHA: f32 = 0.6;

/// What a cell looks like: its resolved colours and the attributes that change
/// the font or add a decoration (REQ-13).
///
/// Inverse, faint and hidden are already resolved into the colours, so the
/// painter never re-reads them. Blinking stays a flag, because the phase
/// changes between frames while a cached frame does not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellLook {
    /// The glyph colour.
    pub(crate) foreground: Hsla,
    /// The cell's background, when it has one of its own.
    pub(crate) background: Option<Hsla>,
    /// Draw the glyph with the bold family and weight.
    pub(crate) bold: bool,
    /// Draw the glyph with the italic family and slant.
    pub(crate) italic: bool,
    /// Underline the run.
    pub(crate) underline: bool,
    /// Strike the run through.
    pub(crate) strikethrough: bool,
    /// Hide the run while the blink phase is off.
    pub(crate) blink: bool,
}

impl CellLook {
    /// A cell with the session's default colours and no attributes.
    pub(crate) fn plain(foreground: Hsla) -> Self {
        Self {
            foreground,
            background: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            blink: false,
        }
    }
}

/// The colours a frame resolves its cells against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameDefaults {
    /// The default foreground: the emulator's, which the style set from the
    /// palette and an OSC 10 sequence can change.
    pub(crate) foreground: Hsla,
    /// The default background, from the same source.
    pub(crate) background: Hsla,
    /// The background of a selected cell.
    pub(crate) selection_background: Hsla,
    /// The foreground of a selected cell.
    pub(crate) selection_foreground: Hsla,
}

/// What the emulator reports about one cell's appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellFacts {
    /// The cell's own foreground, when it set one.
    pub(crate) foreground: Option<Hsla>,
    /// The cell's own background, when it set one.
    pub(crate) background: Option<Hsla>,
    /// The cell's attributes.
    pub(crate) style: Style,
}

/// The look a cell's reported colours and attributes produce (REQ-13).
///
/// Inverse swaps the pair, hidden paints the glyph in its own background,
/// faint keeps part of the glyph's opacity, and a selected cell takes the
/// selection pair, so selected text forms its own runs without the painter
/// knowing about any of it.
pub(crate) fn look_from(facts: &CellFacts, defaults: &FrameDefaults, selected: bool) -> CellLook {
    let style = facts.style;
    let mut foreground = facts.foreground.unwrap_or(defaults.foreground);
    let mut background = facts.background;
    if style.inverse {
        let behind = background.unwrap_or(defaults.background);
        background = Some(foreground);
        foreground = behind;
    }
    if style.invisible {
        foreground = background.unwrap_or(defaults.background);
    }
    if style.faint {
        foreground.a *= FAINT_ALPHA;
    }
    if selected {
        background = Some(defaults.selection_background);
        foreground = defaults.selection_foreground;
    }
    CellLook {
        foreground,
        background,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline != Underline::None,
        strikethrough: style.strikethrough,
        blink: style.blink,
    }
}

/// One cell of a row as the renderer reads it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowCell {
    /// The cell's whole grapheme cluster, so a combining mark draws on its
    /// base cell. Empty for a blank cell and for the spacer that follows a
    /// double-width character, neither of which has a glyph of its own
    /// (REQ-13).
    pub(crate) text: String,
    /// What the cell looks like.
    pub(crate) look: CellLook,
}

/// One run of cells with the same look, shaped as a single line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextRunSpan {
    /// The column the run starts at.
    pub(crate) column: u16,
    /// The run's text, one grapheme cluster per cell it covers.
    pub(crate) text: String,
    /// What the run looks like.
    pub(crate) look: CellLook,
}

/// One cell drawn from its own size instead of from the font: a Powerline
/// separator, a box-drawing line or a block element (REQ-13).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpriteSpan {
    /// The column the cell sits in.
    pub(crate) column: u16,
    /// The code point to draw.
    pub(crate) glyph: char,
    /// What the cell looks like, which carries the inverse, selection and
    /// blink rules the text runs carry.
    pub(crate) look: CellLook,
}

/// A span of columns filled with one background colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BackgroundSpan {
    /// The column the span starts at.
    pub(crate) column: u16,
    /// How many columns it covers.
    pub(crate) columns: u16,
    /// The colour behind them.
    pub(crate) color: Hsla,
}

/// One row of a frame, ready to paint.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FrameRow {
    /// Where the row sits in the viewport, in rows from its top.
    pub(crate) row: u16,
    /// The backgrounds behind the row, left to right.
    pub(crate) backgrounds: Vec<BackgroundSpan>,
    /// The runs of text on the row, left to right.
    pub(crate) runs: Vec<TextRunSpan>,
    /// The cells of the row this crate draws itself, left to right.
    pub(crate) sprites: Vec<SpriteSpan>,
}

/// Group one row's cells into the spans the painter draws.
pub(crate) fn group_row(row: u16, cells: &[RowCell]) -> FrameRow {
    let glyphs = sprite_glyphs(cells);
    FrameRow {
        row,
        backgrounds: background_spans(cells),
        runs: text_runs(cells, &glyphs),
        sprites: sprite_spans(cells, &glyphs),
    }
}

/// The sprite glyph of each cell, where the cell has one.
fn sprite_glyphs(cells: &[RowCell]) -> Vec<Option<char>> {
    cells
        .iter()
        .map(|cell| sprite::sprite_of(&cell.text))
        .collect()
}

/// The cells of a row this crate draws itself.
fn sprite_spans(cells: &[RowCell], glyphs: &[Option<char>]) -> Vec<SpriteSpan> {
    let mut spans = Vec::new();
    for (index, found) in glyphs.iter().take(MAX_COLUMN).enumerate() {
        let (Some(glyph), Some(cell)) = (found, cells.get(index)) else {
            continue;
        };
        spans.push(SpriteSpan {
            column: index as u16,
            glyph: *glyph,
            look: cell.look,
        });
    }
    spans
}

/// The background spans of a row: adjacent cells with the same colour join,
/// and a cell with no background of its own contributes nothing.
fn background_spans(cells: &[RowCell]) -> Vec<BackgroundSpan> {
    let mut spans: Vec<BackgroundSpan> = Vec::new();
    for (index, cell) in cells.iter().take(MAX_COLUMN).enumerate() {
        let Some(color) = cell.look.background else {
            continue;
        };
        let column = index as u16;
        match spans.last_mut() {
            Some(span) if span.color == color && span.column + span.columns == column => {
                span.columns += 1;
            }
            _ => spans.push(BackgroundSpan {
                column,
                columns: 1,
                color,
            }),
        }
    }
    spans
}

/// The text runs of a row.
///
/// Adjacent cells with the same look join. A cell with no glyph ends the run:
/// it is either blank or the spacer of a double-width character, so the run
/// that follows starts at its own column and the wide glyph keeps both of its
/// cells without overlapping its neighbour (REQ-13). A cell this crate draws
/// itself ends the run the same way, and is painted from `sprites`.
fn text_runs(cells: &[RowCell], glyphs: &[Option<char>]) -> Vec<TextRunSpan> {
    let mut runs: Vec<TextRunSpan> = Vec::new();
    let mut open = false;
    for (index, cell) in cells.iter().take(MAX_COLUMN).enumerate() {
        if cell.text.is_empty() || glyphs.get(index).is_some_and(Option::is_some) {
            open = false;
            continue;
        }
        match runs.last_mut() {
            Some(run) if open && run.look == cell.look => run.text.push_str(&cell.text),
            _ => runs.push(TextRunSpan {
                column: index as u16,
                text: cell.text.clone(),
                look: cell.look,
            }),
        }
        open = true;
    }
    runs
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Hsla, Rgba, rgb};
    use libghostty_vt::style::{Style, Underline};

    use crate::frame::{
        BackgroundSpan, CellFacts, CellLook, FrameDefaults, RowCell, SpriteSpan, TextRunSpan,
        group_row, look_from,
    };

    /// The default foreground of the test rows.
    fn foreground() -> Hsla {
        Hsla::from(rgb(0xabb2bf))
    }

    /// The default background of the test rows.
    fn background() -> Hsla {
        Hsla::from(rgb(0x101115))
    }

    /// A second colour, so a test can see a run break.
    fn red() -> Hsla {
        Hsla::from(rgb(0xe06c75))
    }

    /// The selection pair of the test rows.
    fn selection() -> (Hsla, Hsla) {
        (Hsla::from(rgb(0x2c313a)), Hsla::from(rgb(0xd1d5dc)))
    }

    /// The colours a look resolves against.
    fn defaults() -> FrameDefaults {
        let (selection_background, selection_foreground) = selection();
        FrameDefaults {
            foreground: foreground(),
            background: background(),
            selection_background,
            selection_foreground,
        }
    }

    /// A cell with no colours of its own and the given attributes.
    fn facts(style: Style) -> CellFacts {
        CellFacts {
            foreground: None,
            background: None,
            style,
        }
    }

    #[test]
    fn a_cell_with_no_colours_takes_the_session_defaults() {
        let look = look_from(&facts(Style::default()), &defaults(), false);

        assert_eq!(look.foreground, foreground());
        assert_eq!(look.background, None);
        assert!(!look.bold);
    }

    #[test]
    fn every_attribute_reaches_the_run_it_belongs_to() {
        let style = Style {
            bold: true,
            italic: true,
            underline: Underline::Curly,
            strikethrough: true,
            blink: true,
            ..Style::default()
        };

        let look = look_from(&facts(style), &defaults(), false);

        assert!(look.bold);
        assert!(look.italic);
        assert!(look.underline);
        assert!(look.strikethrough);
        assert!(look.blink);
    }

    #[test]
    fn inverse_swaps_the_pair_and_falls_back_to_the_defaults() {
        let style = Style {
            inverse: true,
            ..Style::default()
        };
        let coloured = CellFacts {
            foreground: Some(red()),
            ..facts(style)
        };

        let plain = look_from(&facts(style), &defaults(), false);
        let look = look_from(&coloured, &defaults(), false);

        assert_eq!(plain.foreground, background());
        assert_eq!(plain.background, Some(foreground()));
        assert_eq!(look.background, Some(red()));
    }

    #[test]
    fn hidden_text_takes_the_colour_behind_it() {
        let style = Style {
            invisible: true,
            ..Style::default()
        };
        let on_red = CellFacts {
            background: Some(red()),
            ..facts(style)
        };

        assert_eq!(
            look_from(&facts(style), &defaults(), false).foreground,
            background()
        );
        assert_eq!(look_from(&on_red, &defaults(), false).foreground, red());
    }

    #[test]
    fn faint_text_keeps_part_of_its_opacity() {
        let style = Style {
            faint: true,
            ..Style::default()
        };

        let look = look_from(&facts(style), &defaults(), false);

        assert!(
            look.foreground.a < foreground().a,
            "{} should be dimmer than {}",
            look.foreground.a,
            foreground().a
        );
        assert_eq!(look.foreground.h, foreground().h);
    }

    #[test]
    fn a_selected_cell_takes_the_selection_pair() {
        let (selection_background, selection_foreground) = selection();
        let coloured = CellFacts {
            foreground: Some(red()),
            background: Some(red()),
            ..facts(Style::default())
        };

        let look = look_from(&coloured, &defaults(), true);

        assert_eq!(look.background, Some(selection_background));
        assert_eq!(look.foreground, selection_foreground);
    }

    /// A row of plain cells, one grapheme cluster each.
    fn plain_row(text: &str) -> Vec<RowCell> {
        text.chars()
            .map(|character| RowCell {
                text: character.to_string(),
                look: CellLook::plain(foreground()),
            })
            .collect()
    }

    /// A cell with no glyph: a blank, or a wide character's spacer.
    fn spacer() -> RowCell {
        RowCell {
            text: String::new(),
            look: CellLook::plain(foreground()),
        }
    }

    #[test]
    fn equal_cells_shape_as_one_run() {
        let row = group_row(3, &plain_row("hello"));

        assert_eq!(row.row, 3);
        assert_eq!(
            row.runs,
            vec![TextRunSpan {
                column: 0,
                text: "hello".to_owned(),
                look: CellLook::plain(foreground()),
            }]
        );
        assert!(row.backgrounds.is_empty());
    }

    #[test]
    fn a_change_of_look_starts_a_new_run() {
        let mut cells = plain_row("abcd");
        cells[2].look.foreground = red();
        cells[3].look.foreground = red();

        let row = group_row(0, &cells);

        assert_eq!(row.runs.len(), 2);
        assert_eq!(row.runs[0].text, "ab");
        assert_eq!(row.runs[1].column, 2);
        assert_eq!(row.runs[1].text, "cd");
        assert_eq!(row.runs[1].look.foreground, red());
    }

    #[test]
    fn an_attribute_change_alone_starts_a_new_run() {
        let mut cells = plain_row("abc");
        cells[1].look.bold = true;

        let row = group_row(0, &cells);

        assert_eq!(row.runs.len(), 3);
        assert!(row.runs[1].look.bold);
        assert_eq!(row.runs[2].column, 2);
    }

    #[test]
    fn a_wide_character_keeps_its_spacer_column_for_the_next_run() {
        let mut cells = vec![
            RowCell {
                text: "中".to_owned(),
                look: CellLook::plain(foreground()),
            },
            spacer(),
        ];
        cells.extend(plain_row("ok"));

        let row = group_row(0, &cells);

        assert_eq!(row.runs.len(), 2);
        assert_eq!(row.runs[0].text, "中");
        assert_eq!(row.runs[1].column, 2, "the ASCII keeps the column grid");
        assert_eq!(row.runs[1].text, "ok");
    }

    #[test]
    fn a_combining_mark_stays_on_its_base_cell() {
        let cells = vec![
            RowCell {
                text: "e\u{301}".to_owned(),
                look: CellLook::plain(foreground()),
            },
            RowCell {
                text: "!".to_owned(),
                look: CellLook::plain(foreground()),
            },
        ];

        let row = group_row(0, &cells);

        assert_eq!(row.runs.len(), 1);
        assert_eq!(row.runs[0].text, "e\u{301}!");
    }

    #[test]
    fn a_blank_cell_breaks_a_run_so_the_columns_stay_aligned() {
        let mut cells = plain_row("ab");
        cells.push(spacer());
        cells.extend(plain_row("cd"));

        let row = group_row(0, &cells);

        assert_eq!(row.runs.len(), 2);
        assert_eq!(row.runs[0].column, 0);
        assert_eq!(row.runs[1].column, 3);
    }

    #[test]
    fn backgrounds_group_across_cells_with_and_without_glyphs() {
        let mut cells = plain_row("ab");
        cells.push(spacer());
        for cell in &mut cells {
            cell.look.background = Some(red());
        }
        cells.extend(plain_row("c"));

        let row = group_row(0, &cells);

        assert_eq!(
            row.backgrounds,
            vec![BackgroundSpan {
                column: 0,
                columns: 3,
                color: red(),
            }]
        );
    }

    #[test]
    fn a_sprite_cell_leaves_the_runs_and_breaks_the_one_it_sits_in() {
        let mut cells = plain_row("ab");
        cells.push(RowCell {
            text: "\u{e0b0}".to_owned(),
            look: CellLook::plain(foreground()),
        });
        cells.extend(plain_row("cd"));

        let row = group_row(0, &cells);

        assert_eq!(row.runs.len(), 2, "the separator breaks the run");
        assert_eq!(row.runs[0].text, "ab");
        assert_eq!(row.runs[1].column, 3, "the text after it keeps its column");
        assert_eq!(row.runs[1].text, "cd");
        assert_eq!(
            row.sprites,
            vec![SpriteSpan {
                column: 2,
                glyph: '\u{e0b0}',
                look: CellLook::plain(foreground()),
            }]
        );
    }

    #[test]
    fn a_sprite_cell_keeps_the_colours_its_look_resolved_to() {
        let cells = vec![RowCell {
            text: "\u{2500}".to_owned(),
            look: CellLook {
                background: Some(red()),
                ..CellLook::plain(red())
            },
        }];

        let row = group_row(0, &cells);

        assert_eq!(row.sprites[0].look.foreground, red());
        assert_eq!(row.backgrounds.len(), 1, "the cell still gets its quad");
        assert!(row.runs.is_empty(), "and no shaped run");
    }

    #[test]
    fn a_sprite_code_point_with_a_combining_mark_stays_in_the_runs() {
        let cells = vec![RowCell {
            text: "\u{2500}\u{0301}".to_owned(),
            look: CellLook::plain(foreground()),
        }];

        let row = group_row(0, &cells);

        assert!(row.sprites.is_empty(), "a cluster is not a sprite");
        assert_eq!(row.runs.len(), 1);
    }

    #[test]
    fn two_adjacent_sprites_each_keep_their_own_column() {
        let cells = vec![
            RowCell {
                text: "\u{2502}".to_owned(),
                look: CellLook::plain(foreground()),
            },
            RowCell {
                text: "\u{2588}".to_owned(),
                look: CellLook::plain(foreground()),
            },
        ];

        let row = group_row(0, &cells);

        assert_eq!(row.sprites.len(), 2);
        assert_eq!(row.sprites[0].column, 0);
        assert_eq!(row.sprites[1].column, 1);
    }

    #[test]
    fn a_gap_in_the_background_starts_a_new_span() {
        let mut cells = plain_row("abcd");
        cells[0].look.background = Some(red());
        cells[2].look.background = Some(red());
        cells[3].look.background = Some(Hsla::from(Rgba {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        }));

        let row = group_row(0, &cells);

        assert_eq!(row.backgrounds.len(), 3);
        assert_eq!(row.backgrounds[0].column, 0);
        assert_eq!(row.backgrounds[1].column, 2);
        assert_eq!(row.backgrounds[2].column, 3);
    }
}
