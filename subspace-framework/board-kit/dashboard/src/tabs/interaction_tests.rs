//! Exercise native selection and close callbacks through the shared adapter.

use super::{TabSpec, tab, tab_bar};
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    TestAppContext, Window, div,
};

struct Fixture {
    focus: FocusHandle,
    selected: usize,
    closed: Option<usize>,
    size: Size,
    closable: bool,
    disabled: bool,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = ["General", "Operations"]
            .into_iter()
            .enumerate()
            .map(|(ix, name)| {
                tab(TabSpec::plain(name)
                    .unread(ix == 1 && self.closable)
                    .disabled(self.disabled))
                .when(self.closable, |tab| {
                    tab.on_close(cx.listener(move |view, _, _, cx| {
                        view.closed = Some(ix);
                        cx.notify();
                    }))
                })
            });
        div().track_focus(&self.focus).child(
            tab_bar("test-resource-tabs")
                .with_size(self.size)
                .selected_index(self.selected)
                .children(tabs)
                .on_click(cx.listener(|view, ix, _, cx| {
                    view.selected = *ix;
                    cx.notify();
                })),
        )
    }
}

fn mount(cx: &mut TestAppContext) -> (gpui_kit::Entity<Fixture>, &mut gpui_kit::VisualTestContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle().tab_stop(false);
        focus.focus(window, cx);
        Fixture {
            focus,
            selected: 0,
            closed: None,
            size: Size::Small,
            closable: true,
            disabled: false,
        }
    });
    cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(view.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    (view, cx)
}

#[gpui_kit::test]
fn native_selection_and_unread_accessibility_survive_the_shared_treatment(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("test-resource-tabs");
        assert_eq!(bar.find(0usize).selected(), Some(true));
        assert_eq!(bar.find(1usize).label(), Some("Operations, unread items"));
        bar.click(1usize, cx);
    });
    assert_eq!(view.read_with(cx, |view, _| view.selected), 1);
}

#[gpui_kit::test]
fn hovering_and_closing_an_inactive_tab_does_not_select_it(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.within("test-resource-tabs").hover(1usize, cx);
        window
            .within("test-resource-tabs")
            .within(1usize)
            .click("close-tab", cx);
    });
    assert_eq!(
        view.read_with(cx, |view, _| (view.selected, view.closed)),
        (0, Some(1))
    );
}

#[gpui_kit::test]
fn an_inactive_close_button_can_receive_keyboard_focus_and_activate(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.press("tab", cx);
        window.press("tab", cx);
        assert_eq!(
            window
                .within("test-resource-tabs")
                .within(1usize)
                .find("close-tab")
                .focused(),
            Some(true)
        );
        assert!(
            window
                .within("test-resource-tabs")
                .within(1usize)
                .find("close-tab")
                .visible()
        );
        window.press("space", cx);
    });
    assert_eq!(view.read_with(cx, |view, _| view.closed), Some(1));
}

#[gpui_kit::test]
fn rendered_close_buttons_are_transparent_at_rest_and_visible_on_selection_or_hover(
    cx: &mut TestAppContext,
) {
    let (_, cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("test-resource-tabs");
        assert!(bar.within(0usize).find("close-tab").visible());
        assert!(!bar.within(1usize).find("close-tab").visible());
        bar.hover(1usize, cx);
        assert!(bar.within(1usize).find("close-tab").visible());
    });
}

#[gpui_kit::test]
fn small_and_default_layers_follow_the_tab_height_and_labels_have_equal_padding(
    cx: &mut TestAppContext,
) {
    let (view, cx) = mount(cx);
    for (size, height) in [(Size::Small, 24.), (Size::Medium, 32.)] {
        view.update(cx, |view, cx| {
            view.size = size;
            view.closable = false;
            cx.notify();
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            let mut bar = window.within("test-resource-tabs");
            let tab = bar.find(0usize).bounds();
            let selected = bar.within(0usize);
            let layers = selected.find("tab-layers").bounds();
            let content = selected.find("tab-content-row").bounds();
            assert_eq!(tab.size.height, gpui_kit::px(height));
            assert_eq!(
                (layers.top(), layers.size.height),
                (tab.top(), tab.size.height)
            );
            assert_eq!(content.left() - tab.left(), tab.right() - content.right());
        });
    }
}

