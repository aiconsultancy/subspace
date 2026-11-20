use super::{AccountTokenSource, LeaseGate, NativeAccount, Retry, Shared, Status, account_session};
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subspace_account::{Account, Cancel, Error, Provider, ProviderInfo, ServiceToken, Started};
use subspace_core::Secret;

/// A headless provider that, like a careless implementation, keeps serving
/// the outgoing account during a switch and does not advance its generation
/// when it commits the incoming one.
struct Switching {
    generation: AtomicU64,
    account: Mutex<Option<Account>>,
    next: Account,
    failure: Option<Error>,
}

impl Provider for Switching {
    fn info(&self) -> ProviderInfo {
        ProviderInfo::default()
    }
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    fn start(&self) -> Result<Started, Error> {
        Ok(Started::headless())
    }
    fn finish(&self, _: Started, cancel: &Cancel, generation: u64) -> Result<Account, Error> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if generation != self.generation() {
            return Err(Error::Cancelled);
        }
        cancel.begin_commit()?;
        *self.account.lock().unwrap() = Some(self.next.clone());
        Ok(self.next.clone())
    }
    fn restore(&self) -> Result<Option<Account>, Error> {
        Ok(self.account.lock().unwrap().clone())
    }
    fn reconnect(&self) -> Result<Option<Account>, Error> {
        self.restore()
    }
    fn sign_out(&self) -> Result<(), Error> {
        *self.account.lock().unwrap() = None;
        Ok(())
    }
    fn token_with_validity(&self, _: &str, _: Duration) -> Result<ServiceToken, Error> {
        let account = self.restore()?.ok_or(Error::SignInRequired)?;
        Ok(ServiceToken {
            token: Secret::new(account.id.clone()),
            account,
            expires_at: Instant::now() + Duration::from_secs(3600),
        })
    }
}

fn person(id: &str) -> Account {
    Account {
        tenant: "org_fixture".into(),
        id: id.into(),
        name: "Example Developer".into(),
        username: "developer@example.test".into(),
    }
}

fn signed_in(
    failure: Option<Error>,
    cx: &mut TestAppContext,
) -> (Entity<NativeAccount>, AccountTokenSource) {
    let manager: Arc<dyn Provider> = Arc::new(Switching {
        generation: AtomicU64::default(),
        account: Mutex::new(Some(person("user_outgoing"))),
        next: person("user_incoming"),
        failure,
    });
    let gate = Arc::new(LeaseGate::default());
    let source = AccountTokenSource::new(manager.clone(), gate.clone());
    let entity = cx.new(|_| NativeAccount {
        manager: Some(manager),
        gate,
        provider: ProviderInfo::default(),
        account: Some(person("user_outgoing")),
        status: Status::Idle,
        before_sign_in: Status::Idle,
        retry: Retry::Restore,
        cancel: None,
        url: None,
        code: None,
        run: 0,
    });
    cx.update(|cx| cx.set_global(Shared(entity.clone())));
    (entity, source)
}

fn leased(source: &AccountTokenSource) -> Result<String, Error> {
    source
        .token_for("example-api", Duration::ZERO)
        .map(|grant| grant.account.id)
}

#[gpui_kit::test]
fn a_client_bound_while_switching_gets_no_token_and_is_stale_after_commit(cx: &mut TestAppContext) {
    let (entity, source) = signed_in(None, cx);
    let session = cx.update(|cx| account_session(cx)).unwrap();
    assert_eq!(leased(&source).as_deref(), Ok("user_outgoing"));

    entity.update(cx, |account, cx| account.start(cx));
    // A hub created now binds to the switching generation.
    let lease = source.generation();
    assert_eq!(leased(&source), Err(Error::Cancelled));
    assert!(!session.is_current());
    assert_eq!(
        session.token_for("example-api", Duration::ZERO).err(),
        Some(Error::Cancelled)
    );

    cx.run_until_parked();
    cx.update(|cx| {
        let account = entity.read(cx);
        assert_eq!(account.status, Status::Idle);
        assert_eq!(account.account, Some(person("user_incoming")));
    });
    assert_ne!(
        source.generation(),
        lease,
        "the committed account has a generation of its own"
    );
    assert_eq!(leased(&source).as_deref(), Ok("user_incoming"));
}

#[gpui_kit::test]
fn a_cancelled_or_failed_switch_reopens_leases_for_the_existing_account(cx: &mut TestAppContext) {
    let (entity, source) = signed_in(None, cx);
    entity.update(cx, |account, cx| account.start(cx));
    assert_eq!(leased(&source), Err(Error::Cancelled));
    entity.update(cx, |account, cx| account.cancel(cx));
    cx.run_until_parked();
    assert_eq!(leased(&source).as_deref(), Ok("user_outgoing"));

    let (entity, source) = signed_in(Some(Error::Offline), cx);
    entity.update(cx, |account, cx| account.start(cx));
    assert_eq!(leased(&source), Err(Error::Cancelled));
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(entity.read(cx).status, Status::Failed(Error::Offline)));
    assert_eq!(leased(&source).as_deref(), Ok("user_outgoing"));
}

#[gpui_kit::test]
fn signing_out_refuses_leases_until_the_account_is_gone(cx: &mut TestAppContext) {
    let (entity, source) = signed_in(None, cx);
    entity.update(cx, |account, cx| account.sign_out(cx));
    let lease = source.generation();
    assert_eq!(leased(&source), Err(Error::Cancelled));
    cx.run_until_parked();
    assert_ne!(source.generation(), lease);
    assert_eq!(leased(&source), Err(Error::SignInRequired));
}
