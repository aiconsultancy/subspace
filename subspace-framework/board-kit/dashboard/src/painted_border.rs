//! Reads painted borders from the last frame's scene in tests.
//!
//! GPUI Fast paints a plain border as strips of the border colour: each strip
//! is the whole quad masked to one edge, with no border widths of its own.
//! Other borders keep one quad with border widths. Both forms read the same
//! here.

use gpui_kit::{Background, Bounds, Edges, Hsla, ScaledPixels, Window};

/// Each painted border of `colour`: the quad's bounds and its edge widths.
pub(crate) fn painted_borders(
    window: &Window,
    colour: Hsla,
) -> Vec<(Bounds<ScaledPixels>, Edges<ScaledPixels>)> {
    let fill = Background::from(colour);
    let mut borders: Vec<(Bounds<ScaledPixels>, Edges<ScaledPixels>)> = Vec::new();
    for quad in window.painted_quads() {
        let none = quad.border_widths == Edges::default();
        if !none && quad.border_color == colour {
            borders.push((quad.bounds, quad.border_widths));
            continue;
        }
        let mask = quad.content_mask.bounds;
        if !none || quad.background != fill || mask == quad.bounds {
            continue;
        }
        let index = match borders
            .iter()
            .position(|(bounds, _)| *bounds == quad.bounds)
        {
            Some(index) => index,
            None => {
                borders.push((quad.bounds, Edges::default()));
                borders.len() - 1
            }
        };
        add_strip(&mut borders[index].1, quad.bounds, mask);
    }
    borders
}

fn add_strip(
    edges: &mut Edges<ScaledPixels>,
    bounds: Bounds<ScaledPixels>,
    strip: Bounds<ScaledPixels>,
) {
    let full_width = strip.size.width == bounds.size.width;
    if full_width && strip.top() == bounds.top() {
        edges.top = strip.size.height;
    } else if full_width && strip.bottom() == bounds.bottom() {
        edges.bottom = strip.size.height;
    } else if strip.left() == bounds.left() {
        edges.left = strip.size.width;
    } else {
        edges.right = strip.size.width;
    }
}
