//! Sign-in over the main window, for an app that cannot be used without its
//! account (`AccountRegistration::required`). While there is no session it
//! covers the window with the shared dialog treatment: the provider's
//! sign-in, the account's progress and Quit. It cannot be dismissed into the
//! app; it closes once signed in and returns when the session ends. An app
//! without a required account never shows it.

use gpui_kit::component::{Disableable as _, WindowExt as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, Entity, EntityId, FocusHandle, Global, InteractiveElement as _,
    IntoElement as _, ParentElement as _, Role, Stateful, StatefulInteractiveElement as _,
    Styled as _, Subscription, TestSupportExt as _, Window, div,
};
use subspace_account::Error;
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, dialog_button};
use subspace_dashboard::dialog;
use subspace_lucide::IconName;

use super::{Quit, QuitAndRestart, SubspaceApp};
use crate::account::{self as native, NativeAccount, Status, sign_in_label, status_text};

/// The shell's side of the gate, kept with the window's app: the account it
/// follows and whether it showed last time, so focus moves to it once when
/// it appears.
#[derive(Default)]
pub(super) struct SignInGate {
    watched: Option<(EntityId, Subscription)>,
    shown: bool,
    pub(in crate::app) focus: Option<FocusHandle>,
    /// Brings focus back to the gate while it shows.
    kept: Option<Subscription>,
}

impl Global for SignInGate {}

/// The gate over `app`'s window, while the account asks for sign-in. When
/// it appears it closes what was open and takes focus; when it closes,
/// focus returns to the Space on show.
pub(super) fn render(
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<SubspaceApp>,
) -> Option<AnyElement> {
    let entity = native::shared(cx)?;
    watch_account(&entity, cx);
    let asks = entity.read(cx).asks_for_sign_in();
    let focus = gate_focus(window, cx);
    let state = cx.default_global::<SignInGate>();
    let was = std::mem::replace(&mut state.shown, asks);
    match (was, asks) {
        (false, true) => cx.defer_in(window, |app, window, cx| app.clear_for_sign_in(window, cx)),
        (true, false) => cx.defer_in(window, |app, window, cx| app.focus_active(window, cx)),
        _ => {}
    }
    asks.then(|| gate(&entity, &focus, tokens, cx))
}

/// Whether the app asks for sign-in now, as last drawn.
pub(super) fn shown(cx: &gpui_kit::App) -> bool {
    cx.try_global::<SignInGate>()
        .is_some_and(|state| state.shown)
}

