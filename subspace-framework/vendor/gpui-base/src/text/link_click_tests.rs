use std::sync::{Arc, Mutex};

use gpui::{
    AppContext as _, ClickEvent, Context, Entity, IntoElement, Modifiers, MouseButton,
    ParentElement as _, Pixels, Point, Render, SharedString, Styled as _, TestAppContext, Window,
    div, point, px,
};

use super::{TextView, TextViewState};
use crate::{TextSelection, TextSelectionLayer};

type Clicks = Arc<Mutex<Vec<(SharedString, MouseButton)>>>;

struct LinkView {
    state: Entity<TextViewState>,
    clicks: Clicks,
}

impl Render for LinkView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div().w(px(280.)).child(TextSelectionLayer).child(
            TextView::new(&self.state)
                .selectable(true)
                .on_link_click(move |url, event, _, _| {
                    if let ClickEvent::Mouse(click) = event {
                        clicks.lock().unwrap().push((url.clone(), click.up.button));
                    }
                }),
        )
    }
}

const URL: &str = "https://example.org/path";
const IMAGE: &str = concat!(
    "data:image/png;base64,",
    "iVBORw0KGgoAAAANSUhEUgAAACgAAAAYCAYAAACIhL/AAAAANUlEQVR4nO3OMREAIAwAsfpXVleggnuGDNkzs3u+lgcEBeuAoGAdEBSsA4KCdUBQsA4IPnYBtcGYyGMVk+kAAAAASUVORK5CYII=",
);

fn check_auxiliary_click(
    source: &str,
    at: Point<Pixels>,
    expected_text: Option<&str>,
    cx: &mut TestAppContext,
) {
    cx.update(crate::init);
    let clicks = Clicks::default();
    let (view, cx) = cx.add_window_view(|_, cx| LinkView {
        state: cx.new(|cx| TextViewState::markdown(source, cx)),
        clicks: clicks.clone(),
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        view.read(cx)
            .state
            .clone()
            .update(cx, |state, cx| state.select_all(cx));
        window.draw(cx).clear(cx);
    });
    let selected = cx.update(|window, cx| TextSelection::selected_text(window, cx));
    if let Some(text) = expected_text {
        assert!(selected.contains(text), "selected text: {selected:?}");
    }

    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    assert!(clicks.lock().unwrap().is_empty());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    assert_eq!(
        clicks.lock().unwrap().as_slice(),
        &[(URL.into(), MouseButton::Right)]
    );
    assert_eq!(
        cx.update(|window, cx| TextSelection::selected_text(window, cx)),
        selected
    );

    cx.update(|window, cx| {
        TextSelection::clear(window, cx);
        window.draw(cx).clear(cx);
    });
    for button in [MouseButton::Left, MouseButton::Middle] {
        cx.simulate_mouse_down(at, button, Modifiers::default());
        cx.simulate_mouse_up(at, button, Modifiers::default());
    }
    assert_eq!(
        clicks.lock().unwrap().as_slice(),
        &[
            (URL.into(), MouseButton::Right),
            (URL.into(), MouseButton::Left),
            (URL.into(), MouseButton::Middle),
        ]
    );
}

#[gpui::test]
fn a_formatted_link_reports_the_auxiliary_click_with_a_selection(cx: &mut TestAppContext) {
    check_auxiliary_click(
        &format!("[alpha **bravo**]({URL}) tail"),
        point(px(20.), px(12.)),
        Some("alpha bravo"),
        cx,
    );
}

#[gpui::test]
fn a_standalone_linked_image_reports_the_auxiliary_click(cx: &mut TestAppContext) {
    check_auxiliary_click(
        &format!("[![dot]({IMAGE})]({URL})"),
        point(px(12.), px(12.)),
        None,
        cx,
    );
}

#[gpui::test]
fn an_inline_linked_image_reports_the_auxiliary_click(cx: &mut TestAppContext) {
    check_auxiliary_click(
        &format!("head [![dot]({IMAGE})]({URL}) tail"),
        point(px(60.), px(12.)),
        Some("head"),
        cx,
    );
}
