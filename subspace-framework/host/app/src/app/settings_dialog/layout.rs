//! One retained composition observes the editor without making it own its lifecycle.

use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Subscription, Window, div,
};

use super::super::about_dialog::update_panel::compact::CompactUpdates;
use super::{Settings, footer, lifecycle::Lifecycle};
use crate::app::OpenLogView;

pub(super) struct SettingsDialog {
    settings: Entity<Settings>,
    updates: Option<Entity<CompactUpdates>>,
    lifecycle: Lifecycle,
    _subscription: Subscription,
}

impl SettingsDialog {
    pub(super) fn new(
        settings: Entity<Settings>,
        updates: Option<Entity<CompactUpdates>>,
        lifecycle: Lifecycle,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&settings, |_, _, cx| cx.notify());
        Self {
            settings,
            updates,
            lifecycle,
            _subscription: subscription,
        }
    }
}

impl Render for SettingsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let updates = self.updates.clone().map(Into::into);
        let actions = footer::settings(self.lifecycle.clone()).into_any_element();
        let content = self.settings.update(cx, |settings, cx| {
            settings.render_dialog(window, cx, updates, Some(actions))
        });
        div()
            .on_action(cx.listener(|this, _: &OpenLogView, window, cx| {
                cx.stop_propagation();
                this.lifecycle.show_log(window, cx);
            }))
            .child(content)
    }
}
