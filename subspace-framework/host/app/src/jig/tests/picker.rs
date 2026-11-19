//! The colour fields: GPUI Kit's colour select edits each theme colour.

use gpui_kit::component::{Theme, try_parse_color};
use gpui_kit::{AppContext as _, Entity, TestAppContext};

use super::super::Jig;
use super::{initialize, open_jig, settle};

fn color_picker(
    jig: &Jig,
    key: &str,
) -> Entity<gpui_kit::component::color_picker::ColorPickerState> {
    let field = jig.colors.iter().find(|field| field.key == key).unwrap();
    field.picker.clone()
}

#[gpui_kit::test]
fn a_picked_colour_previews_and_saves_as_the_hex_it_shows(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    initialize(root.path(), cx);
    let jig = open_jig(cx);
    let picker = jig
        .read_with(cx, |jig, _| color_picker(jig, "status_bar.background"))
        .unwrap();
    // Typing in the picker's hex field commits the colour, as picking does.
    cx.update_window(jig.into(), |_, window, cx| {
        picker.update(cx, |picker, cx| picker.commit_hex("#123456", window, cx));
    })
    .unwrap();
    settle(cx);
    jig.read_with(cx, |jig, cx| {
        let field = jig
            .colors
            .iter()
            .find(|field| field.key == "status_bar.background")
            .unwrap();
        assert_eq!(field.value(cx), "#123456");
        assert_eq!(
            Theme::global(cx).colors.status_bar,
            try_parse_color("#123456").unwrap()
        );
        let document = jig.edited_document(cx).unwrap();
        assert_eq!(
            document.color("status_bar.background").as_deref(),
            Some("#123456")
        );
    })
    .unwrap();
}
