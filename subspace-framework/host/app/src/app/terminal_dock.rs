//! The docked terminal. The shell decides whether it shows, where it docks,
//! how wide a bottom terminal is and whether it fills the workspace; the
//! panel draws its sessions and header and asks for changes with events.

use gpui_kit::{
    AnyView, App, Context, Div, InteractiveElement as _, Stateful, Subscription, Window,
};
use subspace_dashboard::dock_panel::{
    BottomSpan, DockPanelEvent, DockPanelHandle, PanelLayout, Placement,
};

use super::{
    MoveTerminalLeft, MoveTerminalRight, MoveTerminalToBottom, ShowTerminalHelp, SubspaceApp,
    ToggleTerminal, ToggleTerminalFullWidth, ToggleTerminalMaximized, save_prefs,
};

pub(super) struct TerminalDock {
    pub(super) panel: DockPanelHandle,
    pub(super) state: DockState,
    _events: Subscription,
}

/// Whether the terminal shows, and its layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct DockState {
    pub(super) open: bool,
    pub(super) layout: PanelLayout,
}

/// What asks the terminal to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Change {
    Toggle,
    Hide,
    ToggleMaximized,
    Move(Placement),
    Span(BottomSpan),
    Labels(bool),
    /// Leave maximize, as switching Space does.
    Restore,
    /// Settings saved new stored choices.
    Stored(PanelLayout),
}

/// Where focus goes after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusMove {
    Panel,
    Space,
    Stay,
}

impl From<DockPanelEvent> for Change {
    fn from(event: DockPanelEvent) -> Self {
        match event {
            DockPanelEvent::Hide => Self::Hide,
            DockPanelEvent::ToggleMaximized => Self::ToggleMaximized,
            DockPanelEvent::Move(placement) => Self::Move(placement),
            DockPanelEvent::SetBottomSpan(span) => Self::Span(span),
            DockPanelEvent::SetLabels(labels) => Self::Labels(labels),
        }
    }
}

/// The state after `change`. Hiding, moving and changing the bottom span
/// leave maximize; maximizing a hidden terminal shows it.
pub(super) fn transition(state: DockState, change: Change) -> DockState {
    let layout = state.layout;
    match change {
        Change::Toggle => DockState {
            open: !state.open,
            layout: PanelLayout {
                maximized: false,
                ..layout
            },
        },
        Change::Hide => DockState {
            open: false,
            layout: PanelLayout {
                maximized: false,
                ..layout
            },
        },
        Change::ToggleMaximized => DockState {
            open: true,
            layout: PanelLayout {
                maximized: !(state.open && layout.maximized),
                ..layout
            },
        },
        Change::Move(placement) => DockState {
            open: state.open,
            layout: PanelLayout {
                placement,
                maximized: false,
                ..layout
            },
        },
        Change::Span(span) => DockState {
            open: state.open,
            layout: PanelLayout {
                span,
                maximized: false,
                ..layout
            },
        },
        Change::Labels(labels) => DockState {
            open: state.open,
            layout: PanelLayout { labels, ..layout },
        },
        Change::Restore => DockState {
            open: state.open,
            layout: PanelLayout {
                maximized: false,
                ..layout
            },
        },
        Change::Stored(stored) => DockState {
            open: state.open,
            layout: PanelLayout {
                maximized: layout.maximized,
                ..stored
            },
        },
    }
}

/// Showing the terminal focuses its session and hiding it returns focus to
/// the Space; a layout change from the panel keeps focus in the terminal.
pub(super) fn focus_move(before: DockState, after: DockState, change: Change) -> FocusMove {
    match change {
        Change::Toggle | Change::Hide if before.open && !after.open => FocusMove::Space,
        Change::Toggle | Change::ToggleMaximized | Change::Move(_) | Change::Span(_)
            if after.open =>
        {
            FocusMove::Panel
        }
        _ => FocusMove::Stay,
    }
}

impl TerminalDock {
    /// The product's terminal for this window, shown again when it was open
    /// when the app last quit.
    pub(super) fn create(window: &mut Window, cx: &mut Context<SubspaceApp>) -> Option<Self> {
        let registration = crate::terminal::registration(cx)?;
        let panel = (registration.create)(window, cx);
        let (open, layout) = crate::terminal::stored(cx);
        panel.set_layout(layout, cx);
        let app = cx.weak_entity();
        let events = panel.subscribe(
            window,
            cx,
            Box::new(move |event, window, cx| {
                _ = app.update(cx, |app, cx| {
                    app.change_terminal(Change::from(*event), window, cx);
                });
            }),
        );
        let _ = open.then(|| panel.show(window, cx));
        Some(Self {
            panel,
            state: DockState { open, layout },
            _events: events,
        })
    }
}

