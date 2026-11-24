//! The shell's own rendering: its overlays, its actions and the window body.

mod build_actions;
mod dock;
mod layout;
mod space_views;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, Hsla, InteractiveElement as _, IntoElement, ParentElement,
    Render, Stateful, Styled, Window, div, px,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::workspace::{SpaceDock, SpaceDocks};

use super::keys::{APP_CONTEXT, ShowDashboard};
use super::state::window_title;
use super::{
    CloseJig, NextDashboard, PreviousDashboard, Quit, QuitAndRestart, SubspaceApp, ToggleJig,
    ToggleNavigationCollapsed, ToggleNavigationMode, ToggleSpaceInspector, ToggleSpaceNavigator,
    ToggleWrap,
};
use crate::status_bar::{self, StatusFacts};

/// How far down the window an overlay starts, and how dark its backdrop is.
const OVERLAY_TOP: f32 = 80.;
const OVERLAY_DIM: f32 = 0.45;
/// The empty state's padding and text width, in pixels.
const EMPTY_STATE_PAD: f32 = 24.;
const EMPTY_STATE_WIDTH: f32 = 520.;

impl SubspaceApp {
    pub(super) fn ensure_help_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.help_dock.is_some() {
            return;
        }
        let host = super::help_dock::HelpDock::create(
            &cx.entity(),
            |app, window, cx| app.workspace_split(&Tokens::current(cx), window, cx),
            window,
            cx,
        );
        self.help_open = host.read(cx).is_open();
        self._subscriptions
            .push(cx.subscribe(&host, |app, _, event, cx| {
                let super::help_dock::HelpDockEvent::OpenChanged(open) = event;
                app.help_open = *open;
                cx.notify();
            }));
        self.help_dock = Some(host);
    }

    fn help_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.ensure_help_dock(window, cx);
        let content = super::help_dock::HelpContent::from_app(self, cx);
        let host = self.help_dock.as_ref().cloned();
        if let Some(host) = &host {
            host.update(cx, |host, cx| {
                host.sync(content, window, cx);
                host.set_open(self.help_open, window, cx);
            });
        }
        div().flex_1().min_h_0().min_w_0().children(host)
    }

    /// Set the window title when it changes.
    fn sync_title(&mut self, window: &mut Window, cx: &App) {
        let dashboard = self.dashboard();
        let title = window_title(
            crate::product::desktop_name(cx),
            self.main
                .title()
                .or_else(|| dashboard.map(|dashboard| dashboard.title.as_ref())),
            dashboard
                .filter(|_| self.main.is_board())
                .and_then(|dashboard| dashboard.window_title(cx)),
        );
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
    }

    pub(super) fn switch_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell
            .on_action(cx.listener(|app, _: &NextDashboard, window, cx| app.step(1, window, cx)))
            .on_action(
                cx.listener(|app, _: &PreviousDashboard, window, cx| app.step(-1, window, cx)),
            )
            .on_action(cx.listener(|app, action: &ShowDashboard, window, cx| {
                app.show(action.index, window, cx)
            }))
    }

    pub(super) fn render_overlays(
        &mut self,
        tokens: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let jig = self
            .jig_open
            .get()
            .then(|| self.jig.clone().into_any_element());
        let palette = self.render_palette(tokens, window, cx);
        let shortcuts = self
            .shortcuts_anchor
            .read(cx)
            .is_open()
            .then(|| div().into_any_element());
        let toasts = self.render_toasts(tokens, cx).into_any_element();
        let sign_in = super::sign_in_gate::render(tokens, window, cx);
        let quit = self.render_quit_dialog(tokens, cx);
        // Help is a workspace tool, not an overlay covering the board; a
        // maximized terminal does cover it.
        let terminal = self.terminal_maximized().then(|| div().into_any_element());
        let menu = self.cover.menu.is_some().then(|| div().into_any_element());
        let overlays = [
            &jig, &shortcuts, &terminal, &palette, &sign_in, &quit, &menu,
        ];
        self.sync_board_cover(overlays, window, cx);
        jig.into_iter()
            .chain(palette)
            .chain(sign_in)
            .chain(quit)
            .chain([toasts])
            .collect()
    }

    pub(super) fn shell_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell
            .on_action(cx.listener(Self::toggle_palette))
            .on_action(cx.listener(Self::search_items))
            .on_action(cx.listener(Self::open_full_text))
            .on_action(cx.listener(Self::close_full_text))
            .on_action(cx.listener(Self::open_log_view))
            .on_action(cx.listener(Self::close_log_view))
            .on_action(cx.listener(Self::reload))
            .on_action(cx.listener(Self::toggle_help))
            .on_action(cx.listener(Self::toggle_shortcuts))
            .on_action(cx.listener(Self::toggle_board_help))
            .on_action(cx.listener(Self::close_board_help))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::configure_board))
            .on_action(cx.listener(Self::open_about))
            .on_action(cx.listener(Self::close_help))
            .on_action(cx.listener(|app, _: &Quit, _, cx| app.request_quit(cx)))
            .on_action(cx.listener(|app, _: &QuitAndRestart, _, cx| app.request_restart(cx)))
            .on_action(cx.listener(|app, _: &ToggleNavigationMode, window, cx| {
                app.toggle_navigation_mode(window, cx)
            }))
            .on_action(
                cx.listener(|app, _: &ToggleNavigationCollapsed, window, cx| {
                    app.toggle_navigation_collapsed(window, cx)
                }),
            )
            .on_action(cx.listener(|_, _: &ToggleWrap, _, cx| super::toggle_wrap(cx)))
            .on_action(cx.listener(|app, _: &ToggleJig, window, cx| {
                let open = !app.jig_open.get();
                app.set_jig(open, window, cx);
            }))
            .on_action(cx.listener(|app, _: &CloseJig, window, cx| app.set_jig(false, window, cx)))
            .map(|shell| Self::developer_actions(shell, cx))
            .map(|shell| Self::report_issue_actions(shell, cx))
            .map(|shell| Self::space_dock_actions(shell, cx))
    }

    /// The status bar's dock switches act on the Space on show.
    fn space_dock_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell
            .on_action(cx.listener(|app, _: &ToggleSpaceNavigator, window, cx| {
                app.toggle_space_dock(SpaceDock::Navigator, window, cx)
            }))
            .on_action(cx.listener(|app, _: &ToggleSpaceInspector, window, cx| {
                app.toggle_space_dock(SpaceDock::Inspector, window, cx)
            }))
    }

    /// Hands a dock switch's click to the Space on show, then redraws the
    /// status bar with the Space's new state.
    fn toggle_space_dock(&mut self, dock: SpaceDock, window: &mut Window, cx: &mut Context<Self>) {
        if self.space_docks(cx).get(dock).is_none() {
            return;
        }
        if let Some(dashboard) = self.dashboard() {
            dashboard.toggle_dock(dock, window, cx);
        }
        cx.notify();
    }

    /// The docks of the Space on show, for the status bar's switches. None
    /// while another surface, such as the log view, a maximised terminal or
    /// zoomed Help, covers the Space.
    pub(super) fn space_docks(&self, cx: &App) -> SpaceDocks {
        self.dashboard()
            .filter(|_| self.main.is_board() && !self.terminal_maximized() && !self.help_zoomed(cx))
            .map_or(SpaceDocks::NONE, |dashboard| dashboard.docks(cx))
    }

    pub(super) fn status_facts(&self, cx: &App) -> StatusFacts {
        let chat = self.chat.read(cx);
        let status = [
            self.dashboard().and_then(|dashboard| dashboard.status(cx)),
            self.terminal_status(cx),
        ];
        StatusFacts {
            status: join_status(status),
            docks: self.space_docks(cx),
            loading: self.dashboard().is_some_and(|dashboard| dashboard.busy(cx)),
            copilot_busy: chat.is_busy(),
            copilot_attention: chat.needs_attention(),
            copilot_unavailable: chat.is_unavailable(),
            chat_engine: chat.engine_label(),
            chat_icon: chat.engine_icon(),
            chat_disabled: chat.is_disabled(),
            product_version: crate::build_metadata::status_text(
                &cx.global::<subspace_dashboard::CurrentProduct>().0,
            ),
            available_update: super::available_update_version(cx),
            #[cfg(feature = "report-issue")]
            report_issue: super::report_issue::available(cx),
        }
    }

    fn workspace_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let maximized = self
            .shown_terminal()
            .filter(|(_, layout)| layout.maximized && self.main.is_board());
        let Some((view, _)) = maximized else {
            return self.normal_workspace_body(window, cx);
        };
        // A maximized terminal covers the Space and the chat.
        div().flex().flex_1().min_h_0().min_w_0().child(view)
    }

    fn normal_workspace_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let tokens = Tokens::current(cx);
        let views = self.dashboard_views(window, &tokens, cx);
        let gallery = self.chat_open && self.chat.read(cx).canvas_gallery();
        let canvas = if self.chat_open {
            self.chat
                .update(cx, |chat, cx| chat.render_canvas(window, cx))
        } else {
            None
        };
        let parts = dock::WorkspaceParts {
            space: (!gallery).then(|| div().flex_1().min_w_0().min_h_0().children(views)),
            canvas,
            chat: self.chat_open.then(|| self.chat.clone().into_any_element()),
        };
        match self.shown_terminal() {
            Some((view, layout)) => self.place_terminal(parts, view, layout, cx),
            None => dock::row()
                .children(parts.space)
                .children(parts.canvas)
                .children(parts.chat),
        }
    }
}

