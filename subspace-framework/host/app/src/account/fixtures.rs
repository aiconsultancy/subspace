use super::{NativeAccount, Retry, Session, Shared, Status, shared};
use gpui_kit::{App, AppContext as _};
use subspace_account::{Account, Error, ProviderInfo};

/// Seeds `state`; `required-<state>` seeds it for an app that cannot be
/// used without the account, which asks for sign-in over the window.
/// `failed` is a sign-in that could not reach the service, with no session.
pub(crate) fn seed(state: &str, cx: &mut App) -> Result<(), String> {
    let (required, state) = state
        .strip_prefix("required-")
        .map_or((false, state), |state| (true, state));
    let status = status(state).ok_or("Unknown synthetic account state")?;
    let account = (!matches!(
        state,
        "signed-out" | "waiting" | "device" | "completing" | "unavailable" | "failed"
    ))
    .then(account);
    let session = session(state);
    let retry = if state == "failed" {
        Retry::SignIn
    } else {
        Retry::Restore
    };
    let (provider, code) = (
        provider(),
        (state == "device").then(|| "ABCD-EFGH".to_owned()),
    );
    if let Some(entity) = shared(cx) {
        entity.update(cx, |this, cx| {
            (this.required, this.account, this.provider) = (required, account, provider);
            (this.status, this.code, this.session, this.retry) = (status, code, session, retry);
            this.before_sign_in = Status::Idle;
            cx.notify();
        });
    } else {
        let entity = cx.new(|_| NativeAccount {
            required,
            manager: None,
            gate: Default::default(),
            provider,
            account,
            status,
            before_sign_in: Status::Idle,
            retry,
            session,
            cancel: None,
            url: None,
            code,
            run: 0,
        });
        cx.set_global(Shared(entity));
    }
    Ok(())
}

fn status(state: &str) -> Option<Status> {
    Some(match state {
        "signed-out" | "signed-in" => Status::Idle,
        "waiting" | "switching" | "device" => Status::Waiting,
        "completing" => Status::Completing,
        "keychain" => Status::Failed(Error::Keychain),
        "reconnecting" => Status::Refreshing,
        "offline" | "failed" => Status::Failed(Error::Offline),
        "expired" => Status::Failed(Error::SignInRequired),
        "denied" => Status::Failed(Error::NotPermitted),
        "unavailable" => Status::Unavailable,
        _ => return None,
    })
}

/// The session each state stands for: a saved one that could not be
/// checked when offline or the keychain is locked.
fn session(state: &str) -> Session {
    match state {
        "signed-in" | "switching" | "reconnecting" | "denied" => Session::Usable,
        "offline" | "keychain" => Session::Unchecked,
        _ => Session::None,
    }
}

fn account() -> Account {
    Account {
        tenant: "00000000-0000-0000-0000-000000000001".into(),
        id: "00000000-0000-0000-0000-000000000003".into(),
        name: "Example Developer".into(),
        username: "developer@example.test".into(),
    }
}

/// The fictional provider every seeded state signs in with.
fn provider() -> ProviderInfo {
    ProviderInfo {
        name: "Example Identity".into(),
        organization: "Example Org".into(),
        detail: Some((
            "Tenant ID".into(),
            "00000000-0000-0000-0000-000000000001".into(),
        )),
        browser: true,
    }
}
