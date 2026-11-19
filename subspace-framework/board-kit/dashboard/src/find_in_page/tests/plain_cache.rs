//! Plain text revisions invalidate the native paint cache even after an A->B->A edit.
use crate::find_in_page::{FindInPage, participant::Native};
use crate::rich::plain::PlainState;
use gpui_kit::test::TestWindowExt as _;
#[cfg(feature = "gpui-kit-text-patch")]
use gpui_kit::{
    App, Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

fn plain_state(pane: &Entity<FindInPage>, cx: &VisualTestContext) -> Entity<PlainState> {
    pane.read_with(cx, |p, cx| {
        p.session
            .read(cx)
            .entries
            .values()
            .find_map(|entry| match &entry.native {
                Native::Plain(state) => state.upgrade(),
                _ => None,
            })
            .expect("plain text participant")
    })
}

#[cfg(feature = "gpui-kit-text-patch")]
struct ScrollContent {
    focus: FocusHandle,
    scroll: ScrollHandle,
}

#[cfg(feature = "gpui-kit-text-patch")]
impl gpui_kit::Focusable for ScrollContent {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(feature = "gpui-kit-text-patch")]
impl Render for ScrollContent {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .child(gpui_kit::base::TextSelectionLayer)
            .child(crate::find_in_page::scroll(
                self.scroll.clone(),
                div()
                    .id("find-plain-scroll")
                    .h(px(120.))
                    .w(px(400.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(div().h(px(240.)))
                    .child(crate::rich::plain_text("find-title", "Needle title")),
            ))
    }
}

#[cfg(feature = "gpui-kit-text-patch")]
#[gpui_kit::test]
fn a_plain_find_match_remains_highlighted_and_selectable_after_reveal(cx: &mut TestAppContext) {
    use gpui_kit::base::TextSelection;

    let scroll = ScrollHandle::new();
    let (pane, cx) = crate::find_in_page::test_support::mount_with(cx, |cx| ScrollContent {
        focus: cx.focus_handle(),
        scroll: scroll.clone(),
    });
    assert_eq!(scroll.offset().y, px(0.));
    cx.update(|window, cx| window.press(crate::secondary!("f"), cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.input("needle", cx));
    cx.run_until_parked();
    let plain = plain_state(&pane, cx);
    cx.update(|window, cx| window.render_frame(cx));
    assert!(
        scroll.offset().y < px(0.),
        "find should reveal the off-screen match"
    );
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.click_with_options(
            "plain-menu-Name(\"find-title\")",
            gpui_kit::test::ClickOptions::new()
                .with_offset(gpui_kit::point(gpui_kit::px(5.), gpui_kit::px(8.)))
                .with_count(2),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(TextSelection::selected_text(window, cx), "Needle");
    });
    assert!(plain.read_with(cx, |state, _| state.highlight_count()) > 0);
}

#[gpui_kit::test]
fn plain_text_changing_back_before_rebuild_still_reapplies_highlights(cx: &mut TestAppContext) {
    let (pane, cx) = crate::find_in_page::test_support::mount(cx);
    cx.update(|window, cx| window.press(crate::secondary!("f"), cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.input("needle", cx));
    cx.run_until_parked();
    let plain = plain_state(&pane, cx);
    assert!(plain.read_with(cx, |s, _| s.highlight_count()) > 0);
    plain.update(cx, |s, cx| {
        let original = s.text.clone();
        s.set_text("other".into(), cx);
        s.set_text(original, cx);
        assert_eq!(s.highlight_count(), 0);
    });
    cx.run_until_parked();
    assert!(plain.read_with(cx, |s, _| s.highlight_count()) > 0);
}
