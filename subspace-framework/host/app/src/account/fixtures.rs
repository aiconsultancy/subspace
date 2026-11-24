use super::{NativeAccount, Retry, Shared, Status, shared};
use gpui_kit::{App, AppContext as _};
use subspace_account::{Account, Error, ProviderInfo};

/// Seeds `state`; `required-<state>` seeds it for an app that cannot be
/// used without the account, which asks for sign-in over the window.
pub(crate) fn seed(state: &str, cx: &mut App) -> Result<(), String> {
    let (required, state) = state
        .strip_prefix("required-")
        .map_or((false, state), |state| (true, state));
    let status = match state {
        "signed-out" | "signed-in" => Status::Idle,
        "waiting" | "switching" | "device" => Status::Waiting,
        "completing" => Status::Completing,
        "keychain" => Status::Failed(Error::Keychain),
        "reconnecting" => Status::Refreshing,
        "offline" => Status::Failed(Error::Offline),
        "expired" => Status::Failed(Error::SignInRequired),
        "denied" => Status::Failed(Error::NotPermitted),
        "unavailable" => Status::Unavailable,
        _ => return Err("Unknown synthetic account state".into()),
    };
    let account = (!matches!(
        state,
        "signed-out" | "waiting" | "device" | "completing" | "unavailable"
    ))
    .then(|| Account {
        tenant: "00000000-0000-0000-0000-000000000001".into(),
        id: "00000000-0000-0000-0000-000000000003".into(),
        name: "Example Developer".into(),
        username: "developer@example.test".into(),
    });
    let provider = provider();
    let code = (state == "device").then(|| "ABCD-EFGH".to_owned());
    if let Some(entity) = shared(cx) {
        entity.update(cx, |this, cx| {
            this.required = required;
            this.account = account;
            this.provider = provider;
            this.status = status;
            this.code = code;
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
            retry: Retry::Restore,
            cancel: None,
            url: None,
            code,
            run: 0,
        });
        cx.set_global(Shared(entity));
    }
    Ok(())
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
