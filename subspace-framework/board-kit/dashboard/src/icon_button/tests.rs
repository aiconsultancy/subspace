use super::{IconButton, IconButtonSize};
mod dialog;
mod focus_policy;
mod latched;
mod overflow;
mod roles;
mod surface;
use crate::anchored::AnchoredContextMenuExt as _;
use gpui_kit::component::{
    Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Context, FocusHandle, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, px, size,
};
use subspace_lucide::IconName;

struct Fixture;

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex().child(
            IconButton::new("plain")
                .icon(IconName::Search)
                .accessibility_label("Search"),
        )
    }
}

#[test]
fn finite_size_mapping_defaults_and_round_trips() {
    assert_eq!(IconButtonSize::from_key(None), IconButtonSize::Small);
    assert_eq!(
        IconButtonSize::from_key(Some("invalid")),
        IconButtonSize::Small
    );
    for (choice, glyph, target, gap, row) in [
        (IconButtonSize::Small, 14., 24., 4., 30.),
        (IconButtonSize::Medium, 20., 36., 6., 45.),
        (IconButtonSize::Large, 28., 48., 8., 60.),
        (IconButtonSize::ExtraLarge, 40., 72., 12., 90.),
    ] {
        assert_eq!(IconButtonSize::from_key(Some(choice.key())), choice);
        assert!(!choice.label().is_empty());
        assert_eq!(choice.metrics().glyph().to_pixels(px(16.)), px(glyph));
        assert_eq!(choice.metrics().target().to_pixels(px(16.)), px(target));
        assert_eq!(choice.metrics().gap().to_pixels(px(16.)), px(gap));
        assert_eq!(choice.metrics().row_height().to_pixels(px(16.)), px(row));
    }
}

struct States {
    calls: usize,
    busy: bool,
}

impl Render for States {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_4()
            .child(
                IconButton::new("labelled")
                    .icon(IconName::Search)
                    .label("Search")
                    .dropdown_caret(true)
                    .xsmall(),
            )
            .child(
                IconButton::new("compact")
                    .icon(IconName::X)
                    .accessibility_label("Remove")
                    .compact(),
            )
            .child(
                crate::action::ActionState::Success
                    .apply(
                        IconButton::new("state")
                            .icon(IconName::Search)
                            .label("Done"),
                    )
                    .icon(IconName::ArrowUp)
                    .selected(true)
                    .toggled(true)
                    .loading(self.busy)
                    .on_click(cx.listener(|this, _, _, _| this.calls += 1)),
            )
            .child(
                IconButton::from_button(Button::new("adapted").ghost())
                    .icon(IconName::X)
                    .label("Close"),
            )
    }
}

#[gpui_kit::test]
fn compact_labels_and_loading_keep_their_contract_at_every_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1400.), px(300.)), |_, _| States {
        calls: 0,
        busy: true,
    });
    let mut normal_text_height = None;
    for choice in IconButtonSize::ALL {
        cx.update(|cx| choice.apply(cx));
        cx.update_window(*handle, |_, window, cx| {
            window.render_frame(cx);
            let label = window.within("labelled").find("icon-button-label").bounds();
            let expected = *normal_text_height.get_or_insert(label.size.height);
            assert_eq!(label.size.height, expected, "icon size must not zoom text");
            assert_eq!(window.find("labelled").label(), Some("Search"));
            assert_eq!(window.find("compact").bounds().size, size(px(20.), px(20.)));
            assert_eq!(
                window.within("compact").find("toolbar-icon").bounds().size,
                size(px(12.), px(12.))
            );
            assert_eq!(window.find("state").checked(), Some(true));
            let glyph = window.within("state").find("toolbar-icon").bounds();
            assert_eq!(
                glyph.size.height,
                choice.metrics().glyph().to_pixels(px(16.))
            );
            assert_eq!(
                window.find("adapted").bounds().size.height,
                choice.metrics().target().to_pixels(px(16.))
            );
            window.click("state", cx);
        })
        .unwrap();
    }
    assert_eq!(handle.read_with(cx, |this, _| this.calls).unwrap(), 0);
    handle
        .update(cx, |this, _, cx| {
            this.busy = false;
            cx.notify();
        })
        .unwrap();
    cx.update_window(*handle, |_, window, cx| window.click("state", cx))
        .unwrap();
    assert_eq!(handle.read_with(cx, |this, _| this.calls).unwrap(), 1);
}

#[gpui_kit::test]
fn glyphs_and_targets_follow_rem_without_changing_the_preference(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1000.), px(300.)), |_, _| Fixture);
    cx.update(|cx| IconButtonSize::Large.apply(cx));
    cx.update_window(*handle, |_, window, cx| {
        window.set_rem_size(px(20.));
        window.render_frame(cx);
        assert_eq!(window.find("plain").bounds().size, size(px(60.), px(60.)));
        assert_eq!(
            window.within("plain").find("toolbar-icon").bounds().size,
            size(px(35.), px(35.))
        );
        assert_eq!(IconButtonSize::current(cx), IconButtonSize::Large);
    })
    .unwrap();
}

struct FocusFixture {
    focus: FocusHandle,
    calls: usize,
}

impl Render for FocusFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("focus-action")
            .icon(IconName::Search)
            .label("Search")
            .track_focus(&self.focus)
            .on_click(cx.listener(|this, _, _, _| this.calls += 1))
            .anchored_context_menu(|menu, _, _| {
                menu.item(gpui_kit::component::menu::PopupMenuItem::new("Show label"))
            })
    }
}

#[gpui_kit::test]
fn external_focus_and_context_menu_keep_native_keyboard_activation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(500.), px(300.)), |_, cx| FocusFixture {
        focus: cx.focus_handle(),
        calls: 0,
    });
    handle
        .update(cx, |this, window, cx| {
            this.focus.focus(window, cx);
        })
        .unwrap();
    cx.update_window(*handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("focus-action").focused(), Some(true));
        window.press("enter", cx);
        window.right_click("focus-action", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(*handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_some());
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(handle.read_with(cx, |this, _| this.calls).unwrap(), 1);
}

#[gpui_kit::test]
fn global_size_changes_real_glyph_and_hit_target(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(1000.), px(300.)), |_, _| Fixture);
    for (choice, glyph, target) in [
        (IconButtonSize::Medium, 20., 36.),
        (IconButtonSize::Small, 14., 24.),
        (IconButtonSize::Large, 28., 48.),
        (IconButtonSize::ExtraLarge, 40., 72.),
    ] {
        cx.update(|cx| choice.apply(cx));
        cx.update_window(*handle, |_, window, cx| {
            window.render_frame(cx);
            let frame = window.find("plain").bounds();
            assert_eq!(frame.size, size(px(target), px(target)));
            let icon = window.within("plain").find("toolbar-icon").bounds();
            assert_eq!(icon.size, size(px(glyph), px(glyph)));
            assert_eq!(icon.center(), frame.center());
        })
        .unwrap();
    }
}
