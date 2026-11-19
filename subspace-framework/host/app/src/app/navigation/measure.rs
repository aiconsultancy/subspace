//! Shape navigation labels with the same font the rail renders.

use gpui_kit::{Pixels, SharedString, Window};
use subspace_dashboard::measure::text_width;

pub fn shaped_width(
    window: &Window,
    text: &str,
    font_size: Pixels,
    font_family: SharedString,
) -> f32 {
    f32::from(text_width(window, text, font_family, font_size))
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::ActiveTheme as _;
    use gpui_kit::{TestAppContext, px};

    use super::shaped_width;
    use crate::app::SubspaceApp;

    #[gpui_kit::test]
    fn a_larger_font_shapes_the_same_label_wider(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
            crate::theme::initialize(cx);
            cx.set_global(crate::registry::BoardFactories(Vec::new()));
        });
        let window = cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx));
        window
            .update(cx, |_, window, cx| {
                let family = cx.theme().font_family.clone();
                let body = shaped_width(window, "Subspace", px(16.), family.clone());
                let badge = shaped_width(window, "Subspace", px(11.), family);
                assert!(body > 0.);
                assert!(
                    body > badge,
                    "the same glyphs must shape wider at the row font than at badge size: {body} vs {badge}"
                );
                assert_eq!(
                    shaped_width(window, "", px(16.), cx.theme().font_family.clone()),
                    0.
                );
            })
            .unwrap();
    }
}
