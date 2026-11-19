//! Shaping one session's text: the font each run draws with and the cell the
//! grid is built from (REQ-13, REQ-14).
//!
//! A run's family follows its weight and slant, so `font-family-bold` and the
//! rest of the Ghostty font keys reach the glyphs. The cell comes from shaping
//! `M` once per layout: its advance is the column width and its ascent plus
//! descent the row height, which `adjust-cell-height` then adjusts. Both are
//! snapped to whole device pixels, as Ghostty rounds its own cell to whole
//! pixels, so a cell edge never falls inside a pixel.

use gpui_kit::{
    Font, FontFeatures, FontStyle, FontWeight, Pixels, SharedString, StrikethroughStyle, TextRun,
    TextSystem, UnderlineStyle, Window, black, px,
};

use crate::frame::CellLook;
use crate::metrics::{CellMetrics, snapped_cell};
use crate::style::TerminalFonts;

/// How thick an underline and a strike-through are drawn, in points.
const LINE_THICKNESS: f32 = 1.0;

/// The share of the font size a cell falls back to when the text system
/// cannot measure the font, which keeps the grid arithmetic sound.
const FALLBACK_CELL_WIDTH: f32 = 0.6;

const MEASUREMENT_SIZE: f32 = 16.0;
const ADVANCE_TOLERANCE: f32 = 0.01;

/// Check normal terminal characters by advance, including in fonts whose
/// extra symbols make platform metadata report varying glyph widths.
pub(crate) fn fixed_width(text: &TextSystem, face: &Font) -> bool {
    let id = text.resolve_font(face);
    let width = |ch| {
        text.advance(id, px(MEASUREMENT_SIZE), ch)
            .ok()
            .map(|size| size.width)
    };
    width('M').is_some_and(|base| {
        base > px(0.0)
            && ['i', '0', ' '].into_iter().all(|ch| {
                width(ch).is_some_and(|value| (value - base).abs() < px(ADVANCE_TOLERANCE))
            })
    })
}

/// One cell, measured by shaping `M` in the session's own font and snapped
/// to whole device pixels (REQ-14).
pub(crate) fn measure_cell(fonts: &TerminalFonts, window: &Window) -> CellMetrics {
    let shaped =
        window
            .text_system()
            .shape_line("M".into(), fonts.size, &[measuring_run(fonts)], None);
    let measured = CellMetrics {
        width: cell_width(shaped.width(), fonts.size),
        height: f32::from(shaped.ascent + shaped.descent),
    };
    snapped_cell(measured, fonts.cell_height, window.scale_factor())
}

/// One shaped run's font, colour and decorations.
pub(crate) fn text_run(len: usize, look: CellLook, fonts: &TerminalFonts) -> TextRun {
    TextRun {
        len,
        font: run_font(look, fonts),
        color: look.foreground,
        // The background and the selection are painted as quads, so a cell
        // with no glyph still gets its colour.
        background_color: None,
        underline: look.underline.then(|| UnderlineStyle {
            thickness: px(LINE_THICKNESS),
            color: Some(look.foreground),
            wavy: false,
        }),
        strikethrough: look.strikethrough.then(|| StrikethroughStyle {
            thickness: px(LINE_THICKNESS),
            color: Some(look.foreground),
        }),
    }
}

/// A run that measures one cell and draws nothing.
fn measuring_run(fonts: &TerminalFonts) -> TextRun {
    text_run(1, CellLook::plain(black()), fonts)
}

/// The family, weight and slant a run draws with (REQ-14).
fn run_font(look: CellLook, fonts: &TerminalFonts) -> Font {
    Font {
        family: family_for(look, fonts),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: if look.bold {
            FontWeight(fonts.base_weight.0.max(FontWeight::BOLD.0))
        } else {
            fonts.base_weight
        },
        style: if look.italic {
            FontStyle::Italic
        } else {
            fonts.base_style
        },
    }
}

/// The configured family for a run's weight and slant.
fn family_for(look: CellLook, fonts: &TerminalFonts) -> SharedString {
    match (look.bold, look.italic) {
        (true, true) => fonts.bold_italic.clone(),
        (true, false) => fonts.bold.clone(),
        (false, true) => fonts.italic.clone(),
        (false, false) => fonts.regular.clone(),
    }
}

/// The width of one cell: the shaped advance, or a share of the font size
/// when the text system could not measure the font.
fn cell_width(advance: Pixels, size: Pixels) -> f32 {
    let advance = f32::from(advance);
    if advance > 0.0 {
        return advance;
    }
    f32::from(size) * FALLBACK_CELL_WIDTH
}

#[cfg(test)]
mod tests {
    use gpui_kit::{FontStyle, FontWeight, Hsla, px, rgb};

    use crate::font::{cell_width, family_for, text_run};
    use crate::frame::CellLook;
    use crate::style::TerminalFonts;

    /// One family per weight and slant, so a test can tell them apart.
    fn fonts() -> TerminalFonts {
        TerminalFonts {
            regular: "Regular".into(),
            bold: "Bold".into(),
            italic: "Italic".into(),
            bold_italic: "Bold Italic".into(),
            base_weight: FontWeight::NORMAL,
            base_style: FontStyle::Normal,
            size: px(13.),
            cell_height: None,
        }
    }

    /// A look with the given weight and slant.
    fn look(bold: bool, italic: bool) -> CellLook {
        CellLook {
            bold,
            italic,
            ..CellLook::plain(Hsla::from(rgb(0xabb2bf)))
        }
    }

    #[test]
    fn each_weight_and_slant_takes_its_own_family() {
        let fonts = fonts();

        assert_eq!(family_for(look(false, false), &fonts), "Regular");
        assert_eq!(family_for(look(true, false), &fonts), "Bold");
        assert_eq!(family_for(look(false, true), &fonts), "Italic");
        assert_eq!(family_for(look(true, true), &fonts), "Bold Italic");
    }

    #[test]
    fn inherited_semibold_applies_to_plain_text_and_measurement() {
        let fonts = TerminalFonts {
            base_weight: FontWeight::SEMIBOLD,
            ..fonts()
        };
        assert_eq!(
            text_run(1, look(false, false), &fonts).font.weight,
            FontWeight::SEMIBOLD
        );
        assert_eq!(
            text_run(1, look(true, false), &fonts).font.weight,
            FontWeight::BOLD
        );
    }

    #[test]
    fn a_decorated_run_carries_its_underline_and_strike() {
        let decorated = CellLook {
            underline: true,
            strikethrough: true,
            ..look(false, false)
        };

        let plain = text_run(2, look(false, false), &fonts());
        let run = text_run(2, decorated, &fonts());

        assert!(plain.underline.is_none());
        assert!(plain.strikethrough.is_none());
        assert!(run.underline.is_some());
        assert!(run.strikethrough.is_some());
        assert_eq!(run.len, 2);
    }

    #[test]
    fn a_font_the_text_system_cannot_measure_still_gives_a_cell_width() {
        assert_eq!(cell_width(px(0.), px(13.)), 7.8);
        assert_eq!(cell_width(px(7.5), px(13.)), 7.5);
    }
}
