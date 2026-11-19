use std::rc::Rc;

use super::{Restart, UpdateAction, UpdatePanel, UpdateState, updates::download_percentage};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::component::{progress::Progress, spinner::Spinner};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _,
    Window, div,
};
use subspace_dashboard::copy_details::CopyDetails;
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::{
    Tokens,
    action::{Intent, action_button},
    links,
};
use subspace_lucide::IconName;

pub(in crate::app::about_dialog) struct AboutUpdates {
    pub(super) panel: Entity<UpdatePanel>,
    restart: Restart,
    _subscription: Subscription,
}

impl AboutUpdates {
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

impl Render for AboutUpdates {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self.panel.read(cx);
        div().when(panel.has_updater(), |body| {
            body.child(
                GroupBox::new()
                    .id("about-updates")
                    .title("Updates")
                    .fill()
                    .child(content(
                        panel,
                        self.panel.clone(),
                        self.restart.clone(),
                        &Tokens::current(cx),
                    )),
            )
        })
    }
}

fn content(
    panel: &UpdatePanel,
    entity: Entity<UpdatePanel>,
    restart: Restart,
    tokens: &Tokens,
) -> AnyElement {
    v_flex()
        .min_w_0()
        .gap_2()
        .child(
            h_flex()
                .items_start()
                .flex_wrap()
                .gap_3()
                .child(details(panel, tokens))
                .child(update_button(panel, entity, restart)),
        )
        .children(
            panel
                .error_details
                .clone()
                .map(|error| CopyDetails::new("update-copy-details", error)),
        )
        .children(download_progress(panel))
        .into_any_element()
}

fn details(panel: &UpdatePanel, tokens: &Tokens) -> AnyElement {
    let status = panel
        .status()
        .unwrap_or_else(|| "Check for a newer version".into());
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(status_element(status, panel.state.busy(), tokens))
        .children(panel.release_link().map(|url| {
            div()
                .id("update-release-link")
                .aria_label("View on GitHub")
                .test_support()
                .child(links::external(
                    "release-target",
                    "View on GitHub",
                    url.to_owned(),
                    tokens,
                ))
        }))
        .into_any_element()
}

fn update_button(panel: &UpdatePanel, entity: Entity<UpdatePanel>, restart: Restart) -> IconButton {
    let action = UpdateAction::of(&panel.state);
    let (id, icon) = match action {
        UpdateAction::Check => ("check-for-updates", IconName::RefreshCw),
        UpdateAction::Install => ("install-update", IconName::Download),
        UpdateAction::Restart => ("restart-to-update", IconName::RotateCw),
    };
    action_button(id, Intent::Quiet, Some(icon), action.label())
        .small()
        .accessibility_label(action.label())
        .loading(panel.state.busy())
        .disabled(panel.state.busy())
        .on_click(move |_, window, cx| {
            UpdatePanel::run(&entity, action, window, cx, restart.as_ref());
        })
}

pub(super) fn download_progress(panel: &UpdatePanel) -> Option<AnyElement> {
    if !matches!(panel.state, UpdateState::Installing(_)) {
        return None;
    }
    download_percentage(&panel.progress).map(|percent| {
        Progress::new("update-download-progress")
            .xsmall()
            .value(percent)
            .accessibility_label(format!("Update download: {percent:.0}%"))
            .into_any_element()
    })
}

fn status_element(status: String, busy: bool, tokens: &Tokens) -> AnyElement {
    h_flex()
        .id("update-status")
        .role(Role::Status)
        .aria_label(status.clone())
        .test_support()
        .min_w_0()
        .gap_2()
        .items_start()
        .text_sm()
        .whitespace_normal()
        .text_color(tokens.fg)
        .children(busy.then(|| {
            div()
                .id("update-progress-spinner")
                .test_support()
                .flex_shrink_0()
                .child(Spinner::new().small())
        }))
        .child(div().min_w_0().child(status))
        .into_any_element()
}