/// The sign-in button's focus, made once. While the gate shows, focus that
/// leaves it, as Tab would move it to a control behind, comes back.
fn gate_focus(window: &mut Window, cx: &mut Context<SubspaceApp>) -> FocusHandle {
    if let Some(focus) = cx
        .try_global::<SignInGate>()
        .and_then(|state| state.focus.clone())
    {
        return focus;
    }
    let focus = cx.focus_handle();
    let back = focus.clone();
    let kept = cx.on_focus_out(&focus, window, move |_, _, window, cx| {
        if shown(cx) {
            window.focus(&back, cx);
        }
    });
    let state = cx.default_global::<SignInGate>();
    state.focus = Some(focus.clone());
    state.kept = Some(kept);
    focus
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
        if let Some(focus) = cx
            .try_global::<SignInGate>()
            .and_then(|state| state.focus.clone())
        {
            window.focus(&focus, cx);
        }
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

fn gate(
    entity: &Entity<NativeAccount>,
    focus: &FocusHandle,
    tokens: &Tokens,
    cx: &mut Context<SubspaceApp>,
) -> AnyElement {
    let shown = Shown::of(entity.read(cx));
    let title = format!("Sign in to {}", crate::product::desktop_name(cx));
    let frame = dialog::frame(tokens, dialog::DialogWidth::D540, title.clone())
        .id("sign-in-gate")
        .role(Role::Dialog)
        .aria_label(title)
        .test_support()
        .child(
            v_flex()
                .gap_2()
                .child(introduction(&shown.organization, &shown.provider))
                .children(
                    shown
                        .progress
                        .clone()
                        .map(|text| div().text_color(tokens.muted).child(text)),
                ),
        )
        .child(actions(entity, &shown, focus, cx));
    dialog::backdrop(tokens, frame)
}

/// What the gate shows of the account, read once per render.
struct Shown {
    status: Status,
    provider: String,
    organization: String,
    progress: Option<String>,
    busy: bool,
}

impl Shown {
    fn of(account: &NativeAccount) -> Self {
        let code = account
            .code
            .as_deref()
            .filter(|_| account.status == Status::Waiting);
        Self {
            status: account.status,
            provider: account.provider.name.clone(),
            organization: account.provider.organization.clone(),
            progress: status_text(account.status, false, &account.provider.name, code),
            busy: account.busy(),
        }
    }
}

/// The line under the title: whose account, from the build's branding.
pub(super) fn introduction(organization: &str, provider: &str) -> String {
    match (organization.is_empty(), provider.is_empty()) {
        (false, _) => format!("Sign in with your {organization} account to continue."),
        (true, false) => format!("Sign in with your {provider} account to continue."),
        (true, true) => "Sign in to continue.".into(),
    }
}

/// Quit first, then the sign-in, or while it runs, its Cancel and Open
/// browser again.
fn actions(
    entity: &Entity<NativeAccount>,
    shown: &Shown,
    focus: &FocusHandle,
    cx: &mut Context<SubspaceApp>,
) -> Div {
    let quit = dialog_button(
        "sign-in-gate-quit",
        Intent::Neutral,
        Some(IconName::LogOut),
        "Quit",
    )
    .on_click(cx.listener(|app, _, _, cx| app.request_quit(cx)));
    let footer = dialog::footer().child(quit);
    match shown.status {
        Status::Starting | Status::Waiting => footer.children(pending(entity, shown.status)),
        status => footer.children(retry(entity, status)).child(sign_in(
            entity,
            sign_in_label(status, false, &shown.provider),
            shown.busy,
            focus,
        )),
    }
}

fn pending(entity: &Entity<NativeAccount>, status: Status) -> Vec<AnyElement> {
    let cancel = entity.clone();
    let mut buttons = vec![
        dialog_button(
            "sign-in-gate-cancel",
            Intent::Neutral,
            Some(IconName::X),
            "Cancel",
        )
        .on_click(move |_, _, cx| cancel.update(cx, |account, cx| account.cancel(cx)))
        .into_any_element(),
    ];
    if status == Status::Waiting {
        let reopen = entity.clone();
        buttons.push(
            dialog_button(
                "sign-in-gate-reopen",
                Intent::Primary,
                Some(IconName::ExternalLink),
                "Open browser again",
            )
            .on_click(move |_, _, cx| reopen.update(cx, |account, cx| account.reopen(cx)))
            .into_any_element(),
        );
    }
    buttons
}

/// Retry, after a sign-in that could not reach the service.
fn retry(entity: &Entity<NativeAccount>, status: Status) -> Option<AnyElement> {
    let retry = entity.clone();
    matches!(
        status,
        Status::Failed(Error::Offline | Error::Keychain | Error::Service)
    )
    .then(|| {
        dialog_button(
            "sign-in-gate-retry",
            Intent::Neutral,
            Some(IconName::RefreshCw),
            "Retry",
        )
        .on_click(move |_, _, cx| retry.update(cx, |account, cx| account.retry(cx)))
        .into_any_element()
    })
}

fn sign_in(
    entity: &Entity<NativeAccount>,
    label: String,
    busy: bool,
    focus: &FocusHandle,
) -> AnyElement {
    let signin = entity.clone();
    dialog_button(
        "sign-in-gate-sign-in",
        Intent::Primary,
        Some(IconName::LogIn),
        label,
    )
    .track_focus(focus)
    .disabled(busy)
    .on_click(move |_, _, cx| signin.update(cx, |account, cx| account.start(cx)))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::introduction;

    #[test]
    fn the_gate_names_the_organisation_from_the_build_s_branding() {
        assert_eq!(
            introduction("Example Org", "WorkOS"),
            "Sign in with your Example Org account to continue."
        );
        assert_eq!(
            introduction("", "WorkOS"),
            "Sign in with your WorkOS account to continue."
        );
        assert_eq!(introduction("", ""), "Sign in to continue.");
    }
}
