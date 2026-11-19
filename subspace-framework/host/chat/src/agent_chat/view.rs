use super::input::CHAT_PANEL_CONTEXT;
use gpui_kit::{AnyElement, IntoElement};

#[cfg(test)]
mod extent_tests;

use super::logic::clamped_width;
use super::sign_in::SignIn;
use super::{
    Animation, ChatPanel, ChatResize, Context, Div, DragMoveEvent, EmptyView, FULL_FRACTION,
    HANDLE_LEFT, HANDLE_WIDTH, REPLAY_BANNER, REVEAL_DURATION, Render, Tokens, Window,
    YOLO_WARNING, div, ease_out_quint, h_flex, px, v_flex,
};
use crate::action::{Intent, action_button};
use crate::rich;
use gpui_kit::component::Disableable as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnimationExt as _, AppContext as _, InteractiveElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, relative,
};
use subspace_lucide::IconName;

/// Chat and a docked Canvas each get half the auxiliary width budget.
const SHARED_WIDTH_FRACTION: f32 = super::MAX_WIDTH_FRACTION / 2.;

pub(super) fn reconnect_intent(sign_in: bool) -> Intent {
    if sign_in {
        Intent::Neutral
    } else {
        Intent::Primary
    }
}

impl ChatPanel {
    /// Limit rendered panes against their actual parent row, not the viewport.
    /// Gallery replaces the board; otherwise reserve its share even beside a
    /// side dock or rail. Stored widths return unchanged when the row widens.
    pub(super) fn pane_width_fraction(&self) -> f32 {
        if !self.canvas_gallery()
            && super::canvas::shown_canvas(&self.tab().canvases, self.sessions_open).is_some()
        {
            SHARED_WIDTH_FRACTION
        } else {
            super::MAX_WIDTH_FRACTION
        }
    }

