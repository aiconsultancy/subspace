//! One Settings-owned GitHub draft, composed on board and extension pages,
//! under the GitHub accounts that sign in and out at once.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, ElementId, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    div,
};
use subspace_core::credentials::AccountSource;
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::github_sign_in::{self, Phase};
use subspace_dashboard::{Tokens, anchored};
use subspace_lucide::IconName;

use super::{Settings, help, labelled, note, text};
use crate::settings::{extensions, form::GITHUB_TOKEN};

pub(super) fn render(
    settings: &Settings,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Div {
    let placeholder = if settings.github.found() {
        text::PICK_ACCOUNT
    } else {
        text::NO_ACCOUNTS
    };
    v_flex()
        .gap_3()
        .child(accounts(settings, locked, tokens, cx))
        .child(labelled(
            text::ACCOUNT,
            anchored::select(&settings.account)
                .id("settings-github-account")
                .accessibility_label("GitHub account")
                .placeholder(placeholder)
                .disabled(locked),
        ))
        .child(note(tokens, help::GH_ACCOUNT))
        .child(settings.token_field(GITHUB_TOKEN, locked, tokens, cx))
        .child(note(tokens, help::GH_TOKEN))
}

/// Each account with where its token comes from, Sign out for signed-in
/// ones, then Sign in with GitHub and the running sign-in's status. An app
/// without an OAuth App offers neither Sign in nor Sign out.
fn accounts(settings: &Settings, locked: bool, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let offered = settings.github.sign_in.is_some();
    let sign_out = offered.then_some(locked);
    let rows: Vec<AnyElement> = settings
        .github
        .accounts
        .logins
        .iter()
        .map(|login| {
            let source = settings.github.accounts.source(login);
            account_row(login, source, sign_out, tokens, cx)
        })
        .collect();
    let (empty_help, accounts_help) = if offered {
        (help::GH_NO_ACCOUNTS, help::GH_ACCOUNTS)
    } else {
        (help::GH_NO_CLI_ACCOUNTS, help::GH_CLI_ACCOUNTS)
    };
    let empty = rows.is_empty().then(|| note(tokens, empty_help));
    labelled(
        text::GITHUB_ACCOUNTS,
        v_flex()
            .id("settings-github-accounts")
            .test_support()
            .role(Role::Group)
            .aria_label(text::GITHUB_ACCOUNTS)
            .w_full()
            .gap_1()
            .children(rows)
            .children(empty),
    )
    .children(sign_in_controls(settings, locked, tokens, cx))
    .child(note(tokens, accounts_help))
}

/// Sign in with GitHub and the running sign-in's status, when the app
/// offers it.
fn sign_in_controls(
    settings: &Settings,
    locked: bool,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Vec<AnyElement> {
    let Some(shared) = settings.github.sign_in.as_ref() else {
        return Vec::new();
    };
    let phase = shared.read(cx).phase().clone();
    let busy = !matches!(phase, Phase::Idle | Phase::Failed(_));
    let sign_in = github_sign_in::sign_in_button("settings-github-sign-in", busy || locked)
        .on_click(cx.listener(|settings, _, _, cx| settings.start_sign_in(cx)));
    let cancel = cx.listener(|settings, _, _, cx| settings.cancel_sign_in(cx));
    let status = github_sign_in::status("settings-github-sign-in", &phase, tokens, cancel, cx);
    std::iter::once(h_flex().child(sign_in).into_any_element())
        .chain(status)
        .collect()
}

/// One account row. `sign_out` is `None` when the app offers no sign-in,
/// else whether a signed-in account's Sign out is disabled.
fn account_row(
    login: &str,
    source: AccountSource,
    sign_out: Option<bool>,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let source_colour = match source {
        AccountSource::SignInExpired => tokens.yellow,
        AccountSource::SignedIn | AccountSource::GitHubCli => tokens.muted,
    };
    let sign_out = sign_out.filter(|_| source != AccountSource::GitHubCli);
    let name = login.to_owned();
    h_flex()
        .id(ElementId::Name(
            format!("settings-github-account:{login}").into(),
        ))
        .test_support()
        .gap_2()
        .min_h_6()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(tokens.fg)
                .child(login.to_owned()),
        )
        .child(div().text_color(source_colour).child(source.label()))
        .children(sign_out.map(|locked| {
            action_button(
                ElementId::Name(format!("settings-github-sign-out:{login}").into()),
                Intent::Neutral,
                Some(IconName::LogOut),
                "Sign out",
            )
            .disabled(locked)
            .on_click(cx.listener(move |settings, _, _, cx| settings.sign_out(name.clone(), cx)))
        }))
        .into_any_element()
}

pub(super) fn extension_page(
    settings: &Settings,
    id: &str,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let Some((_, view)) = settings
        .extensions
        .iter()
        .find(|(extension, _)| *extension == id)
    else {
        return div().into_any_element();
    };
    if !extensions::registrations(cx)
        .iter()
        .any(|extension| extension.id == id && extension.has_github_credentials())
    {
        return view.clone().into_any_element();
    }
    v_flex()
        .gap_4()
        .child(render(settings, false, tokens, cx))
        .child(view.clone())
        .into_any_element()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod sign_in_tests;
