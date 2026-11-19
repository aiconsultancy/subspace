//! The toast stack in the bottom-left corner (REQ-12).

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement as _, Styled, div, px,
};
use subspace_dashboard::action::{Intent, icon_button};
use subspace_dashboard::{Toast, ToastTone, Tokens, links};
use subspace_lucide::IconName;

use super::SubspaceApp;

/// How many toasts the stack shows at once.
pub const MAX_TOASTS: usize = 4;

/// Toast layout, in pixels.
const MAX_WIDTH: f32 = 560.;
const GAP: f32 = 10.;
const PAD_LEFT: f32 = 12.;
const PAD_RIGHT: f32 = 6.;
const PAD_Y: f32 = 6.;
const STACK_INSET: f32 = 12.;
const STACK_GAP: f32 = 6.;
const DISMISS_LABEL: &str = "Dismiss notification";

pub struct ShownToast {
    pub id: u64,
    pub toast: Toast,
    /// When set, the toast's link opens the About dialog instead of a URL.
    opens_about: bool,
}

/// Only an HTTPS link is offered, so a toast cannot open a local handler.
fn link_is_safe(href: &str) -> bool {
    href.starts_with("https://")
}

/// The toast text for a newer release found at startup (REQ-07).
fn update_available_text(version: &str) -> String {
    format!("Version {version} is available")
}

fn toast_border_color(tone: ToastTone, tokens: &Tokens) -> gpui_kit::Hsla {
    if tone == ToastTone::Error {
        tokens.red
    } else {
        tokens.line
    }
}

impl SubspaceApp {
    pub(super) fn push_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.push_shown_toast(
            ShownToast {
                id: 0,
                toast,
                opens_about: false,
            },
            cx,
        );
    }

    /// Shows the toasts the MCP service queued. Draining only a non-empty
    /// queue keeps this global observer from notifying itself again.
    pub(super) fn show_mcp_toasts(&mut self, cx: &mut Context<Self>) {
        let queued = cx
            .try_global::<crate::mcp::McpService>()
            .is_some_and(crate::mcp::McpService::has_toasts);
        if queued {
            for toast in cx.global_mut::<crate::mcp::McpService>().take_toasts() {
                self.push_toast(toast, cx);
            }
        }
    }

    /// A toast naming the version found at startup, whose link opens the
    /// About dialog (REQ-07) instead of a URL.
    pub(super) fn push_update_toast(&mut self, version: &str, cx: &mut Context<Self>) {
        let toast = Toast::info(update_available_text(version)).link("", "View");
        self.push_shown_toast(
            ShownToast {
                id: 0,
                toast,
                opens_about: true,
            },
            cx,
        );
    }

    fn push_shown_toast(&mut self, mut shown: ShownToast, cx: &mut Context<Self>) {
        self.next_toast += 1;
        shown.id = self.next_toast;
        let id = shown.id;
        let duration = shown.toast.duration();
        if self.toasts.len() >= MAX_TOASTS {
            self.toasts.remove(0);
        }
        self.toasts.push(shown);
        cx.spawn(async move |app, cx| {
            cx.background_executor().timer(duration).await;
            _ = app.update(cx, |app, cx| app.dismiss_toast(id, cx));
        })
        .detach();
        cx.notify();
    }

    pub(super) fn dismiss_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        self.toasts.retain(|shown| shown.id != id);
        cx.notify();
    }

    fn toast_link(
        shown: &ShownToast,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let id = shown.id;
        let link = shown.toast.link.as_ref()?;
        if shown.opens_about {
            let label = link.label.clone();
            return Some(
                div()
                    .id(("toast-link", id))
                    .text_color(tokens.accent)
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.dismiss_toast(id, cx);
                        app.open_about(&super::OpenAbout, window, cx);
                    })),
            );
        }
        if !link_is_safe(&link.href) {
            return None;
        }
        let href = link.href.clone();
        Some(
            div()
                .id(("toast-link", id))
                .text_color(tokens.accent)
                .cursor_pointer()
                .child(link.label.clone())
                .on_click(move |_, _, cx| {
                    links::open(&href, cx);
                }),
        )
    }

    pub(super) fn toast_element(
        shown: &ShownToast,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = shown.id;
        let toast = &shown.toast;
        let link = Self::toast_link(shown, tokens, cx);
        h_flex()
            .id(("toast", id))
            .max_w(px(MAX_WIDTH))
            .flex_wrap()
            .gap(px(GAP))
            .pl(px(PAD_LEFT))
            .pr(px(PAD_RIGHT))
            .py(px(PAD_Y))
            .bg(tokens.surface)
            .border_1()
            .border_color(toast_border_color(toast.tone, tokens))
            .rounded(tokens.surface_radius)
            .shadow_lg()
            .text_size(tokens.row_font)
            .text_color(tokens.fg_strong)
            .child(
                div()
                    .min_w_0()
                    .whitespace_normal()
                    .child(toast.text.clone()),
            )
            .children(link.map(|link| h_flex().gap_1().child("·").child(link)))
            .child(
                icon_button(
                    ("toast-dismiss", id),
                    Intent::Quiet,
                    IconName::X,
                    DISMISS_LABEL,
                )
                .on_click(cx.listener(move |app, _, _, cx| app.dismiss_toast(id, cx))),
            )
    }

    pub(super) fn render_toasts(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let toasts: Vec<Stateful<Div>> = self
            .toasts
            .iter()
            .map(|shown| Self::toast_element(shown, tokens, cx))
            .collect();
        v_flex()
            .absolute()
            .left(px(STACK_INSET))
            .bottom(px(STACK_INSET))
            .gap(px(STACK_GAP))
            .items_start()
            .children(toasts)
    }
}

#[cfg(test)]
mod tests {
    use super::{link_is_safe, update_available_text};

    #[test]
    pub(super) fn only_https_toast_links_are_offered() {
        assert!(link_is_safe("https://dev.azure.com/item/1"));
        assert!(!link_is_safe("http://internal/item/1"));
        assert!(!link_is_safe("file:///etc/passwd"));
    }

    #[test]
    pub(super) fn the_update_toast_names_the_version() {
        assert_eq!(update_available_text("1.2.0"), "Version 1.2.0 is available");
    }
}
