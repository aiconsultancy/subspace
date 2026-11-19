//! The designer's sections: live preview in other views, Save and reset.

use gpui_kit::component::{Theme, try_parse_color};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle};
use subspace_dashboard::Tokens;
use subspace_dashboard::density;
use subspace_dashboard::tokens::{ButtonFinish, ToolbarLook};

use super::super::Jig;
use super::{initialize, open_jig, open_section, set_color, settle};
use crate::theme::ThemeDocument;

struct ButtonPreview;

impl gpui_kit::Render for ButtonPreview {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use subspace_dashboard::action::{Intent, dialog_button};
        dialog_button("preview-save", Intent::Primary, None, "Save")
    }
}

fn preview_height(
    window: WindowHandle<ButtonPreview>,
    cx: &mut TestAppContext,
) -> gpui_kit::Pixels {
    cx.update_window(window.into(), |_, window, cx| {
        window.set_rem_size(gpui_kit::px(density::REFERENCE_REM));
        window.render_frame(cx);
        window.find("preview-save").bounds().size.height
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_control_token_edit_previews_in_other_views_until_cancel(cx: &mut TestAppContext) {
    use gpui_kit::component::slider::SliderEvent;
    let root = tempfile::tempdir().unwrap();
    initialize(root.path(), cx);
    let preview = cx.add_window(|_, _| ButtonPreview);
    assert_eq!(preview_height(preview, cx), gpui_kit::px(21.));
    let jig = open_jig(cx);
    jig.update(cx, |jig, _, cx| {
        let (_, slider) = jig
            .sliders
            .iter()
            .find(|(control, _)| control.token == "--dialog-button-h")
            .expect("the designer edits the dialog button height");
        slider.update(cx, |_, cx| cx.emit(SliderEvent::Change(30.0.into())));
    })
    .unwrap();
    assert_eq!(
        preview_height(preview, cx),
        gpui_kit::px(30.),
        "no Save needed"
    );
    jig.update(cx, |jig, window, cx| jig.cancel(window, cx))
        .unwrap();
    assert_eq!(preview_height(preview, cx), gpui_kit::px(21.));
}

/// Drag one density slider to `value`, as the person would.
fn slide(jig: WindowHandle<Jig>, token: &str, value: f32, cx: &mut TestAppContext) {
    use gpui_kit::component::slider::SliderEvent;
    jig.update(cx, |jig, _, cx| {
        let (_, slider) = jig
            .sliders
            .iter()
            .find(|(control, _)| control.token == token)
            .expect("the designer edits every density token");
        slider.update(cx, |_, cx| cx.emit(SliderEvent::Change(value.into())));
    })
    .unwrap();
}

/// Edit one value in each section: body size and row height, the Liquid
/// glass switch, glass toolbar buttons at rest and on hover and GPUI Kit's
/// radius, and a derived title-bar colour and the flat button hover fill.
fn edit_every_section(jig: WindowHandle<Jig>, cx: &mut TestAppContext) {
    use gpui_kit::component::slider::SliderEvent;
    slide(jig, "--row-font", 15., cx);
    slide(jig, "--row-h", 40., cx);
    open_section(jig, super::super::Section::Controls, cx);
    cx.update_window(jig.into(), |_, window, cx| {
        window.click("jig-switch--liquid-glass", cx);
        window.click(("--toolbar-button-rest", 1usize), cx);
        window.click(("--toolbar-button-hover", 1usize), cx);
    })
    .unwrap();
    jig.update(cx, |jig, window, cx| {
        let slider = jig.radii[0].2.clone();
        slider.update(cx, |slider, cx| slider.set_value(3., window, cx));
        slider.update(cx, |_, cx| cx.emit(SliderEvent::Change(3.0.into())));
        set_color(jig, "title_bar.background", "#224466", window, cx);
        set_color(jig, "button-hover", "#ffffff22", window, cx);
    })
    .unwrap();
}

const GLASS: ToolbarLook = ToolbarLook {
    rest: ButtonFinish::Glass,
    hover: ButtonFinish::Glass,
};

fn edited(cx: &mut TestAppContext) -> WindowHandle<Jig> {
    let root = tempfile::tempdir().unwrap().keep();
    initialize(&root, cx);
    let jig = open_jig(cx);
    edit_every_section(jig, cx);
    jig
}

#[gpui_kit::test]
fn each_section_previews_live_without_save(cx: &mut TestAppContext) {
    edited(cx);
    cx.update(|cx| {
        let tokens = Tokens::current(cx);
        assert_eq!(tokens.row_height, gpui_kit::px(40.));
        assert!(tokens.controls.liquid_glass);
        assert_eq!(tokens.controls.toolbar_look, GLASS);
        assert_eq!(tokens.button_hover, try_parse_color("#ffffff22").unwrap());
        let theme = Theme::global(cx);
        assert_eq!(theme.font_size, gpui_kit::px(14. * 15. / 13.));
        assert_eq!(theme.radius, gpui_kit::px(3.));
        assert_eq!(theme.title_bar, try_parse_color("#224466").unwrap());
    });
}

#[gpui_kit::test]
fn save_keeps_every_section_in_the_theme_file(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    initialize(root.path(), cx);
    let jig = open_jig(cx);
    edit_every_section(jig, cx);
    jig.update(cx, |jig, window, cx| {
        jig.name.update(cx, |input, cx| {
            input.set_value("Designer Theme", window, cx)
        });
        jig.save(false, window, cx);
    })
    .unwrap();
    settle(cx);
    let saved = std::fs::read_to_string(root.path().join("themes/Designer Theme.json")).unwrap();
    let saved = ThemeDocument::parse(&saved).unwrap();
    let density = saved.metadata().unwrap().density;
    assert_eq!(density["--row-h"], "40px");
    assert_eq!(density["--row-font"], "15px");
    assert_eq!(density["--liquid-glass"], "1");
    assert_eq!(density["--toolbar-button-rest"], "glass");
    assert_eq!(density["--toolbar-button-hover"], "glass");
    assert_eq!(saved.color("button-hover").as_deref(), Some("#ffffff22"));
    assert_eq!(saved.radius("radius"), Some(3.));
    assert_eq!(
        saved.color("title_bar.background").as_deref(),
        Some("#224466")
    );
}

#[gpui_kit::test]
fn section_resets_return_to_the_base_theme_live(cx: &mut TestAppContext) {
    let jig = edited(cx);
    jig.update(cx, |jig, window, cx| {
        for section in [
            super::super::Section::Spacing,
            super::super::Section::Controls,
            super::super::Section::Colours,
        ] {
            jig.reset_section(section, window, cx);
        }
    })
    .unwrap();
    cx.update(|cx| {
        let tokens = Tokens::current(cx);
        assert_eq!(
            tokens.row_height,
            gpui_kit::px(28.),
            "the base theme's row height"
        );
        assert!(!tokens.controls.liquid_glass);
        assert_eq!(tokens.controls.toolbar_look, ToolbarLook::FLAT);
        assert_ne!(tokens.button_hover, try_parse_color("#ffffff22").unwrap());
        let theme = Theme::global(cx);
        assert_ne!(theme.radius, gpui_kit::px(3.));
        assert_ne!(theme.title_bar, try_parse_color("#224466").unwrap());
    });
}
