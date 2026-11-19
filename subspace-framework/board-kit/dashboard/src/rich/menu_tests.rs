//! Exercise the real native text/menu event path, including selected links.
use super::{Format, RichTextContext, rich_text};
use gpui_kit::base::{TextSelection, TextSelectionLayer};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    ClipboardItem, Context, IntoElement, MouseButton, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div, point, px,
};
use std::sync::{Arc, Mutex};

struct Fixture {
    context: Arc<Mutex<Option<RichTextContext>>>,
}
impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let context = self.context.clone();
        div().size_full().child(TextSelectionLayer).child(
            div().w(px(500.)).child(
                rich_text(
                    "menu-link",
                    Format::Markdown,
                    "[alpha **bravo**](https://example.org/path) charlie",
                )
                .native(move |view, incoming| {
                    *context.lock().unwrap() = Some(incoming);
                    view
                }),
            ),
        )
    }
}

#[gpui_kit::test]
fn selected_formatted_link_keeps_selection_on_a_secondary_click(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (fixture, cx) = cx.add_window_view(|_, _| Fixture {
        context: Arc::default(),
    });
    cx.run_until_parked();
    let (selected, link_position) = cx.update(|window, cx| {
        window.render_frame(cx);
        let state = fixture
            .read(cx)
            .context
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .state
            .upgrade()
            .unwrap();
        state.update(cx, |state, cx| state.select_all(cx));
        window.render_frame(cx);
        let selected = TextSelection::selected_text(window, cx);
        assert!(selected.contains("alpha bravo"));
        cx.write_to_clipboard(ClipboardItem::new_string("untouched".into()));
        let bounds = window.find("rich-menu-Name(\"menu-link\")").bounds();
        (selected, bounds.origin + point(px(35.), px(18.)))
    });
    cx.simulate_mouse_down(link_position, MouseButton::Right, Default::default());
    cx.run_until_parked();
    // A real window can redraw while the button is still held.
    cx.update(|window, cx| window.render_frame(cx));
    cx.simulate_mouse_up(link_position, MouseButton::Right, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(TextSelection::selected_text(window, cx), selected);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("untouched")
        );
        #[cfg(feature = "gpui-kit-text-patch")]
        {
            assert_eq!(
                window.within("popup-menu").find(1usize).label(),
                Some("Copy link")
            );
            window.within("popup-menu").click(1usize, cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("https://example.org/path")
            );
        }
    });
    #[cfg(feature = "gpui-kit-text-patch")]
    check_menu_away_from_link(link_position + point(px(150.), px(0.)), cx);
}

#[cfg(feature = "gpui-kit-text-patch")]
fn check_menu_away_from_link(
    position: gpui_kit::Point<gpui_kit::Pixels>,
    cx: &mut gpui_kit::VisualTestContext,
) {
    cx.simulate_mouse_down(position, MouseButton::Right, Default::default());
    cx.simulate_mouse_up(position, MouseButton::Right, Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.within("popup-menu").find(1usize).label(),
            Some("Select all")
        );
    });
}
