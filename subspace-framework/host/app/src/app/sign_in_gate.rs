//! Sign-in over the main window, for an app that cannot be used without its
//! account (`AccountRegistration::required`). While there is no session it
//! covers the window with the shared dialog treatment: the provider's
//! sign-in, the account's progress and Quit. It cannot be dismissed into the
//! app; it closes once signed in and returns when the session ends. An app
//! without a required account never shows it.

use gpui_kit::component::WindowExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Entity, EntityId, FocusHandle, Global, InteractiveElement as _,
    Stateful, Subscription, Window,
};
use subspace_dashboard::Tokens;

use super::{Quit, QuitAndRestart, SubspaceApp};
use view::{Shown, gate, retries};

mod view;
use crate::account::{self as native, NativeAccount, Status};
use crate::notifications::Route;

/// The shell's side of the gate, kept with the window's app: the account it
/// follows, whether it showed last time, its focus handles and where focus
/// belongs now.
#[derive(Default)]
pub(super) struct SignInGate {
    watched: Option<(EntityId, Subscription)>,
    shown: bool,
    /// Whether the quit confirmation is open over the gate, which then
    /// answers the keyboard instead.
    quitting: bool,
    pub(in crate::app) handles: Option<Handles>,
    /// The control focus goes to when it is not on one of the gate's.
    target: Option<FocusHandle>,
    /// Bring focus back when it leaves the gate's frame or the window is
    /// activated.
    kept: Vec<Subscription>,
    /// A notification's route that arrived while sign-in was due, opened
    /// once signed in.
    pub(in crate::app) held: Option<Route>,
}

impl Global for SignInGate {}

/// One focus handle for the gate's frame and one for each of its controls.
#[derive(Clone)]
pub(in crate::app) struct Handles {
    pub frame: FocusHandle,
    pub quit: FocusHandle,
    pub sign_in: FocusHandle,
    pub retry: FocusHandle,
    pub cancel: FocusHandle,
    pub reopen: FocusHandle,
}

/// The gate over `app`'s window, while the account asks for sign-in. When
/// it appears it closes what was open; while it shows, focus stays on its
/// own controls, unless the quit confirmation (`quitting`) is open over it;
/// when it closes, focus returns to the Space on show.
pub(super) fn render(
    tokens: &Tokens,
    quitting: bool,
    window: &mut Window,
    cx: &mut Context<SubspaceApp>,
) -> Option<AnyElement> {
    let entity = native::shared(cx)?;
    watch_account(&entity, cx);
    let asks = entity.read(cx).asks_for_sign_in();
    let handles = handles(window, cx);
    let shown = Shown::of(entity.read(cx));
    let order = order(&shown, &handles);
    let holding = entity.read(cx).holds_routes();
    let state = cx.default_global::<SignInGate>();
    let was = std::mem::replace(&mut state.shown, asks);
    state.quitting = quitting;
    state.target = order.last().cloned();
    let released = state.held.take_if(|_| !holding);
    match (was, asks, released) {
        (_, _, Some(route)) => cx.defer_in(window, move |app, window, cx| {
            app.open_notification(route, window, cx)
        }),
        (false, true, None) => {
            cx.defer_in(window, |app, window, cx| app.clear_for_sign_in(window, cx))
        }
        (true, false, None) => cx.defer_in(window, |app, window, cx| app.focus_active(window, cx)),
        _ => {}
    }
    if asks && !quitting && !holds_focus(&order, &handles.frame, window) {
        cx.defer_in(window, |_, window, cx| take_focus(window, cx));
    }
    if asks && quitting {
        let quit = super::quit_dialog::quit_focus(cx);
        if !(quit.cancel.is_focused(window) || quit.confirm.is_focused(window)) {
            cx.defer_in(window, move |_, window, cx| window.focus(&quit.cancel, cx));
        }
    }
    asks.then(|| gate(&entity, (&shown, &handles, order), tokens, cx))
}

/// Holds `route` while sign-in is due, so a notification clicked meanwhile
/// opens once signed in, not behind the dialog. True when it was held.
pub(crate) fn hold(route: Route, cx: &mut gpui_kit::App) -> bool {
    let holding = native::shared(cx).is_some_and(|account| account.read(cx).holds_routes());
    if holding {
        cx.default_global::<SignInGate>().held = Some(route);
    }
    holding
}

/// Whether the app asks for sign-in now, as last drawn.
pub(super) fn shown(cx: &gpui_kit::App) -> bool {
    cx.try_global::<SignInGate>()
        .is_some_and(|state| state.shown)
}