    pub(super) fn render_yolo_warning(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        (self.capabilities().copilot_controls && self.tab().confirm_yolo).then(|| {
            v_flex()
                .p_2()
                .gap_2()
                .text_color(tokens.yellow)
                .child(rich::plain_text("chat-yolo-warning", YOLO_WARNING))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            action_button(
                                "chat-yolo-cancel",
                                Intent::Neutral,
                                Some(IconName::X),
                                "Cancel",
                            )
                            .on_click(cx.listener(|chat, _, _, cx| {
                                chat.tab_mut().confirm_yolo = false;
                                cx.notify();
                            })),
                        )
                        .child(
                            action_button(
                                "chat-yolo-confirm",
                                Intent::Caution,
                                Some(IconName::ShieldAlert),
                                "Enable YOLO",
                            )
                            .on_click(cx.listener(|chat, _, _, cx| chat.confirm_mode(cx))),
                        ),
                )
        })
    }

    /// This chat's failure, else a failure that belongs to no chat, with
    /// Sign in to GitHub when GitHub rejected the chat account's gh token.
    pub(super) fn render_error(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Option<Div> {
        let error = self.tab().error.clone().or_else(|| self.error.clone());
        let signing = self.signing_in(cx);
        let busy = self.tab().is_busy() || signing;
        // Only a stopped runtime needs a new one. A chat error, such as a
        // refused command or a replay's failure, leaves the runtime running,
        // and restarting it would interrupt every chat.
        let stopped = self.is_unavailable();
        let reconnect = stopped && !self.tab().read_only;
        let sign_in = self.sign_in.is_some() && reconnect;
        let note = self.sign_in.as_ref().and_then(SignIn::note);
        let in_app = self.in_app_status(tokens, cx);
        let reconnect_intent = reconnect_intent(sign_in);
        error.map(|error| {
            v_flex()
                .p_2()
                .gap_1()
                .text_color(tokens.red)
                .child(rich::plain_text("chat-error", &error))
                .children(
                    super::engine::error_guidance(self.engine, reconnect).map(|guidance| {
                        div()
                            .text_color(tokens.muted)
                            .child(rich::plain_text("chat-error-guidance", guidance))
                    }),
                )
                .children(note.map(|note| {
                    div()
                        .text_color(tokens.yellow)
                        .child(rich::plain_text("chat-sign-in-note", &note))
                }))
                .children(in_app)
                .when(sign_in, |column| {
                    column.child(
                        action_button(
                            "chat-sign-in",
                            Intent::Primary,
                            Some(IconName::LogIn),
                            "Sign in to GitHub",
                        )
                        .disabled(signing)
                        .on_click(cx.listener(|chat, _, _, cx| chat.sign_in(cx))),
                    )
                })
                .when(reconnect, |column| {
                    column.child(
                        action_button(
                            "chat-reconnect",
                            reconnect_intent,
                            Some(IconName::RotateCw),
                            "Reconnect",
                        )
                        .disabled(busy)
                        .on_click(cx.listener(|chat, _, window, cx| chat.reconnect(window, cx))),
                    )
                })
                .children(self.copy_legacy_error())
        })
    }

    /// The `/` list, the context pills and the input with its toolbar.
    pub(super) fn render_composer(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_1()
            .child(self.slash.clone())
            .child(self.render_context_pills(cx))
            .child(self.input.clone())
    }

    /// What a replay tab says above its disabled input, with the way back to
    /// the task's own page, where it can still be steered.
    pub(super) fn render_replay_banner(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let tab = self.tab();
        let url = tab.session_url.clone();
        tab.read_only.then(|| {
            v_flex()
                .p_2()
                .gap_1()
                .text_color(tokens.muted)
                .child(rich::plain_text("chat-replay-banner", REPLAY_BANNER))
                .children(url.map(|url| {
                    action_button(
                        "chat-replay-session",
                        Intent::Neutral,
                        Some(IconName::ExternalLink),
                        if url.as_ref() == "https://github.com/copilot/agents" {
                            "Open GitHub Agents in browser"
                        } else {
                            "Open session in browser"
                        },
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        crate::links::open(&url, cx);
                    }))
                }))
        })
    }

    /// The chat below the header, or the session list while the Sessions tab
    /// is selected.
    pub(super) fn render_body(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.is_disabled() {
            return vec![
                div()
                    .id("chat-off")
                    .test_support()
                    .p_2()
                    .text_color(tokens.muted)
                    .child(rich::plain_text("chat-off-guidance", self.guidance()))
                    .into_any_element(),
            ];
        }
        if self.sessions_open {
            return vec![
                self.render_sessions(tokens, cx).into_any_element(),
                div()
                    .children(self.render_notice(tokens))
                    .into_any_element(),
                div()
                    .children(self.render_delete_dialog(cx))
                    .into_any_element(),
            ];
        }
        let mut body = vec![self.render_transcript(tokens, cx).into_any_element()];
        // Everything that needs an answer sits directly above the input.
        body.extend(
            self.render_pending(tokens, cx)
                .map(IntoElement::into_any_element),
        );
        body.extend(
            self.render_yolo_warning(tokens, cx)
                .map(IntoElement::into_any_element),
        );
        body.extend(
            self.render_failure(tokens, cx)
                .map(IntoElement::into_any_element),
        );
        body.extend(
            self.render_error(tokens, cx)
                .map(IntoElement::into_any_element),
        );
        body.extend(
            self.render_replay_banner(tokens, cx)
                .map(IntoElement::into_any_element),
        );
        body.push(self.render_composer(cx).into_any_element());
        body
    }

    /// One line of feedback under the session list, such as a finished export.
    fn render_notice(&self, tokens: &Tokens) -> Option<Div> {
        let notice = self.view.notice.clone().or_else(|| self.error.clone())?;
        Some(
            div()
                .p_2()
                .text_color(tokens.muted)
                .child(rich::plain_text("chat-notice", &notice)),
        )
    }

    pub(super) fn render_frame(&self, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = v_flex().id("agent-chat").when(!self.is_disabled(), |pane| {
            super::strip::bind_tab_actions(pane, cx)
        });
        frame
            .test_support()
            .key_context(CHAT_PANEL_CONTEXT)
            .on_key_down(cx.listener(Self::canvas_key))
            .on_drop::<gpui_kit::ExternalPaths>(
                cx.listener(|chat, paths, _, cx| chat.drop_paths(paths, cx)),
            )
            .relative()
            .h_full()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .bg(tokens.surface)
            .border_l_1()
            .border_color(tokens.line)
            .text_size(tokens.row_font)
            .on_drag_move::<ChatResize>(cx.listener(
                |chat, event: &DragMoveEvent<ChatResize>, window, cx| {
                    chat.resize(
                        f32::from(event.bounds.right() - event.event.position.x),
                        window,
                        cx,
                    );
                },
            ))
            .child(self.render_header(tokens, cx))
            .children(self.render_body(tokens, cx))
            .child(
                div()
                    .id("chat-resize")
                    .absolute()
                    .left(px(HANDLE_LEFT))
                    .top_0()
                    .bottom_0()
                    .w(px(HANDLE_WIDTH))
                    .cursor_col_resize()
                    .on_drag(ChatResize, |_, _, _, cx| cx.new(|_| EmptyView)),
            )
    }
}

impl Render for ChatPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let width = clamped_width(self.width, f32::from(window.viewport_size().width));
        let panel = self.render_frame(&tokens, cx);
        div()
            .h_full()
            .min_w_0()
            .max_w(relative(self.pane_width_fraction()))
            .flex_shrink_0()
            .child(panel)
            .with_animation(
                ("chat-reveal", self.reveal),
                Animation::new(REVEAL_DURATION).with_easing(ease_out_quint()),
                move |pane, delta| {
                    pane.w(px(width * delta))
                        .when(delta < FULL_FRACTION, |pane| pane.overflow_hidden())
                },
            )
    }
}
