use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use subspace_framework::core::Secret;

use super::{FleetApi, TokenError};
use crate::tests::{FleetFixture, TOKEN};
use crate::{FleetRecord, FleetState, Link, TokenStore};

mod keychain;
mod ordering;
mod retries;

const HOME: &str = "https://fleet.example.com";

fn service(
    store: Arc<TokenStore>,
    fleets: Vec<FleetRecord>,
    cx: &mut TestAppContext,
) -> Entity<FleetApi> {
    let service = cx.new(|_| FleetApi::new(store));
    service.update(cx, |service, cx| service.set_fleets(fleets, cx));
    cx.run_until_parked();
    service
}

fn aurora(address: &str) -> Vec<FleetRecord> {
    vec![FleetRecord::new(
        "aurora",
        "Aurora",
        Some(address.to_owned()),
    )]
}

fn save(
    service: &Entity<FleetApi>,
    token: &str,
    cx: &mut TestAppContext,
) -> Result<(), TokenError> {
    let task = service.update(cx, |service, cx| {
        service.save_token("aurora", Secret::new(token), cx)
    });
    cx.run_until_parked();
    cx.foreground_executor().block_test(task)
}

fn state(service: &Entity<FleetApi>, cx: &mut TestAppContext) -> FleetState {
    service.read_with(cx, |service, _| service.state("aurora"))
}

fn token(service: &Entity<FleetApi>, cx: &mut TestAppContext) -> Option<String> {
    service.read_with(cx, |service, _| {
        service
            .client("aurora")
            .ok()
            .map(|client| client.token().expose().to_owned())
    })
}

#[gpui_kit::test]
fn a_token_is_saved_replaced_and_cleared(cx: &mut TestAppContext) {
    let store = Arc::new(TokenStore::memory());
    let service = service(store.clone(), aurora(HOME), cx);
    assert_eq!(state(&service, cx), FleetState::NoToken);
    assert!(service.read_with(cx, |service, _| service.client("aurora").is_err()));

    save(&service, " first ", cx).unwrap();
    assert_eq!(state(&service, cx), FleetState::Ready);
    assert_eq!(token(&service, cx).as_deref(), Some("first"));
    save(&service, "second", cx).unwrap();
    assert_eq!(token(&service, cx).as_deref(), Some("second"));
    assert_eq!(
        save(&service, "two words", cx),
        Err(TokenError::Store(crate::StoreError::InvalidToken))
    );
    assert_eq!(token(&service, cx).as_deref(), Some("second"));

    // Another run of the app reads what was saved.
    let reopened = self::service(store.clone(), aurora(HOME), cx);
    assert_eq!(token(&reopened, cx).as_deref(), Some("second"));

    let clear = service.update(cx, |service, cx| service.clear_token("aurora", cx));
    cx.run_until_parked();
    cx.foreground_executor().block_test(clear).unwrap();
    assert_eq!(state(&service, cx), FleetState::NoToken);
    assert!(store.load("aurora").unwrap().is_none());
}

