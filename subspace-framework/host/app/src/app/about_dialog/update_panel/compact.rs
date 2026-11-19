//! A retained Settings presentation, not a second updater or draft owner.

use std::rc::Rc;

use super::{Restart, UpdateAction, UpdatePanel, UpdateState, view};
use gpui_kit::component::{Disableable as _, Sizable as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div,
};
use subspace_dashboard::copy_details::CopyDetails;
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_lucide::IconName;

pub(in crate::app) struct CompactUpdates {
    panel: Entity<UpdatePanel>,
    restart: Restart,
    _subscription: Subscription,
}

impl CompactUpdates {
    pub(super) fn new(
        panel: Entity<UpdatePanel>,
        restart: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&panel, |_, _, cx| cx.notify());
        Self {
            panel,
            restart: Rc::new(restart),
            _subscription: subscription,
        }
    }
}

impl Render for CompactUpdates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self.panel.read(cx);
        let status = summary(panel);
        div()
            .id("settings-updates")
            .test_support()
            .w_full()
            .min_w_0()
            .text_color(Tokens::current(cx).fg)
            .child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .id("update-status")
                            .role(Role::Status)
                            .aria_label(status)
                            .test_support()
                            .w_full()
                            .min_w_0()
                            .child(update_action(
                                panel,
                                self.panel.clone(),
                                self.restart.clone(),
                            )),
                    )
                    .children(
                        panel
                            .error_details
                            .clone()
                            .map(|error| CopyDetails::new("settings-update-copy-details", error)),
                    )
                    .children(view::download_progress(panel)),
            )
    }
}

fn update_action(panel: &UpdatePanel, entity: Entity<UpdatePanel>, restart: Restart) -> IconButton {
    let action = UpdateAction::of(&panel.state);
    let icon = match action {
        UpdateAction::Check => IconName::RefreshCw,
        UpdateAction::Install => IconName::Download,
        UpdateAction::Restart => IconName::RotateCw,
    };
    action_button(
        "settings-update-action",
        Intent::Quiet,
        Some(icon),
        summary(panel),
    )
    .small()
    .w_full()
    .min_w_0()
    .max_w_full()
    .loading(panel.state.busy())
    .disabled(panel.state.busy())
    .tooltip(format!("{}. {}", summary(panel), action.label()))
    .on_click(move |_, window, cx| {
        UpdatePanel::run(&entity, action, window, cx, restart.as_ref());
    })
}

fn summary(panel: &UpdatePanel) -> String {
    match &panel.state {
        UpdateState::NoRelease { .. } => "No releases published yet".into(),
        UpdateState::Idle => format!("{} v{}", panel.app_name, panel.current_version()),
        UpdateState::Available(release) => format!("Update to v{}", release.version),
        UpdateState::Installed(installed) => format!("Restart to use v{}", installed.version),
        _ => panel.status().unwrap_or_default(),
    }
}

pub(in crate::app) fn settings_updates(
    restart: impl Fn(&mut Window, &mut App) + 'static,
    cx: &mut Context<super::SubspaceApp>,
) -> Option<Entity<CompactUpdates>> {
    let panel = super::panel(cx)?;
    Some(cx.new(|cx| CompactUpdates::new(panel, restart, cx)))
}
