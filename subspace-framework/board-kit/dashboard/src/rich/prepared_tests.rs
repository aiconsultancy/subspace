use gpui_kit::base::{TextSelection, TextSelectionLayer};
use gpui_kit::{
    Context, IntoElement, Modifiers, MouseButton, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, Window, div, point, px,
};
#[derive(Clone, Copy)]
enum RichPath {
    Prepared,
    Markdown,
    WithLinks,
    Html,
}

struct PreparedFixture(SharedString, RichPath);

impl Render for PreparedFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let view = match self.1 {
            RichPath::Prepared => {
                super::rich_text_prepared("prepared", self.0.clone(), |_, _, _| true)
            }
            RichPath::Markdown => super::rich_text("prepared", super::Format::Markdown, &self.0),
            RichPath::WithLinks => super::rich_text_with_links(
                "prepared",
                super::Format::Markdown,
                &self.0,
                |_, _, _| true,
            ),
            RichPath::Html => super::rich_text("prepared", super::Format::Html, &self.0),
        };
        div()
            .size_full()
            .child(TextSelectionLayer)
            .child(div().w(px(640.)).child(view))
    }
}

#[gpui_kit::test]
fn rich_text_prepared_keeps_all_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let prepared = "costs \\$5 and \\$10; `<form>$$</form>`; *Image: shot.png*; @Name; TST-827";
    assert_visible(
        prepared,
        "costs $5 and $10; <form>$$</form>; Image: shot.png; @Name; TST-827",
        RichPath::Prepared,
        cx,
    );
}

#[gpui_kit::test]
fn every_rich_markdown_path_contains_parser_panics(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for path in [RichPath::Prepared, RichPath::Markdown, RichPath::WithLinks] {
        for (source, visible) in [
            ("1. ```\n- x\n", "1. ``` - x"),
            ("1. a\n   ```\n- x\n", "1. a ``` - x"),
            ("1. a\n   ```\n- x\n```", "1. a ``` - x ```"),
        ] {
            assert_visible(source, visible, path, cx);
        }
    }
}

#[gpui_kit::test]
fn html_generated_list_fences_use_the_same_guard(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let html = "<ol><li><pre>```\n~~~</pre></li></ol><ul><li>x</li></ul>";
    assert_eq!(
        crate::html::to_markdown(html),
        "1. ~~~\n   ```\n   ~~~\n   ~~~\n- x"
    );
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    assert!(markdown::to_mdast(&crate::html::to_markdown(html), &options).is_err());
    assert_visible(html, "1. ~~~ ``` ~~~ ~~~ - x", RichPath::Html, cx);
}

fn assert_visible(
    prepared: impl Into<SharedString>,
    visible: &str,
    path: RichPath,
    cx: &mut TestAppContext,
) {
    let prepared = prepared.into();
    let (_, cx) = cx.add_window_view(|_, _| PreparedFixture(prepared.clone(), path));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_mouse_down(
        point(px(40.), px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(40.), px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|window, cx| window.dispatch_action(Box::new(gpui_kit::base::input::SelectAll), cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        assert_eq!(
            TextSelection::selected_text(window, cx).trim(),
            visible,
            "{prepared}"
        );
    });
}

#[test]
fn current_markdown_owners_use_the_guarded_constructor() {
    let constructor = include_str!("native_state.rs");
    assert_eq!(constructor.matches("TextViewState::markdown(").count(), 1);
    for source in [
        include_str!("view.rs"),
        include_str!("../find_in_page/state/refresh.rs"),
    ] {
        assert!(!source.contains("TextViewState::markdown("));
        assert!(source.contains("native_state::create("));
    }
}
