//! Initialise the native account and schedule silent renewal.

use super::{
    AccountRegistration, AccountTokenSource, LeaseGate, NativeAccount, Retry, Shared, Status,
};
use gpui_kit::{App, AppContext as _};
use std::sync::Arc;
use std::time::Duration;

const RENEW_INTERVAL: Duration = Duration::from_secs(60);

pub(crate) fn init(registration: Option<AccountRegistration>, cx: &mut App) {
    let Some(registration) = registration else {
        return;
    };
    let product = cx
        .global::<subspace_dashboard::CurrentProduct>()
        .0
        .product
        .bundle_id
        .clone();
    let Some(manager) = (registration.provider)(&product) else {
        return;
    };
    // Capture builds must never touch real accounts, even with deployed IDs.
    // A local development provider holds none, so capture can show it.
    if cfg!(all(debug_assertions, feature = "capture")) && !manager.local_development() {
        return;
    }
    let gate = Arc::new(LeaseGate::default());
    if let Some(configure) = registration.configure {
        configure(AccountTokenSource::new(manager.clone(), gate.clone()));
    }
    let entity = cx.new(|_| NativeAccount {
        gate,
        status: Status::Idle,
        provider: manager.info(),
        manager: Some(manager),
        account: None,
        before_sign_in: Status::Idle,
        retry: Retry::Restore,
        cancel: None,
        url: None,
        run: 0,
    });
    cx.set_global(Shared(entity.clone()));
    entity.update(cx, |account, cx| account.restore(cx));
    entity.update(cx, |_, cx| {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RENEW_INTERVAL).await;
                if this
                    .update(cx, |this, cx| {
                        if this.status == Status::Idle && this.account.is_some() {
                            this.reconnect(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    });
}
