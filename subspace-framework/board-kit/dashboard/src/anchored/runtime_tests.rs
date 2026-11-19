use super::{Anchor, dropdown_menu, trigger};
use gpui_kit::component::{button::Button, menu::PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Point, Render, Styled as _, TestAppContext,
    TestSupportExt as _, VisualTestContext, WeakEntity, Window, div, point, px,
};
use std::{cell::Cell, rc::Rc};

mod geometry;
mod interaction;
mod native;

struct Harness {
    anchor: Entity<Anchor>,
    position: Point<gpui_kit::Pixels>,
    host_origin: Point<gpui_kit::Pixels>,
    picked: Rc<Cell<usize>>,
    other_focus: FocusHandle,
    move_focus: bool,
    native: bool,
}

impl Harness {
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.anchor.read(cx).is_open() {
            self.anchor
                .update(cx, |anchor, cx| anchor.dismiss(window, cx));
            return;
        }
        self.show_body(window, cx);
    }

    fn show_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = cx.new(|cx| Body {
            anchor: self.anchor.downgrade(),
            focus: cx.focus_handle(),
            entry: cx.focus_handle(),
            picked: self.picked.clone(),
            next_focus: self.move_focus.then(|| self.other_focus.clone()),
        });
        self.anchor
            .update(cx, |anchor, cx| anchor.show(body, window, cx));
    }

    fn button(&self) -> crate::icon_button::IconButton {
        crate::action::action_button(
            "anchored-trigger",
            crate::action::Intent::Quiet,
            Some(subspace_lucide::IconName::ChevronDown),
            "Open choices",
        )
        .w(px(100.))
    }
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let button = if self.native {
            let picked = self.picked.clone();
            let owner = cx.weak_entity();
            dropdown_menu(self.button(), move |menu, _, _| {
                let picked = picked.clone();
                let owner = owner.clone();
                menu.item(PopupMenuItem::new("Choose item").on_click(move |_, _, _| {
                    picked.set(picked.get() + 1);
                }))
                .item(
                    PopupMenuItem::new("Open chooser").on_click(move |_, window, cx| {
                        _ = owner.update(cx, |root, cx| root.show_body(window, cx));
                    }),
                )
            })
            .with_anchor(&self.anchor)
            .into_any_element()
        } else {
            trigger(
                &self.anchor,
                self.button().on_click(cx.listener(Self::on_open)),
            )
            .into_any_element()
        };
        div()
            .size_full()
            .when(!self.native, |view| {
                view.child(
                    div()
                        .absolute()
                        .left(self.host_origin.x)
                        .top(self.host_origin.y)
                        .child(self.anchor.clone()),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(self.position.x)
                    .top(self.position.y)
                    .child(button),
            )
            .child(div().id("other-focus").track_focus(&self.other_focus))
    }
}

impl Harness {
    fn on_open(&mut self, _: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.open(window, cx);
    }
}

struct Body {
    anchor: WeakEntity<Anchor>,
    focus: FocusHandle,
    entry: FocusHandle,
    picked: Rc<Cell<usize>>,
    next_focus: Option<FocusHandle>,
}

impl Focusable for Body {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.entry.clone()
    }
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("anchored-body")
            .test_support()
            .track_focus(&self.focus)
            .w(px(200.))
            .h(px(120.))
            .child(div().id("entry-focus").track_focus(&self.entry))
            .child(
                Button::new("choose-item")
                    .label("Choose item")
                    .on_click(cx.listener(|body, _, window, cx| {
                        body.picked.set(body.picked.get() + 1);
                        if let Some(focus) = &body.next_focus {
                            focus.focus(window, cx);
                        }
                        _ = body
                            .anchor
                            .update(cx, |anchor, cx| anchor.dismiss(window, cx));
                    })),
            )
    }
}

fn setup(cx: &mut TestAppContext) -> (Entity<Harness>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (root, cx) = cx.add_window_view(|_, cx| Harness {
        anchor: cx.new(Anchor::new),
        position: point(px(40.), px(40.)),
        host_origin: point(px(0.), px(0.)),
        picked: Rc::default(),
        other_focus: cx.focus_handle(),
        move_focus: false,
        native: false,
    });
    settle(cx);
    (root, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

fn open(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click("anchored-trigger", cx));
    settle(cx);
}