#[gpui_kit::test]
fn a_changed_address_withholds_the_token_until_it_is_entered_again(cx: &mut TestAppContext) {
    let store = Arc::new(TokenStore::memory());
    let service = service(store.clone(), aurora(HOME), cx);
    save(&service, "home-token", cx).unwrap();

    // The Subspace API now records another address for the fleet.
    service.update(cx, |service, cx| {
        service.set_fleets(aurora("https://elsewhere.example.com"), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        state(&service, cx),
        FleetState::AddressChanged {
            saved_for: Some(HOME.into())
        }
    );
    assert_eq!(token(&service, cx), None);
    let shared = service.read_with(cx, |service, _| service.shared_fleets());
    assert!(shared[0].token.is_none());
    assert_eq!(shared[0].problem, Some("token-address"));
    let test = service.update(cx, |service, cx| {
        service.test_connection("aurora", None, cx)
    });
    assert_eq!(
        cx.foreground_executor().block_test(test).err(),
        Some(TokenError::AddressChanged)
    );

    // Entering the token again binds it to the new address.
    save(&service, "new-token", cx).unwrap();
    assert_eq!(state(&service, cx), FleetState::Ready);
    assert_eq!(token(&service, cx).as_deref(), Some("new-token"));
    let saved = store.load("aurora").unwrap().unwrap();
    assert_eq!(saved.address(), "https://elsewhere.example.com");
}

#[gpui_kit::test]
fn fleets_without_a_usable_address_take_no_token(cx: &mut TestAppContext) {
    let fleets = vec![
        FleetRecord::new("aurora", "Aurora", None),
        FleetRecord::new("plain", "Plain", Some("http://fleet.example.com".into())),
    ];
    let service = service(Arc::new(TokenStore::memory()), fleets, cx);
    let states: Vec<_> = service.read_with(cx, |service, _| {
        service
            .fleets()
            .into_iter()
            .map(|fleet| fleet.state)
            .collect()
    });
    assert_eq!(
        states,
        [FleetState::NotConfigured, FleetState::InvalidAddress]
    );
    assert!(save(&service, "token", cx).is_err());
    let unknown = service.update(cx, |service, cx| {
        service.save_token("nope", Secret::new("t"), cx)
    });
    assert_eq!(
        cx.foreground_executor().block_test(unknown),
        Err(TokenError::UnknownFleet)
    );
}

/// Advances the app's clock and real time until `done`.
fn until(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        if done(cx) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[gpui_kit::test]
fn every_follower_shares_one_stream_per_fleet(cx: &mut TestAppContext) {
    let fixture = FleetFixture::start(&[]);
    let service = service(Arc::new(TokenStore::memory()), aurora(&fixture.origin), cx);
    save(&service, TOKEN, cx).unwrap();
    let first = service.update(cx, |service, cx| service.follow("aurora", cx));
    let second = service.update(cx, |service, cx| service.follow("aurora", cx));
    assert_eq!(service.read_with(cx, |service, _| service.feeds.len()), 1);
    assert!(until(cx, |cx| state(&service, cx)
        == FleetState::Followed(Link::Live)
        && service.read_with(cx, |service, _| {
            service
                .snapshot("aurora")
                .is_some_and(|snapshot| snapshot.overview.is_some())
        })));
    drop(first);
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert_eq!(service.read_with(cx, |service, _| service.feeds.len()), 1);
    drop(second);
    assert!(until(cx, |cx| service
        .read_with(cx, |service, _| service.feeds.is_empty())));
    assert_eq!(state(&service, cx), FleetState::Ready);
}

#[gpui_kit::test]
fn a_refused_token_is_reported_by_the_followed_fleet(cx: &mut TestAppContext) {
    let fixture = FleetFixture::start(&[]);
    let service = service(Arc::new(TokenStore::memory()), aurora(&fixture.origin), cx);
    save(&service, "wrong-token", cx).unwrap();
    let test = service.update(cx, |service, cx| {
        service.test_connection("aurora", None, cx)
    });
    assert_eq!(
        cx.foreground_executor().block_test(test).err(),
        Some(TokenError::Connection(crate::Error::TokenRefused))
    );
    let typed = service.update(cx, |service, cx| {
        service.test_connection("aurora", Some(Secret::new(TOKEN)), cx)
    });
    assert!(
        cx.foreground_executor()
            .block_test(typed)
            .is_ok_and(|fleet| !fleet.ships.is_empty())
    );
    let _follow = service.update(cx, |service, cx| service.follow("aurora", cx));
    assert!(until(cx, |cx| state(&service, cx)
        == FleetState::Followed(Link::TokenRefused)));
}

#[gpui_kit::test]
fn the_shared_service_asks_for_the_initiatives_that_name_the_fleets(cx: &mut TestAppContext) {
    // Fleet Console can be the first Space shown: nothing else has loaded
    // the initiatives when its page asks for the fleets.
    cx.update(|cx| {
        let catalog = subspace_api_client::initiatives::Catalog::shared(cx);
        assert_eq!(catalog.read(cx).hint(), None);
        FleetApi::shared(cx);
        assert_eq!(catalog.read(cx).hint(), Some("Checking initiatives…"));
    });
}

#[gpui_kit::test]
fn an_installed_service_is_the_shared_one_and_nothing_is_created_to_read_it(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| assert!(FleetApi::current(cx).is_none(), "none until made"));
    let installed = service(Arc::new(TokenStore::memory()), aurora(HOME), cx);
    cx.update(|cx| {
        FleetApi::install(installed.clone(), cx);
        assert_eq!(FleetApi::current(cx), Some(installed.clone()));
        assert_eq!(FleetApi::shared(cx), installed);
    });
}
