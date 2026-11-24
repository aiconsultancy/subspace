//! Native About dialog shared by every product composition.

#[cfg(test)]
mod copy_tests;
mod presentation;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod restart_tests;
pub(super) mod update_panel;
#[cfg(any(test, feature = "test-support"))]
pub(in crate::app) struct FixtureAboutText(pub String);
#[cfg(any(test, feature = "test-support"))]
impl gpui_kit::Global for FixtureAboutText {}

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Context, InteractiveElement as _, ParentElement as _, WeakEntity, Window, px};
use subspace_dashboard::copy_details::CopyDetails;
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::tokens::ButtonRole;
use subspace_dashboard::{CurrentProduct, Modal};
use subspace_lucide::IconName;

use super::{OpenAbout, OpenLogView, SubspaceApp};

const ABOUT_WIDTH: f32 = 560.;

impl SubspaceApp {
    /// Builds the About dialog's update entity and starts the background
    /// startup check (REQ-07). Called once from `app.rs`.
    pub(super) fn install_update_panel(&mut self, cx: &mut Context<Self>) {
        update_panel::install(cx);
        self._subscriptions.extend(update_panel::observe(cx));
    }

    pub(super) fn open_about(
        &mut self,
        _: &OpenAbout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if super::sign_in_gate::shown(cx)
            || window.has_active_dialog(cx)
            || !self.ready_for_about(cx)
        {
            return;
        }
        let context = cx.global::<CurrentProduct>().0.clone();
        let text = about_text(&context, cx);
        let title = format!("About {}", crate::product::desktop_name(cx));
        let app = cx.weak_entity();
        let restart_app = app.clone();
        let update_panel =
            update_panel::about_updates(move |window, cx| restart(&restart_app, window, cx), cx);
        window.open_dialog(cx, move |dialog, _, cx| {
            let close_app = app.clone();
            dialog
                .title(title.clone())
                .width(px(ABOUT_WIDTH))
                .overlay_closable(true)
                .footer(about_footer())
                .child(presentation::identity(&context, &text, cx))
                .when_some(update_panel.clone(), |dialog, panel| {
                    let log_app = app.clone();
                    dialog.child(
                        v_flex()
                            .on_action(move |_: &OpenLogView, window, cx| {
                                window.close_dialog(cx);
                                _ = log_app.update(cx, |app, cx| {
                                    app.open_log_view(&OpenLogView, window, cx)
                                });
                            })
                            .child(panel),
                    )
                })
                .child(presentation::details(&context, &text, cx))
                .children(presentation::credits(&context.product, cx))
                .on_close(move |_, window, cx| {
                    _ = close_app.update(cx, |app, cx| {
                        app.focus_active(window, cx);
                        cx.notify();
                    });
                })
        });
        cx.notify();
    }

    fn ready_for_about(&self, cx: &Context<Self>) -> bool {
        if self.settings.is_some() || self.jig_open.get() || !self.jig.read(cx).can_close() {
            return false;
        }
        self.dashboard()
            .is_none_or(|dashboard| dashboard.modal(cx) != Some(Modal::Dialog))
    }
}

/// About is a Root overlay. The focused element is outside the shell's action
/// path, so route through the window's own shell and uncover its quit prompt.
fn restart(app: &WeakEntity<SubspaceApp>, window: &mut Window, cx: &mut App) {
    window.close_dialog(cx);
    _ = app.update(cx, |app, cx| {
        app.focus_active(window, cx);
        app.request_restart(cx);
    });
}

fn about_text(context: &subspace_product::ProductContext, cx: &gpui_kit::App) -> String {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(text) = cx.try_global::<FixtureAboutText>() {
        return text.0.clone();
    }
    let _ = cx;
    crate::build_metadata::about_text_for(context)
}

fn about_footer() -> DialogFooter {
    DialogFooter::new()
        .child(CopyDetails::diagnostics("about-copy-diagnostics").role(ButtonRole::Dialog))
        .child(DialogClose::new().trigger(|button| {
            IconButton::from_button(button)
                .role(ButtonRole::Dialog)
                .icon(IconName::X)
                .label("Close")
        }))
}

/// App-wide About help, at the foot of the Help panel.
pub(in crate::app) fn help() -> gpui_kit::AnyElement {
    use gpui_kit::IntoElement as _;
    subspace_dashboard::rich::rich_text(
        "help-about",
        subspace_dashboard::rich::Format::Markdown,
        include_str!("about_dialog/help.md"),
    )
    .into_any_element()
}
