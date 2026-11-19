//! Composer trigger presentation, Send or Stop, and the measured toolbar row.

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_dashboard::icon_button::{IconButton, Metrics};
use subspace_lucide::IconName;

use super::labels::effort_label;
use super::native_menu::NativeMenu;
use super::overflow::Control;
use super::{ChatToolbar, ToolbarEvent};
use crate::anchored;
use crate::tokens::Tokens;
use subspace_dashboard::overflow_toolbar::OverflowToolbar;

pub(super) fn button(id: &'static str, icon: IconName, label: &'static str) -> IconButton {
    IconButton::new(id)
        .ghost()
        .small()
        .icon(icon)
        .tooltip(label)
        .accessibility_label(label)
}

impl ChatToolbar {
    pub fn dismiss_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model_anchor
            .update(cx, |anchor, cx| anchor.dismiss(window, cx));
        self.usage_anchor
            .update(cx, |anchor, cx| anchor.dismiss(window, cx));
        self.anchors.dismiss(window, cx);
    }

    pub(super) fn menu_trigger(
        &self,
        control: Control,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let owner = cx.weak_entity();
        let (id, icon, label) = control.presentation();
        let trigger =
            button(id, icon, label).when(matches!(control, Control::Reasoning), |button| {
                button
                    .label(effort_label(self.model.effort.as_deref()))
                    .dropdown_caret(true)
            });
        anchored::dropdown_menu(trigger, move |menu, window, cx| {
            let Some(toolbar) = owner.upgrade() else {
                return menu;
            };
            let model = toolbar.read(cx).model.clone();
            NativeMenu::new(control, model, owner.clone()).build(menu, window, cx)
        })
    }

    fn send(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.model().busy;
        let (icon, label) = if busy {
            (IconName::Square, "Stop")
        } else {
            (IconName::ArrowUp, "Send")
        };
        IconButton::new("chat-send")
            .icon(icon)
            .primary()
            .small()
            .rounded_full()
            .accessibility_label(label)
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(if busy {
                    ToolbarEvent::Stop
                } else {
                    ToolbarEvent::Send
                });
            }))
    }
}

impl Render for ChatToolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let units = self.overflow_units(cx);
        let overflow = OverflowToolbar::sync(&mut self.overflow, units, cx);
        self.anchors.set_toolbar(&overflow);
        h_flex()
            .id("chat-composer-toolbar")
            .test_support()
            .flex_nowrap()
            .w_full()
            .min_w_0()
            .min_h(Metrics::current(cx).target())
            .gap(Metrics::current(cx).gap())
            .text_color(tokens.muted)
            .child(div().flex_1().min_w_0().child(overflow))
            .child(div().flex_shrink_0().child(self.send(cx)))
            .child(
                div()
                    .absolute()
                    .child(self.model_anchor.clone())
                    .child(self.usage_anchor.clone()),
            )
    }
}