/// What the window shows when the config file turned every board off.
pub(super) fn empty_state(tokens: &Tokens, cx: &mut Context<SubspaceApp>) -> Div {
    let file = cx
        .try_global::<crate::product::ProductFiles>()
        .map_or_else(subspace_core::config::place, |files| {
            files.config.display().to_string()
        });
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .p(px(EMPTY_STATE_PAD))
        .text_color(tokens.muted)
        .child(
            div()
                .max_w(px(EMPTY_STATE_WIDTH))
                .whitespace_normal()
                .child(super::board_help::no_boards_message(&file)),
        )
        .child(subspace_dashboard::feedback::recovery_action(
            "no-boards-configure",
            subspace_dashboard::action::Intent::Neutral,
            subspace_lucide::IconName::Settings,
            "Configure",
            cx.listener(|app, _, window, cx| {
                app.open_settings(&super::OpenSettings, window, cx);
            }),
        ))
}

/// The Space's status and the terminal's, joined.
fn join_status(parts: [Option<gpui_kit::SharedString>; 2]) -> Option<gpui_kit::SharedString> {
    let parts: Vec<_> = parts.into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · ").into())
}

pub(super) fn overlay() -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .pt(px(OVERLAY_TOP))
        .bg(Hsla::black().opacity(OVERLAY_DIM))
}

