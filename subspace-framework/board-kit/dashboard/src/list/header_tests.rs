use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Role,
    SharedString, Styled as _, TestAppContext, TestSupportExt as _, Window, px,
};

use super::header;

const INSET: Pixels = px(12.);
const SCOPE: &str = "Example Forge · Signed in as example-reviewer";

struct HeadingFixture {
    title: SharedString,
    width: Pixels,
}

impl Render for HeadingFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("heading-fixture")
            .test_support()
            .w(self.width)
            .child(header("fixture", self.title.clone(), SCOPE, INSET, cx))
    }
}

#[gpui_kit::test]
fn subtitle_follows_the_title_width_and_shrinks_within_the_header(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (fixture, cx) = cx.add_window_view(|_, _| HeadingFixture {
        title: "Issues".into(),
        width: px(600.),
    });
    let short_left = cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("fixture-scope").bounds().left()
    });
    fixture.update(cx, |fixture, cx| {
        fixture.title = "Pull requests".into();
        cx.notify();
    });
    let wide_scope = cx.update(|window, cx| {
        window.render_frame(cx);
        let title = window.find("fixture-heading");
        let scope = window.find("fixture-scope").bounds();
        assert_eq!(title.role(), Some(Role::Heading));
        assert_eq!(title.label(), Some("Pull requests"));
        assert!(scope.left() > short_left);
        assert!(scope.left() >= title.bounds().right());
        assert!(scope.top() < title.bounds().bottom());
        assert!(title.bounds().top() < scope.bottom());
        scope
    });
    fixture.update(cx, |fixture, cx| {
        fixture.width = px(240.);
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bounds = window.find("heading-fixture").bounds();
        let scope = window.find("fixture-scope");
        assert_eq!(scope.label(), Some(SCOPE));
        assert_eq!(scope.bounds().left(), wide_scope.left());
        assert_eq!(scope.bounds().size.height, wide_scope.size.height);
        assert!(scope.bounds().right() < wide_scope.right());
        assert!(scope.bounds().right() <= bounds.right() - INSET);
    });
}