/// The gate's focus handles, made once, with the guard that brings focus
/// back when it leaves the frame while the gate shows.
fn handles(window: &mut Window, cx: &mut Context<SubspaceApp>) -> Handles {
    if let Some(handles) = cx
        .try_global::<SignInGate>()
        .and_then(|state| state.handles.clone())
    {
        return handles;
    }
    let handles = Handles {
        frame: cx.focus_handle(),
        quit: cx.focus_handle(),
        sign_in: cx.focus_handle(),
        retry: cx.focus_handle(),
        cancel: cx.focus_handle(),
        reopen: cx.focus_handle(),
    };
    let kept = vec![
        cx.on_focus_out(&handles.frame, window, |_, _, window, cx| {
            take_focus(window, cx)
        }),
        cx.observe_window_activation(window, |_, window, cx| take_focus(window, cx)),
    ];
    let state = cx.default_global::<SignInGate>();
    state.handles = Some(handles.clone());
    state.kept = kept;
    handles
}

/// The gate's controls in the order they show and Tab visits them: Quit,
/// then Retry and the sign-in, or while it runs, Cancel and Open browser
/// again. A sign-in that cannot start now is skipped.
fn order(shown: &Shown, handles: &Handles) -> Vec<FocusHandle> {
    let mut order = vec![handles.quit.clone()];
    match shown.status {
        Status::Starting => order.push(handles.cancel.clone()),
        Status::Waiting => order.extend([handles.cancel.clone(), handles.reopen.clone()]),
        status => {
            if retries(status) {
                order.push(handles.retry.clone());
            }
            if !shown.busy {
                order.push(handles.sign_in.clone());
            }
        }
    }
    order
}

/// Whether focus is on the gate's frame or one of the controls it shows.
fn holds_focus(order: &[FocusHandle], frame: &FocusHandle, window: &Window) -> bool {
    frame.is_focused(window) || order.iter().any(|handle| handle.is_focused(window))
}

/// Moves focus to the gate's main control, while the gate shows and the
/// quit confirmation is not open over it.
fn take_focus(window: &mut Window, cx: &mut gpui_kit::App) {
    let target = cx
        .try_global::<SignInGate>()
        .filter(|state| state.shown && !state.quitting)
        .and_then(|state| state.target.clone());
    if let Some(target) = target {
        window.focus(&target, cx);
    }
}

/// Tab and Shift-Tab move among the gate's own controls, wrapping round,
/// so they never reach anything behind it.
fn cycle(order: &[FocusHandle], back: bool, window: &mut Window, cx: &mut gpui_kit::App) {
    let count = order.len();
    if count == 0 {
        return;
    }
    let at = order.iter().position(|handle| handle.is_focused(window));
    let next = match (at, back) {
        (Some(index), false) => (index + 1) % count,
        (Some(index), true) => (index + count - 1) % count,
        (None, false) => 0,
        (None, true) => count - 1,
    };
    window.focus(&order[next], cx);
}

impl SubspaceApp {
    /// Quitting, which works whatever covers the window.
    pub(super) fn quit_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell
            .on_action(cx.listener(|app, _: &Quit, _, cx| app.request_quit(cx)))
            .on_action(cx.listener(|app, _: &QuitAndRestart, _, cx| app.request_restart(cx)))
    }

    /// Every other shell, Space-switch, terminal and chat action. While the
    /// app asks for sign-in none of them is attached, so neither a key nor a
    /// menu command reaches anything behind the sign-in dialog.
    pub(super) fn app_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        if shown(cx) {
            return shell;
        }
        shell
            .map(|shell| Self::shell_actions(shell, cx))
            .map(|shell| Self::chat_actions(shell, cx))
            .map(|shell| Self::terminal_actions(shell, cx))
            .map(|shell| Self::switch_actions(shell, cx))
    }

    /// Closes what was open when the gate appears, the palette, Settings and
    /// any dialog, so nothing takes input above or beneath it, then focuses
    /// its sign-in.
    fn clear_for_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session = None;
        self.dismiss_shortcuts(window, cx);
        if self.settings.is_some() {
            self.close_settings(window, cx);
        }
        // A window without Kit's root, such as a test window, has no dialogs.
        if window.root::<gpui_kit::base::Root>().flatten().is_some() {
            window.close_all_dialogs(cx);
        }
        take_focus(window, cx);
        cx.notify();
    }
}

/// Renders the app again whenever the account changes, including an
/// account a capture build seeds after start-up.
fn watch_account(entity: &Entity<NativeAccount>, cx: &mut Context<SubspaceApp>) {
    let id = entity.entity_id();
    let watched = cx
        .try_global::<SignInGate>()
        .and_then(|state| state.watched.as_ref())
        .is_some_and(|(watched, _)| *watched == id);
    if !watched {
        let subscription = cx.observe(entity, |_, _, cx| cx.notify());
        cx.default_global::<SignInGate>().watched = Some((id, subscription));
    }
}
