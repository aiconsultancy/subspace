use super::{Harness, settle};
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Entity, InteractiveElement as _, ParentElement as _, Styled as _,
    TestAppContext, VisualTestContext, div, px,
};

fn unit(key: &'static str, width: f32, checked: bool) -> OverflowUnit {
    OverflowUnit::inline(
        key,
        1.,
        move |_, _| div().w(px(width)).h(px(24.)),
        vec![PopupEntry::new(move || {
            PopupMenuItem::element(|_, _| div().id("choice-label").test_support().child("Choice"))
                .checked(checked)
        })],
    )
}

fn mount_toolbar(
    units: Vec<OverflowUnit>,
    cx: &mut TestAppContext,
) -> (Entity<OverflowToolbar>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let toolbar = cx.new(|_| OverflowToolbar::new(units));
    let observed = toolbar.clone();
    let (_, cx) = cx.add_window_view(|_, _| Harness {
        toolbar,
        width: 60.,
        left: 0.,
    });
    settle(cx);
    (observed, cx)
}

#[gpui_kit::test]
fn hidden_natural_width_and_membership_refresh_without_recreating_toolbar(cx: &mut TestAppContext) {
    let (observed, cx) = mount_toolbar(
        vec![unit("first", 32., false), unit("second", 32., false)],
        cx,
    );
    cx.update(|window, cx| {
        let row = window.find("overflow-toolbar-row").bounds();
        let trigger = window.find("overflow-toolbar-more").bounds();
        assert!(
            window.try_find("overflow-unit:first").is_none(),
            "32 + 4 + 28 cannot fit 60"
        );
        assert_eq!(trigger.size.width, px(28.));
        assert_eq!(
            window.find("overflow-toolbar-button").bounds().size.width,
            px(28.)
        );
        assert!(trigger.right() <= row.right());
        assert_eq!(observed.read(cx).content_width(), 68.);
    });
    observed.update(cx, |toolbar, cx| {
        toolbar.set_units(
            vec![unit("first", 12., false), unit("second", 16., false)],
            cx,
        )
    });
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(observed.read(cx).content_width(), 32.);
        assert!(window.try_find("overflow-toolbar-more").is_none());
        assert!(window.find("overflow-unit:second").visible());
    });
    observed.update(cx, |toolbar, cx| {
        toolbar.set_units(vec![unit("replacement", 100., false)], cx)
    });
    settle(cx);
    cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        assert!(window.find(0usize).visible());
        assert!(window.try_find(1usize).is_none());
        assert!(window.try_find("overflow-unit:first").is_none());
        assert!(window.try_find("overflow-unit:second").is_none());
    });
}

#[gpui_kit::test]
fn native_checked_indicator_updates_in_the_open_popup(cx: &mut TestAppContext) {
    let (observed, cx) = mount_toolbar(vec![unit("choice", 100., false)], cx);
    let unchecked = cx.update(|window, cx| {
        window.click("overflow-toolbar-button", cx);
        window.find("choice-label").bounds().left() - window.find(0usize).bounds().left()
    });
    observed.update(cx, |toolbar, cx| {
        toolbar.set_units(vec![unit("choice", 100., true)], cx)
    });
    settle(cx);
    let checked = cx.update(|window, _| {
        window.find("choice-label").bounds().left() - window.find(0usize).bounds().left()
    });
    assert!(
        checked > unchecked,
        "native checked icon must occupy space before the label"
    );
    observed.update(cx, |toolbar, cx| {
        toolbar.set_units(vec![unit("choice", 100., false)], cx)
    });
    settle(cx);
    cx.update(|window, _| {
        let restored =
            window.find("choice-label").bounds().left() - window.find(0usize).bounds().left();
        assert_eq!(
            restored, unchecked,
            "clearing checked state removes the native icon"
        );
    });
}
