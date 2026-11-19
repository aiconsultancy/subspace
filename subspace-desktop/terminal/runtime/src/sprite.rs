//! The glyphs this crate draws itself, filling their cell exactly.
//!
//! A terminal font draws a Powerline separator, a box-drawing line or a
//! block element at the glyph's own size. The cell is the wrong size for
//! that: `adjust-cell-height` makes a row taller than the font's line, so
//! box lines leave gaps between rows and a Powerline separator no longer
//! meets the cell background beside it. Ghostty answers this with a sprite
//! font that draws these code points from the cell's own measurements, and
//! this module is the same answer in GPUI primitives:
//!
//! | Range | Module |
//! | --- | --- |
//! | U+2500 to U+257F, box drawing | [`box_drawing`], [`box_lines`], [`box_table`] |
//! | U+2580 to U+259F, block elements | [`block`] |
//! | U+E0B0 to U+E0BF, U+E0D2, U+E0D4, Powerline | [`powerline`] |
//!
//! The geometry is ported from Ghostty's `src/font/sprite/draw` (MIT,
//! credited in `licenses/terminal`). It stays pure: [`shapes`] turns a code
//! point and a cell into a list of [`Shape`]s, which the tests check without
//! a window, and [`paint`] is the only part that draws.
//!
//! A cell reaches a sprite only when its whole grapheme cluster is one of
//! these code points: a combining mark or a double-width spacer keeps the
//! shaped-text path.

mod block;
mod box_drawing;
mod box_lines;
mod box_table;
mod paint;
mod powerline;
mod shape;

use gpui_kit::{Bounds, Hsla, Pixels, Window};

use crate::sprite::paint::SpritePaint;
use crate::sprite::shape::{HALF, Shape, SpriteCell, bled};

/// How far a filled or stroked edge is pushed past the cell edge it sits
/// on, in device pixels. Half a pixel covers the boundary pixel without
/// reaching the cell beyond it.
const EDGE_BLEED_PIXELS: f32 = HALF;

/// The glyph a cell draws as a sprite, and `None` for a cell this crate
/// leaves to the font: an empty cell, a cluster with a combining mark, or
/// any code point outside the three ranges.
pub(crate) fn sprite_of(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let first = characters.next()?;
    if characters.next().is_some() || !is_sprite(first) {
        return None;
    }
    Some(first)
}

/// Whether one code point is drawn as a sprite.
fn is_sprite(code_point: char) -> bool {
    matches!(
        u32::from(code_point),
        0x2500..=0x257f | 0x2580..=0x259f | 0xe0b0..=0xe0bf | 0xe0d2 | 0xe0d4
    )
}

/// Draw one sprite glyph, filling the cell it covers exactly.
// qual:allow(test_quality, untested) reason: "Runs only while GPUI paints TerminalElement (Element::paint), a dispatch the call graph cannot follow; view_tests draws the element with render_frame."
pub(crate) fn paint(glyph: char, bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let cell = SpriteCell::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
    let drawn = bled(shapes(glyph, cell), cell, edge_bleed(window.scale_factor()));
    paint::paint_shapes(&drawn, &SpritePaint { bounds, color }, window);
}

/// How far an edge-touching path is pushed past the cell edge, in points.
fn edge_bleed(scale_factor: f32) -> f32 {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return EDGE_BLEED_PIXELS;
    }
    EDGE_BLEED_PIXELS / scale_factor
}

/// The shapes one sprite glyph is drawn from, in points inside the cell.
fn shapes(glyph: char, cell: SpriteCell) -> Vec<Shape> {
    let code_point = u32::from(glyph);
    powerline::shapes(code_point, cell)
        .or_else(|| box_drawing::shapes(code_point, cell))
        .or_else(|| block::shapes(code_point, cell))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::sprite::shape::SpriteCell;
    use crate::sprite::{edge_bleed, is_sprite, shapes, sprite_of};

    /// A cell of plausible size for a 13 point font with a fifth more height.
    const CELL: SpriteCell = SpriteCell {
        width: 8.0,
        height: 19.0,
        thickness: 1.0,
    };

    #[test]
    fn every_covered_range_is_a_sprite_and_nothing_else_is() {
        assert!(is_sprite('\u{2500}'));
        assert!(is_sprite('\u{257f}'));
        assert!(is_sprite('\u{2588}'));
        assert!(is_sprite('\u{e0b0}'));
        assert!(is_sprite('\u{e0d4}'));
        assert!(!is_sprite('\u{24ff}'));
        assert!(!is_sprite('\u{25a0}'));
        assert!(!is_sprite('\u{e0c0}'));
        assert!(!is_sprite('a'));
    }

    #[test]
    fn a_cell_holding_one_sprite_code_point_draws_a_sprite() {
        assert_eq!(sprite_of("\u{e0b0}"), Some('\u{e0b0}'));
        assert_eq!(sprite_of("\u{2500}"), Some('\u{2500}'));
    }

    #[test]
    fn an_empty_cell_or_ordinary_text_keeps_the_font() {
        assert_eq!(sprite_of(""), None, "a blank or a wide spacer");
        assert_eq!(sprite_of("a"), None);
        assert_eq!(sprite_of("中"), None);
    }

    #[test]
    fn a_cluster_with_a_combining_mark_keeps_the_font() {
        let marked = "\u{2500}\u{0301}";

        assert_eq!(sprite_of(marked), None, "a sprite is one code point alone");
        assert_eq!(sprite_of("e\u{0301}"), None);
    }

    #[test]
    fn each_range_reaches_the_module_that_draws_it() {
        assert!(!shapes('\u{e0b0}', CELL).is_empty(), "Powerline");
        assert!(!shapes('\u{2502}', CELL).is_empty(), "box drawing");
        assert!(!shapes('\u{2588}', CELL).is_empty(), "block elements");
    }

    #[test]
    fn a_code_point_no_module_draws_produces_no_shapes() {
        assert!(shapes('a', CELL).is_empty());
    }

    #[test]
    fn an_edge_bleeds_half_a_device_pixel_at_every_scale() {
        assert_eq!(edge_bleed(1.0), 0.5, "half a point at 1x");
        assert_eq!(edge_bleed(2.0), 0.25, "a quarter of a point at 2x");
        assert_eq!(edge_bleed(0.0), 0.5, "an unusable scale falls back");
        assert_eq!(edge_bleed(f32::NAN), 0.5);
    }
}