impl SubspaceApp {
    /// Applies `change`, stores the result and moves focus as it says.
    pub(super) fn change_terminal(
        &mut self,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dock) = self
            .terminal
            .as_mut()
            .filter(|_| !super::sign_in_gate::shown(cx))
        else {
            return;
        };
        let before = dock.state;
        let after = transition(before, change);
        dock.state = after;
        dock.panel.set_layout(after.layout, cx);
        crate::terminal::store(after.open, after.layout, cx);
        save_prefs(cx);
        self.move_focus(focus_move(before, after, change), window, cx);
        cx.notify();
    }

    fn move_focus(&self, focus: FocusMove, window: &mut Window, cx: &mut Context<Self>) {
        match (focus, &self.terminal) {
            (FocusMove::Panel, Some(dock)) => dock.panel.show(window, cx),
            (FocusMove::Space, _) => self.focus_active(window, cx),
            _ => {}
        }
    }

    /// Settings saved the stored layout: apply it.
    pub(super) fn sync_terminal_preferences(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (_, layout) = crate::terminal::stored(cx);
        self.change_terminal(Change::Stored(layout), window, cx);
    }

    /// The terminal while it shows: its view and layout.
    pub(super) fn shown_terminal(&self) -> Option<(AnyView, PanelLayout)> {
        self.terminal
            .as_ref()
            .filter(|dock| dock.state.open)
            .map(|dock| (dock.panel.view(), dock.state.layout))
    }

    /// Whether the terminal fills the workspace.
    pub(super) fn terminal_maximized(&self) -> bool {
        self.shown_terminal()
            .is_some_and(|(_, layout)| layout.maximized)
    }

    pub(super) fn terminal_minimum(&self, cx: &App) -> Option<gpui_kit::Size<gpui_kit::Pixels>> {
        self.terminal
            .as_ref()
            .and_then(|dock| dock.panel.minimum_size(cx))
    }

    /// Whether a terminal session, or anything else in the panel, has focus.
    pub(super) fn terminal_focused(&self, window: &Window, cx: &App) -> bool {
        self.terminal
            .as_ref()
            .filter(|dock| dock.state.open)
            .is_some_and(|dock| dock.panel.focus_handle(cx).contains_focused(window, cx))
    }

    /// The terminal's guide while Help shows it.
    pub(crate) fn terminal_help(
        &self,
        cx: &App,
    ) -> Option<(&'static str, &'static [subspace_dashboard::HelpImage])> {
        crate::terminal::registration(cx)
            .filter(|_| self.help_terminal)
            .map(|registration| (registration.help_markdown, registration.help_images))
    }

    pub(super) fn terminal_quit_warnings(&self, cx: &App) -> Vec<gpui_kit::SharedString> {
        self.terminal
            .as_ref()
            .map(|dock| dock.panel.quit_warnings(cx))
            .unwrap_or_default()
    }

    pub(super) fn terminal_status(&self, cx: &App) -> Option<gpui_kit::SharedString> {
        self.terminal
            .as_ref()
            .filter(|dock| dock.state.open)
            .and_then(|dock| dock.panel.status(cx))
    }

    /// Reloads the terminal's configuration when it has focus; returns
    /// whether it did.
    pub(super) fn reload_terminal(&self, window: &mut Window, cx: &mut App) -> bool {
        let focused = self.terminal_focused(window, cx);
        let dock = self.terminal.as_ref().filter(|_| focused);
        dock.map(|dock| dock.panel.reload(window, cx)).is_some()
    }

    fn show_terminal_help(
        &mut self,
        _: &ShowTerminalHelp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.is_some() || self.terminal.is_none() {
            return;
        }
        self.help_terminal = true;
        self.set_help_open(true, window, cx);
    }

    pub(super) fn terminal_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell
            .on_action(cx.listener(|app, _: &ToggleTerminal, window, cx| {
                app.change_terminal(Change::Toggle, window, cx);
            }))
            .on_action(cx.listener(|app, _: &ToggleTerminalMaximized, window, cx| {
                app.change_terminal(Change::ToggleMaximized, window, cx);
            }))
            .on_action(cx.listener(|app, _: &MoveTerminalLeft, window, cx| {
                app.change_terminal(Change::Move(Placement::Left), window, cx);
            }))
            .on_action(cx.listener(|app, _: &MoveTerminalToBottom, window, cx| {
                app.change_terminal(Change::Move(Placement::Bottom), window, cx);
            }))
            .on_action(cx.listener(|app, _: &MoveTerminalRight, window, cx| {
                app.change_terminal(Change::Move(Placement::Right), window, cx);
            }))
            .on_action(cx.listener(|app, _: &ToggleTerminalFullWidth, window, cx| {
                let span = app.terminal.as_ref().map(|dock| dock.state.layout.span);
                app.change_terminal(Change::Span(other_span(span)), window, cx);
            }))
            .on_action(cx.listener(Self::show_terminal_help))
    }
}