impl Render for SubspaceApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_title(window, cx);
        self.ensure_help_dock(window, cx);
        let tokens = Tokens::current(cx);
        let toolbar = self.render_toolbar(&tokens, window, cx).into_any_element();
        let overlays = self.render_overlays(&tokens, window, cx);

        let workspace = self.help_workspace(window, cx);
        self.position_shortcuts(window, cx);

        layout::root(window, cx)
            .key_context(APP_CONTEXT)
            .map(|shell| Self::shell_actions(shell, cx))
            .map(|shell| Self::chat_actions(shell, cx))
            .map(|shell| Self::terminal_actions(shell, cx))
            .map(|shell| Self::switch_actions(shell, cx))
            .relative()
            .size_full()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .child(toolbar)
            .child(workspace)
            .child(status_bar::render(self.status_facts(cx), &tokens))
            .children(overlays)
            .child(self.shortcuts_anchor.clone())
    }
}

#[cfg(test)]
mod cover_tests;

#[cfg(test)]
mod tests {
    use super::join_status;

    #[test]
    fn the_status_bar_joins_the_spaces_status_and_the_terminals() {
        assert_eq!(join_status([None, None]), None);
        assert_eq!(join_status([Some("a".into()), None]).as_deref(), Some("a"));
        assert_eq!(
            join_status([Some("a".into()), Some("⚠ b".into())]).as_deref(),
            Some("a · ⚠ b")
        );
    }
}
