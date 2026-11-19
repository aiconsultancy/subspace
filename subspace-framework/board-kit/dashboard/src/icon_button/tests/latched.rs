use super::{IconButton, IconButtonSize};
use gpui_kit::component::{Disableable as _, Selectable as _, h_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, IntoElement, ParentElement as _, Render, Role,
    StyleRefinement, Styled as _, TestAppContext, Window, px, size,
};
use subspace_lucide::IconName;

struct Fixture {
    focus: FocusHandle,
    latched: bool,
    disabled: bool,
    loading: bool,
    calls: usize,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .child(
                IconButton::new("latched")
                    .icon(IconName::TextWrap)
                    .label("Wrap")
                    .accessibility_label("Wrap long lines")
                    .tooltip("Wrap long lines (W)")
                    .latched(self.latched)
                    .disabled(self.disabled)
                    .loading(self.loading)
                    .track_focus(&self.focus)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.latched = !this.latched;
                        this.calls += 1;
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("ordinary")
                    .icon(IconName::Search)
                    .label("Search"),
            )
    }
}

fn fixture(cx: &mut Context<Fixture>) -> Fixture {
    Fixture {
        focus: cx.focus_handle(),
        latched: false,
        disabled: false,
        loading: false,
        calls: 0,
    }
}

#[test]
fn latched_selects_the_native_surface_without_background_or_radius_overrides() {
    for on in [false, true] {
        let mut button = IconButton::new("surface").latched(on).button;
        assert_eq!(button.is_selected(), on);
        assert_eq!(button.style(), &StyleRefinement::default());
    }
}

#[gpui_kit::test]
fn latched_reports_both_pressed_values_and_preserves_global_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1000.), px(300.)), |_, cx| fixture(cx));
    let mut label_height = None;
    for choice in IconButtonSize::ALL {
        cx.update(|cx| choice.apply(cx));
        cx.update_window(*handle, |_, window, cx| {
            window.render_frame(cx);
            let button = window.find("latched");
            assert_eq!(button.role(), Some(Role::Button));
            assert_eq!(button.label(), Some("Wrap long lines"));
            // Kit reads these directly from AccessKit Node::toggled, not a
            // test-only value. Button + Toggled is its toggle-button contract.
            assert_eq!(button.checked(), Some(false));
            assert_eq!(button.indeterminate(), Some(false));
            assert_eq!(window.find("ordinary").checked(), None);
            assert_eq!(
                button.bounds().size.height,
                choice.metrics().target().to_pixels(px(16.))
            );
            let glyph = window.within("latched").find("toolbar-icon").bounds();
            assert_eq!(
                glyph.size.height,
                choice.metrics().glyph().to_pixels(px(16.))
            );
            let label = window.within("latched").find("icon-button-label").bounds();
            assert_eq!(
                label.size.height,
                *label_height.get_or_insert(label.size.height)
            );
            window.click("latched", cx);
            assert_eq!(window.find("latched").role(), Some(Role::Button));
            assert_eq!(window.find("latched").checked(), Some(true));
            assert_eq!(window.find("latched").indeterminate(), Some(false));
            window.click("latched", cx);
            assert_eq!(window.find("latched").checked(), Some(false));
        })
        .unwrap();
    }
    assert_eq!(handle.read_with(cx, |this, _| this.calls).unwrap(), 8);
}

#[gpui_kit::test]
fn latched_keeps_native_keyboard_focus_and_rejects_disabled_or_loading_activation(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1000.), px(300.)), |_, cx| fixture(cx));
    handle
        .update(cx, |this, window, cx| this.focus.focus(window, cx))
        .unwrap();
    cx.update_window(*handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("latched").focused(), Some(true));
        window.press("space", cx);
        assert_eq!(window.find("latched").checked(), Some(true));
        window.press("enter", cx);
        assert_eq!(window.find("latched").checked(), Some(false));
    })
    .unwrap();
    for (disabled, loading) in [(true, false), (false, true)] {
        handle
            .update(cx, |this, _, cx| {
                this.disabled = disabled;
                this.loading = loading;
                cx.notify();
            })
            .unwrap();
        cx.update_window(*handle, |_, window, cx| {
            window.click("latched", cx);
            window.press("space", cx);
            window.press("enter", cx);
            assert_eq!(window.find("latched").checked(), Some(false));
        })
        .unwrap();
    }
    assert_eq!(handle.read_with(cx, |this, _| this.calls).unwrap(), 2);
}
