use super::{Menu, MenuAnchors, MenuContents, MenuEvent, MenuOption};
use crate::anchored::Anchor;
use crate::overflow_toolbar::{OverflowToolbar, OverflowUnit, PopupEntry};
use gpui_kit::component::{button::Button, menu::PopupMenuItem};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render,
    Styled as _, Subscription, TestAppContext, VisualTestContext, Window, actions, div, point, px,
};

actions!(menu_owner_test, [OpenPicker]);

struct Owner {
    anchors: MenuAnchors,
    menu: Option<Entity<Menu>>,
    subscription: Option<Subscription>,
    picked: Option<String>,
    picks: usize,
    overflow: bool,
    toolbar: Option<Entity<OverflowToolbar>>,
}

impl Owner {
    fn open(
        &mut self,
        source: Option<Entity<Anchor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = source.unwrap_or_else(|| self.anchors.anchor("picker", cx));
        let contents = MenuContents::new(
            "Choices",
            "Find choice",
            vec![
                MenuOption::new("alpha", "Alpha"),
                MenuOption::new("beta", "Beta"),
            ],
        );
        let Some(menu) = self.anchors.open(anchor, contents, window, cx) else {
            return;
        };
        self.subscription = Some(cx.subscribe(&menu, |owner, _, event, cx| {
            owner.menu = None;
            if let MenuEvent::Pick(value) = event {
                owner.picked = Some(value.to_string());
                owner.picks += 1;
            }
            cx.notify();
        }));
        self.menu = Some(menu);
        cx.notify();
    }

    fn picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.anchors.trigger(
            "picker",
            Button::new("picker-button")
                .label("Choices")
                .on_click(cx.listener(|owner, _, window, cx| owner.open(None, window, cx))),
            cx,
        )
    }

    fn toolbar(&mut self, cx: &mut Context<Self>) -> Entity<OverflowToolbar> {
        let inline = cx.weak_entity();
        let popup = inline.clone();
        let unit = OverflowUnit::inline(
            "picker",
            100.,
            move |_, cx| {
                inline
                    .update(cx, |owner, cx| owner.picker(cx).into_any_element())
                    .unwrap()
            },
            vec![PopupEntry::anchored(move |anchor| {
                let owner = popup.clone();
                PopupMenuItem::new("Choose…").on_click(move |_, window, cx| {
                    _ = owner.update(cx, |owner, cx| owner.open(Some(anchor.host()), window, cx));
                })
            })],
        );
        let toolbar = OverflowToolbar::sync(&mut self.toolbar, vec![unit], cx);
        self.anchors.set_toolbar(&toolbar);
        toolbar
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let control = if self.overflow {
            div().w(px(40.)).child(self.toolbar(cx)).into_any_element()
        } else {
            self.picker(cx).into_any_element()
        };
        div()
            .key_context("MenuOwner")
            .on_action(
                cx.listener(|owner, _: &OpenPicker, window, cx| owner.open(None, window, cx)),
            )
            .child(div().absolute().left(px(50.)).top(px(60.)).child(control))
            .children(self.anchors.view())
    }
}

fn setup(cx: &mut TestAppContext, overflow: bool) -> (Entity<Owner>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::bind_keys(cx);
        cx.bind_keys([KeyBinding::new("ctrl-f", OpenPicker, Some("MenuOwner"))]);
    });
    let (owner, cx) = cx.add_window_view(|_, _| Owner {
        anchors: MenuAnchors::default(),
        menu: None,
        subscription: None,
        picked: None,
        picks: 0,
        overflow,
        toolbar: None,
    });
    settle(cx);
    (owner, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn searchable_body_inline_toggle_escape_and_owner_option_cleanup(cx: &mut TestAppContext) {
    let (owner, cx) = setup(cx, false);
    cx.update(|window, cx| window.click("picker-button", cx));
    settle(cx);
    cx.update(|window, _| {
        let trigger = window.find("picker-button").bounds();
        assert_eq!(window.find("menu").bounds().top(), trigger.bottom());
    });
    cx.update(|window, cx| window.click("picker-button", cx));
    settle(cx);
    owner.read_with(cx, |owner, _| assert!(owner.menu.is_none()));
    cx.update(|window, cx| window.click("picker-button", cx));
    cx.simulate_keystrokes("escape");
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("menu").is_none());
        assert_eq!(window.find("picker-button").focused(), Some(true));
    });
}

#[gpui_kit::test]
fn searchable_body_keyboard_filters_and_commits_once(cx: &mut TestAppContext) {
    let (owner, cx) = setup(cx, false);
    cx.update(|window, cx| {
        window.focus_next(cx);
        window.press("ctrl-f", cx);
    });
    settle(cx);
    cx.simulate_keystrokes("b e t a enter");
    settle(cx);
    owner.read_with(cx, |owner, _| {
        assert_eq!(owner.picked.as_deref(), Some("beta"));
        assert_eq!(owner.picks, 1);
        assert!(owner.menu.is_none());
    });
    cx.update(|window, _| assert_eq!(window.find("picker-button").focused(), Some(true)));
}

#[gpui_kit::test]
fn searchable_overflow_replaces_native_body_and_returns_to_chevron(cx: &mut TestAppContext) {
    let (owner, cx) = setup(cx, true);
    cx.update(|window, cx| window.click("overflow-toolbar-button", cx));
    settle(cx);
    cx.update(|window, cx| {
        assert_eq!(window.find(0usize).label(), Some("Choose…"));
        window.press("down", cx);
        window.press("enter", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("popup-menu").is_none());
        assert!(window.try_find("menu").is_some());
        let trigger = window.find("overflow-toolbar-button").bounds();
        assert_eq!(window.find("menu").bounds().top(), trigger.bottom());
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    owner.read_with(cx, |owner, _| assert!(owner.menu.is_none()));
    cx.update(|window, _| assert_eq!(window.find("overflow-toolbar-button").focused(), Some(true)));
}

#[gpui_kit::test]
fn hidden_picker_keyboard_uses_chevron_and_outside_or_owner_hide_closes_host(
    cx: &mut TestAppContext,
) {
    let (owner, cx) = setup(cx, true);
    cx.update(|window, cx| {
        window.focus_next(cx);
        window.press("ctrl-f", cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("picker-button").is_none());
        assert_eq!(
            window.find("menu").bounds().top(),
            window.find("overflow-toolbar-button").bounds().bottom()
        );
    });
    cx.simulate_click(point(px(10.), px(10.)), Default::default());
    settle(cx);
    owner.read_with(cx, |owner, _| assert!(owner.menu.is_none()));
    cx.update(|window, cx| window.press("ctrl-f", cx));
    settle(cx);
    owner.update_in(cx, |owner, window, cx| owner.anchors.dismiss(window, cx));
    settle(cx);
    cx.update(|window, _| assert!(window.try_find("menu").is_none()));
}
