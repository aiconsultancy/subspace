use super::{IconButton, IconButtonSize};
use gpui_kit::component::{Theme, h_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext,
    Window, px, size,
};
use subspace_lucide::IconName;

const LABEL: &str = "Could not check for updates. Fixture service unavailable.";

struct Labels;

impl Render for Labels {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_4()
            .child(
                IconButton::new("bounded")
                    .icon(IconName::RefreshCw)
                    .label(LABEL)
                    .tooltip(LABEL)
                    .w_48()
                    .min_w_0(),
            )
            .child(
                IconButton::new("compact-bounded")
                    .icon(IconName::RefreshCw)
                    .label(LABEL)
                    .tooltip(LABEL)
                    .compact()
                    .w_48()
                    .min_w_0(),
            )
            .child(
                IconButton::new("natural")
                    .icon(IconName::Search)
                    .label("Find"),
            )
    }
}

fn assert_bounded_label(window: &mut Window, id: &'static str) {
    let button = window.find(id);
    let scope = window.within(id);
    let glyph = scope.find("toolbar-icon").bounds();
    let label = scope.find("icon-button-label").bounds();
    assert_eq!(button.label(), Some(LABEL));
    assert!(glyph.left() >= button.bounds().left());
    assert!(label.left() >= glyph.right());
    assert!(label.right() <= button.bounds().right());
    assert!(label.size.width > px(0.));
}

#[gpui_kit::test]
fn bounded_labels_shrink_inside_the_button_without_changing_intrinsic_short_labels(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let window = cx.open_window(size(px(1400.), px(300.)), |_, _| Labels);
    for font_size in [16., 20.] {
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        for choice in IconButtonSize::ALL {
            cx.update(|cx| choice.apply(cx));
            cx.update_window(*window, |_, window, cx| {
                window.render_frame(cx);
                for id in ["bounded", "compact-bounded"] {
                    assert_bounded_label(window, id);
                }
                let natural = window.find("natural");
                let label = window.within("natural").find("icon-button-label").bounds();
                assert_eq!(natural.label(), Some("Find"));
                assert!(label.left() > natural.bounds().left());
                assert!(label.right() < natural.bounds().right());
                assert!(natural.bounds().size.width < window.find("bounded").bounds().size.width);
            })
            .unwrap();
        }
    }
}
