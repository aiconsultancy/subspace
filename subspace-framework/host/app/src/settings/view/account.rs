//! Immediate account actions, using the shell's existing settings treatment.

use super::{Settings, labelled, note};
use crate::account::{
    self as native, NativeAccount, Status, account_label, sign_in_label, status_text,
};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, Entity, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _,
};
use subspace_account::Error;
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, action_button};
use subspace_lucide::IconName;

pub(super) fn page(tokens: &Tokens, cx: &mut Context<Settings>) -> AnyElement {
    super::page_frame(
        "Use one account across the features of this app that sign in to its services.",
        render(tokens, cx),
        tokens,
        cx,
    )
    .into_any_element()
}

fn render(tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    let Some(entity) = native::shared(cx) else {
        return v_flex();
    };
    let account = entity.read(cx);
    let status = account.status;
    let provider = &account.provider;
    let label = account_label(&provider.organization, &provider.name);
    let mut body = v_flex().id("settings-account").test_support().gap_2();
    if status == Status::Unavailable {
        return labelled(label, body.child(note(tokens, "Sign-in is unavailable.")));
    }
    if let Some(identity) = &account.account {
        body = body
            .child(identity.name.clone())
            .child(note(tokens, identity.username.clone()));
    }
    let code = account
        .code
        .as_deref()
        .filter(|_| status == Status::Waiting);
    if let Some(message) = status_text(status, account.account.is_some(), &provider.name, code) {
        body = body.child(note(tokens, message));
    }
    let actions = if matches!(status, Status::Starting | Status::Waiting) {
        pending_actions(&entity, status)
    } else {
        let signed_in = account.account.is_some();
        ready_actions(&entity, status, signed_in, account.busy(), &provider.name)
    };
    let page = v_flex().gap_3().child(labelled(
        label,
        body.child(actions).child(note(
            tokens,
            "Account changes apply immediately. They do not change your GitHub sign-in.",
        )),
    ));
    match &provider.detail {
        Some((name, value)) => page.child(labelled(name.clone(), note(tokens, value.clone()))),
        None => page,
    }
}

fn pending_actions(entity: &Entity<NativeAccount>, status: Status) -> Div {
    let cancel = entity.clone();
    let mut actions = h_flex().gap_2().flex_wrap().child(
        action_button(
            "settings-account-cancel",
            Intent::Neutral,
            Some(IconName::X),
            "Cancel",
        )
        .on_click(move |_, _, cx| cancel.update(cx, |account, cx| account.cancel(cx))),
    );
    if status == Status::Waiting {
        let reopen = entity.clone();
        actions = actions.child(
            action_button(
                "settings-account-reopen",
                Intent::Neutral,
                Some(IconName::ExternalLink),
                "Open browser again",
            )
            .on_click(move |_, _, cx| reopen.update(cx, |account, cx| account.reopen(cx))),
        );
    }
    actions
}

fn ready_actions(
    entity: &Entity<NativeAccount>,
    status: Status,
    signed_in: bool,
    busy: bool,
    provider: &str,
) -> Div {
    let signin = entity.clone();
    let label = sign_in_label(status, signed_in, provider);
    let mut actions = h_flex().gap_2().flex_wrap().child(
        action_button(
            "settings-account-sign-in",
            Intent::Neutral,
            Some(IconName::LogIn),
            label,
        )
        .disabled(busy)
        .on_click(move |_, _, cx| signin.update(cx, |account, cx| account.start(cx))),
    );
    if matches!(
        status,
        Status::Failed(Error::Offline | Error::Keychain | Error::Service)
    ) {
        let retry = entity.clone();
        actions = actions.child(
            action_button(
                "settings-account-retry",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Retry",
            )
            .on_click(move |_, _, cx| retry.update(cx, |account, cx| account.retry(cx))),
        );
    }
    if signed_in || matches!(status, Status::Failed(_)) {
        let signout = entity.clone();
        actions = actions.child(
            action_button(
                "settings-account-sign-out",
                Intent::Neutral,
                Some(IconName::LogOut),
                "Sign out",
            )
            .disabled(busy)
            .on_click(move |_, _, cx| signout.update(cx, |account, cx| account.sign_out(cx))),
        );
    }
    actions
}

#[cfg(test)]
mod tests;
