use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions,
    px, size,
};

use super::{FleetSettings, Outcome};
use crate::tests::closed_address;
use crate::{FleetApi, FleetRecord, FleetState, TokenStore};

/// A loopback address nothing listens on: the page follows each fleet, and
/// tests never reach beyond this computer.
fn home() -> String {
    closed_address().as_str().to_owned()
}

fn open(
    store: Arc<TokenStore>,
    address: Option<&str>,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<FleetApi>, Entity<FleetSettings>) {
    cx.update(gpui_kit::init);
    let service = cx.new(|_| FleetApi::new(store));
    service.update(cx, |service, cx| {
        service.set_fleets(
            vec![FleetRecord::new(
                "aurora",
                "Aurora",
                address.map(str::to_owned),
            )],
            cx,
        )
    });
    cx.run_until_parked();
    let page_service = service.clone();
    let (handle, page) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(900.), px(900.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| FleetSettings::new(page_service, window, cx)),
        )
        .unwrap()
    });
    cx.run_until_parked();
    (handle, service, page)
}

fn state(service: &Entity<FleetApi>, cx: &mut TestAppContext) -> FleetState {
    service.read_with(cx, |service, _| service.state("aurora"))
}

fn outcome(page: &Entity<FleetSettings>, cx: &mut TestAppContext) -> Option<Outcome> {
    page.read_with(cx, |page, _| page.outcome("aurora").cloned())
}

fn type_token(handle: AnyWindowHandle, text: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("fleet-token-aurora", cx);
        window.input(text, cx);
    })
    .unwrap();
}

fn click(handle: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_pasted_token_is_saved_replaced_and_cleared_from_settings(cx: &mut TestAppContext) {
    let store = Arc::new(TokenStore::memory());
    let home = home();
    let (handle, service, page) = open(store.clone(), Some(&home), cx);
    assert_eq!(state(&service, cx), FleetState::NoToken);

    click(handle, "fleet-save-aurora", cx);
    assert_eq!(
        outcome(&page, cx),
        Some(Outcome::Failed("Paste the token to save it.".into()))
    );

    type_token(handle, "first-token", cx);
    click(handle, "fleet-save-aurora", cx);
    assert_eq!(
        outcome(&page, cx),
        Some(Outcome::Done(format!(
            "Saved. Subspace sends this token only to {home}."
        )))
    );
    // The field is emptied: the token does not stay in the page.
    let typed = page.read_with(cx, |page, cx| {
        page.input("aurora").unwrap().read(cx).value()
    });
    assert!(typed.is_empty());
    assert!(matches!(
        state(&service, cx),
        FleetState::Ready | FleetState::Followed(_)
    ));
    let saved = store.load("aurora").unwrap().unwrap();
    assert_eq!(saved.address(), home);

    type_token(handle, "second-token", cx);
    click(handle, "fleet-save-aurora", cx);
    let saved = store.load("aurora").unwrap().unwrap();
    let address = crate::FleetAddress::parse(&home).unwrap();
    assert_eq!(
        saved.for_address(&address).unwrap().expose(),
        "second-token"
    );

    // A token pasted, then the saved one cleared: the field empties too, so
    // the pasted token cannot be saved by mistake or stay in the page.
    type_token(handle, "pasted-token", cx);
    click(handle, "fleet-clear-aurora", cx);
    let typed = page.read_with(cx, |page, cx| {
        page.input("aurora").unwrap().read(cx).value()
    });
    assert!(typed.is_empty());
    assert_eq!(state(&service, cx), FleetState::NoToken);
    assert!(store.load("aurora").unwrap().is_none());
    assert_eq!(
        outcome(&page, cx),
        Some(Outcome::Done("Token cleared from this computer.".into()))
    );
}

#[gpui_kit::test]
fn a_changed_address_asks_for_the_token_again(cx: &mut TestAppContext) {
    let store = Arc::new(TokenStore::memory());
    let home = home();
    let elsewhere = format!("{home}/elsewhere");
    store
        .save(
            "aurora",
            &crate::FleetAddress::parse(&home).unwrap(),
            &subspace_framework::core::Secret::new("home-token"),
        )
        .unwrap();
    let (handle, service, page) = open(store.clone(), Some(&elsewhere), cx);
    assert_eq!(
        state(&service, cx),
        FleetState::AddressChanged {
            saved_for: Some(home.clone())
        }
    );
    click(handle, "fleet-test-aurora", cx);
    assert!(matches!(
        outcome(&page, cx),
        Some(Outcome::Failed(text)) if text.contains("Paste the token for the new address")
    ));
    type_token(handle, "new-token", cx);
    click(handle, "fleet-save-aurora", cx);
    assert!(!matches!(
        state(&service, cx),
        FleetState::AddressChanged { .. }
    ));
    assert_eq!(store.load("aurora").unwrap().unwrap().address(), elsewhere);
}

#[gpui_kit::test]
fn a_fleet_without_an_address_offers_no_token_field(cx: &mut TestAppContext) {
    let (handle, service, page) = open(Arc::new(TokenStore::memory()), None, cx);
    assert_eq!(state(&service, cx), FleetState::NotConfigured);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("fleet-token-aurora").is_none());
        assert!(window.try_find("fleet-save-aurora").is_none());
    })
    .unwrap();
    assert!(page.read_with(cx, |page, _| page.outcome("aurora").is_none()));
}
