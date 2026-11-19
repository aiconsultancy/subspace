use super::{IconButton, IconButtonSize};
use gpui_kit::component::{
    WindowExt as _,
    button::Button,
    dialog::{DialogClose, DialogFooter},
    h_flex,
};
use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, TestAppContext, TestSupportExt as _, Window, WindowBounds, WindowOptions, div, px,
    size,
};
use std::{cell::Cell, rc::Rc, time::Duration};
use subspace_lucide::IconName;

struct Fixture(Rc<Cell<usize>>);

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex().children([false, true].map(|loading| {
            let closed = self.0.clone();
            Button::new(if loading { "open-busy" } else { "open-idle" })
                .label("Open dialog")
                .on_click(move |_, window, cx| {
                    let closed = closed.clone();
                    window.open_dialog(cx, move |dialog, _, _| {
                        let closed = closed.clone();
                        dialog
                            .title("Adapter regression")
                            .footer(DialogFooter::new().child(
                                div().id("adapted-footer").test_support().child(
                                    DialogClose::new().trigger(|button| {
                                        IconButton::from_button(button)
                                            .focus_on_click()
                                            .icon(IconName::X)
                                            .label("Close")
                                            .loading(loading)
                                    }),
                                ),
                            ))
                            .on_close(move |_, _, _| closed.set(closed.get() + 1))
                    });
                })
        }))
    }
}

fn assert_single_glyph_geometry(window: &mut Window, choice: IconButtonSize) {
    let mut footer = window.within("adapted-footer");
    let button = footer.find("close");
    let content = footer.within("close");
    let glyph = content.find("toolbar-icon").bounds();
    let label = content.find("icon-button-label").bounds();
    let gap = choice.metrics().gap().to_pixels(px(16.));
    // Default Button has two 1px borders. A retained native icon would add
    // another icon and gap, and fail this width and leading-inset measurement.
    let padding = gpui_kit::rems(choice.nominal().pad).to_pixels(px(16.));
    let expected_width = glyph.size.width + gap + label.size.width + padding * 2. + px(2.);
    assert_eq!(button.bounds().size.width, expected_width);
    assert_eq!(
        button.bounds().size.height,
        choice.metrics().target().to_pixels(px(16.))
    );
    assert_eq!(
        glyph.size.width,
        choice.metrics().glyph().to_pixels(px(16.))
    );
    assert_eq!(glyph.left() - button.bounds().left(), padding + px(1.));
    assert_eq!(button.label(), Some("Close"));
}

#[gpui_kit::test]
async fn real_dialog_close_adapter_has_one_glyph_and_preserves_cancel(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let closed = Rc::new(Cell::new(0));
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(900.), px(700.)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(|_| Fixture(closed.clone())),
        )
        .unwrap()
    });
    for (choice, loading, expected_closed) in [
        (IconButtonSize::Small, false, 1),
        (IconButtonSize::ExtraLarge, true, 2),
    ] {
        cx.update(|cx| choice.apply(cx));
        cx.update_window(handle, |_, window, cx| {
            window.click(if loading { "open-busy" } else { "open-idle" }, cx);
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(1), |window, _| {
            window.try_find("dialog").is_some()
        })
        .await;
        cx.update_window(handle, |_, window, cx| {
            assert_single_glyph_geometry(window, choice);
            window.within("adapted-footer").click("close", cx);
            if loading {
                assert!(window.try_find("dialog").is_some());
                window.press("escape", cx);
            }
        })
        .unwrap();
        cx.wait_for(handle, Duration::from_secs(1), |window, _| {
            window.try_find("dialog").is_none()
        })
        .await;
        assert_eq!(closed.get(), expected_closed);
    }
}