#[gpui_kit::test]
fn disabled_tabs_have_no_accent_and_cannot_change_selection(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx);
    view.update(cx, |view, cx| {
        view.disabled = true;
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("test-resource-tabs");
        for ix in 0..2 {
            assert!(bar.within(ix).try_find("tab-selected-edge").is_none());
        }
        bar.click(1usize, cx);
    });
    assert_eq!(view.read_with(cx, |view, _| view.selected), 0);
}

struct SuffixFixture;
impl Render for SuffixFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::{Styled as _, TestSupportExt as _};
        gpui_kit::component::v_flex().children([false, true].map(|suffix_first| {
            let bar = tab_bar(if suffix_first {
                "suffix-first"
            } else {
                "suffix-last"
            })
            .child(tab(TabSpec::plain("Tab")));
            let suffix = || div().id("suffix").test_support().size_6();
            let space = || div().id("last-empty").test_support().size_6();
            if suffix_first {
                bar.suffix(suffix()).last_empty_space(space())
            } else {
                bar.last_empty_space(space()).suffix(suffix())
            }
        }))
    }
}

#[gpui_kit::test]
fn last_empty_space_preserves_an_explicit_suffix_in_either_builder_order(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| SuffixFixture);
    cx.update(|window, cx| {
        window.render_frame(cx);
        for id in ["suffix-first", "suffix-last"] {
            let bar = window.within(id);
            assert!(bar.find("suffix").visible());
            assert!(bar.find("last-empty").visible());
        }
    });
}

#[gpui_kit::test]
fn the_glass_hover_rim_is_sized_to_the_tab(cx: &mut TestAppContext) {
    let (_, cx) = mount(cx);
    cx.update(|window, cx| {
        cx.set_global(crate::density::Density::from_json(
            r#"{"--toolbar-button-hover":"glass"}"#,
        ));
        window.refresh();
        window.render_frame(cx);
        let mut bar = window.within("test-resource-tabs");
        bar.hover(1usize, cx);
        let bounds = bar.find(1usize).bounds();
        let rim = crate::Tokens::current(cx).glass_rim;
        assert!(window.painted_quads().iter().any(|quad| {
            quad.border_color == rim
                && quad.bounds.size.width <= bounds.size.width.scale(window.scale_factor())
                && quad.bounds.size.width
                    >= (bounds.size.width - gpui_kit::px(2.)).scale(window.scale_factor())
                && quad.border_widths.left > gpui_kit::px(0.).scale(window.scale_factor())
                && quad.border_widths.right > gpui_kit::px(0.).scale(window.scale_factor())
                && quad.border_widths.top > gpui_kit::px(0.).scale(window.scale_factor())
                && quad.border_widths.bottom > gpui_kit::px(0.).scale(window.scale_factor())
        }));
    });
}

#[gpui_kit::test]
fn the_selected_tab_paints_its_fill_and_accent_edge_across_its_own_bounds(cx: &mut TestAppContext) {
    let (_, cx) = mount(cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("test-resource-tabs");
        let tab = bar.find(0usize).bounds();
        let selected = bar.within(0usize);
        let layers = selected.find("tab-layers").bounds();
        let edge = selected.find("tab-selected-edge").bounds();
        assert_eq!(
            (layers.top(), layers.size.height),
            (tab.top(), tab.size.height)
        );
        assert_eq!(
            (edge.top(), edge.size.height),
            (tab.top(), gpui_kit::px(2.))
        );
        assert!(edge.left() <= tab.left() + gpui_kit::px(1.) && edge.right() >= tab.right());
        assert!(bar.within(1usize).try_find("tab-selected-edge").is_none());
    });
}
