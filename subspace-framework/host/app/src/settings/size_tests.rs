use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{App, Entity, TestAppContext, Window, px};
use subspace_dashboard::Tokens;
use subspace_dashboard::density::Density;

use super::organization_tests::open_settings;

/// Replaces the entry as typing does, which emits `InputEvent::Change`.
pub(super) fn type_font_size(
    input: &Entity<InputState>,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) {
    input.update(cx, |input, cx| {
        input.replace_all(text.to_owned(), window, cx)
    });
}

fn leave_font_size(input: &Entity<InputState>, cx: &mut App) {
    input.update(cx, |_, cx| cx.emit(InputEvent::Blur));
}

#[gpui_kit::test]
fn font_size_entries_change_only_their_role_and_clamp_when_left(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            type_font_size(&settings.font_sizes[0], "15.5", window, cx);
            type_font_size(&settings.font_sizes[1], "5", window, cx);
            type_font_size(&settings.font_sizes[2], "100", window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    settings
        .update(cx, |settings, _, cx| {
            let density = cx.global::<Density>();
            assert_eq!(density.get("--heading-font"), 15.5);
            assert_eq!(
                density.get("--row-font"),
                13.,
                "out of range is not previewed"
            );
            assert_eq!(density.get("--mono-font"), 13.);
            leave_font_size(&settings.font_sizes[1], cx);
            leave_font_size(&settings.font_sizes[2], cx);
        })
        .unwrap();
    cx.run_until_parked();
    settings
        .update(cx, |settings, _, cx| {
            assert_eq!(cx.global::<Density>().get("--row-font"), 10.);
            assert_eq!(cx.global::<Density>().get("--mono-font"), 20.);
            assert_eq!(settings.font_sizes[1].read(cx).value().as_ref(), "10");
            assert_eq!(settings.font_sizes[2].read(cx).value().as_ref(), "20");
            assert_eq!(Tokens::current(cx).mono_font, px(20.));
        })
        .unwrap();
}

#[gpui_kit::test]
fn the_tab_size_is_its_own_setting(cx: &mut TestAppContext) {
    use subspace_dashboard::density::TAB_FONT_SIZE;
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            type_font_size(&settings.font_sizes[TAB_FONT_SIZE], "12", window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    settings
        .update(cx, |_, _, cx| {
            let density = cx.global::<Density>();
            assert_eq!(density.get("--tab-font"), 12.);
            assert_eq!(
                density.get("--heading-font"),
                15.,
                "headings keep their size"
            );
            assert_eq!(Tokens::current(cx).tab_font, px(12.));
        })
        .unwrap();
}
