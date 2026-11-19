use super::{PanelBody, render};
use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollDelta,
    Styled as _, TestAppContext, TestSupportExt as _, Window, div, point, px,
};
struct Fixture;
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = v_flex().children((0..50).map(|index| {
            div()
                .id(format!("body-row-{index}"))
                .test_support()
                .h(px(24.))
                .flex_shrink_0()
                .child(format!("Fixture line {index}"))
        }));
        v_flex()
            .w(px(300.))
            .h(px(150.))
            .child(render(PanelBody::from(rows)))
    }
}
#[gpui_kit::test]
fn fallback_scroll_position_survives_a_panel_rerender(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (fixture, cx) = cx.add_window_view(|_, _| Fixture);
    let before = cx.update(|window, cx| {
        window.render_frame(cx);
        let start = window.find("body-row-0").bounds().top();
        window.scroll(
            "detail-body",
            ScrollDelta::Pixels(point(px(0.), px(-180.))),
            cx,
        );
        let after = window.find("body-row-0").bounds().top();
        assert!(after < start);
        after
    });
    fixture.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("body-row-0").bounds().top(), before);
    });
}
