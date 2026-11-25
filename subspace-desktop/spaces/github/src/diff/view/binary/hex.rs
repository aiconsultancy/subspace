//! A virtual list of fixed-width hex and ASCII rows for the current page.

use std::fmt::Write as _;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    Context, IntoElement, ListHorizontalSizingBehavior, ParentElement as _, Styled as _, div, px,
    uniform_list,
};
use subspace_client_fleet::diff::DiffContent;
use subspace_framework::dashboard::Tokens;

use super::{BinaryView, DiffView};
use crate::diff::view::{CODE_FONT_PX, code_row_height};

const BYTES_PER_ROW: usize = 8;

pub(super) fn list(
    binary: &BinaryView,
    page: &DiffContent,
    tokens: &Tokens,
    cx: &mut Context<DiffView>,
) -> impl IntoElement {
    uniform_list(
        "crew-binary-dump",
        page.bytes.len().div_ceil(BYTES_PER_ROW),
        cx.processor(
            |view: &mut DiffView, range: std::ops::Range<usize>, _, cx| {
                let tokens = Tokens::current(cx);
                let Some(page) = view.binary.as_ref().and_then(|binary| binary.page.as_ref())
                else {
                    return Vec::new();
                };
                range
                    .map(|index| {
                        let start = index * BYTES_PER_ROW;
                        let end = (start + BYTES_PER_ROW).min(page.bytes.len());
                        div()
                            .h(px(code_row_height(&tokens)))
                            .px_3()
                            .whitespace_nowrap()
                            .child(hex_row(page.offset + start as u64, &page.bytes[start..end]))
                    })
                    .collect::<Vec<_>>()
            },
        ),
    )
    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
    .track_scroll(&binary.scroll)
    .flex_1()
    .min_h_0()
    .w_full()
    .font_family(cx.theme().mono_font_family.clone())
    .text_size(tokens.mono_size(CODE_FONT_PX))
}

fn hex_row(offset: u64, bytes: &[u8]) -> String {
    let mut row = format!("{offset:016x}  ");
    for byte in bytes {
        _ = write!(&mut row, "{byte:02x} ");
    }
    for _ in bytes.len()..BYTES_PER_ROW {
        row.push_str("   ");
    }
    row.push_str("  |");
    row.extend(bytes.iter().map(|&byte| {
        if byte.is_ascii_graphic() || byte == b' ' {
            char::from(byte)
        } else {
            '.'
        }
    }));
    row.push('|');
    row
}

#[cfg(test)]
mod tests {
    use super::hex_row;

    #[test]
    fn hex_rows_preserve_offsets_and_show_only_printable_ascii() {
        assert_eq!(
            hex_row(0x100, b"A\0\xFF z"),
            "0000000000000100  41 00 ff 20 7a            |A.. z|"
        );
    }
}