/// The bottom span that full width switches to.
fn other_span(span: Option<BottomSpan>) -> BottomSpan {
    match span {
        Some(BottomSpan::FullWidth) => BottomSpan::BesideChat,
        _ => BottomSpan::FullWidth,
    }
}

#[cfg(test)]
mod tests {
    use subspace_dashboard::dock_panel::{BottomSpan, PanelLayout, Placement};

    use super::{Change, DockState, FocusMove, focus_move, other_span, transition};

    #[test]
    fn full_width_switches_between_the_two_bottom_spans() {
        assert_eq!(other_span(None), BottomSpan::FullWidth);
        assert_eq!(
            other_span(Some(BottomSpan::BesideChat)),
            BottomSpan::FullWidth
        );
        assert_eq!(
            other_span(Some(BottomSpan::FullWidth)),
            BottomSpan::BesideChat
        );
    }

    fn shown(maximized: bool) -> DockState {
        DockState {
            open: true,
            layout: PanelLayout {
                maximized,
                ..PanelLayout::default()
            },
        }
    }

    #[test]
    fn toggling_shows_and_hides_and_moves_focus_between_the_terminal_and_the_space() {
        let hidden = DockState::default();
        let open = transition(hidden, Change::Toggle);
        assert!(open.open);
        assert_eq!(focus_move(hidden, open, Change::Toggle), FocusMove::Panel);
        let closed = transition(shown(true), Change::Toggle);
        assert!(!closed.open && !closed.layout.maximized);
        assert_eq!(
            focus_move(shown(true), closed, Change::Toggle),
            FocusMove::Space
        );
        let hidden_again = transition(shown(false), Change::Hide);
        assert!(!hidden_again.open);
        assert_eq!(
            focus_move(hidden, transition(hidden, Change::Hide), Change::Hide),
            FocusMove::Stay
        );
    }

    #[test]
    fn maximize_shows_the_terminal_and_hiding_moving_or_switching_restores_it() {
        let maximized = transition(DockState::default(), Change::ToggleMaximized);
        assert!(maximized.open && maximized.layout.maximized);
        assert!(
            !transition(maximized, Change::ToggleMaximized)
                .layout
                .maximized
        );
        assert!(!transition(maximized, Change::Restore).layout.maximized);
        assert!(transition(maximized, Change::Restore).open);
        let moved = transition(maximized, Change::Move(Placement::Left));
        assert_eq!(moved.layout.placement, Placement::Left);
        assert!(!moved.layout.maximized);
        let spanned = transition(maximized, Change::Span(BottomSpan::FullWidth));
        assert_eq!(spanned.layout.span, BottomSpan::FullWidth);
        assert!(!spanned.layout.maximized);
        assert_eq!(
            focus_move(shown(false), maximized, Change::ToggleMaximized),
            FocusMove::Panel
        );
        assert_eq!(
            focus_move(maximized, shown(false), Change::Restore),
            FocusMove::Stay
        );
    }

    #[test]
    fn labels_and_stored_choices_keep_open_state_and_focus() {
        let labelled = transition(shown(true), Change::Labels(true));
        assert!(labelled.layout.labels && labelled.layout.maximized && labelled.open);
        let stored = PanelLayout {
            placement: Placement::Right,
            span: BottomSpan::FullWidth,
            maximized: false,
            labels: true,
        };
        let applied = transition(shown(true), Change::Stored(stored));
        assert_eq!(applied.layout.placement, Placement::Right);
        assert!(applied.layout.maximized && applied.open);
        assert_eq!(
            focus_move(shown(true), applied, Change::Stored(stored)),
            FocusMove::Stay
        );
    }
}
