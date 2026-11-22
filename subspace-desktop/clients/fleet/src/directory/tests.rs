use std::collections::BTreeMap;

use subspace_api_client::model::MemberInitiative;
use subspace_framework::core::Secret;

use super::{
    FleetRecord, FleetState, TokenLoad, fleets_of, ready_client, retain_tokens, shared_fleet,
    state_of,
};
use crate::FleetAddress;
use crate::feed::Link;
use crate::tokens::SavedToken;

fn record(address: Option<&str>) -> FleetRecord {
    FleetRecord::new("aurora", "Aurora", address.map(str::to_owned))
}

fn saved_for(address: &str) -> TokenLoad {
    TokenLoad::Saved(SavedToken::new(
        &FleetAddress::parse(address).unwrap(),
        Secret::new("fixture-fleet"),
    ))
}

#[test]
fn the_fleets_of_initiatives_are_distinct_and_keep_the_api_address() {
    let initiatives: Vec<MemberInitiative> = serde_json::from_value(serde_json::json!([
        {"id": "1", "name": "One", "channels": "", "fleetId": "aurora",
         "fleet": {"id": "aurora", "name": "Aurora", "fleetApiUrl": "https://fleet.example.com"}},
        {"id": "2", "name": "Two", "channels": "", "fleetId": "aurora",
         "fleet": {"id": "aurora", "name": "Aurora"}},
        {"id": "3", "name": "Three", "channels": "", "fleetId": "borealis",
         "fleet": {"id": "borealis", "slug": "borealis", "name": " "}},
        {"id": "4", "name": "Four", "channels": ""}
    ]))
    .unwrap();
    assert_eq!(
        fleets_of(&initiatives),
        [
            record(Some("https://fleet.example.com")),
            FleetRecord::new("borealis", "borealis", None),
        ]
    );
}

#[test]
fn a_token_is_only_used_for_the_address_it_was_saved_for() {
    let home = record(Some("https://fleet.example.com/"));
    let token = saved_for("https://fleet.example.com");
    let client = ready_client(&home, &token).ok().unwrap();
    assert_eq!(client.address().as_str(), "https://fleet.example.com");
    assert_eq!(client.token().expose(), "fixture-fleet");

    // The API now records another address: the token is not handed out.
    let moved = record(Some("https://elsewhere.example.com"));
    assert_eq!(
        ready_client(&moved, &token).err(),
        Some(FleetState::AddressChanged {
            saved_for: Some("https://fleet.example.com".into())
        })
    );
    let shared = shared_fleet(&moved, &token);
    assert!(shared.token.is_none());
    assert_eq!(
        shared.endpoint.as_deref(),
        Some("https://elsewhere.example.com")
    );
    assert_eq!(shared.problem, Some("token-address"));
}

#[test]
fn every_state_before_a_connection_is_reported() {
    let home = record(Some("https://fleet.example.com"));
    let token = saved_for("https://fleet.example.com");
    assert_eq!(
        state_of(&record(None), &token, None),
        FleetState::NotConfigured
    );
    assert_eq!(
        state_of(&record(Some(" ")), &token, None),
        FleetState::NotConfigured
    );
    assert_eq!(
        state_of(&record(Some("http://fleet.example.com")), &token, None),
        FleetState::InvalidAddress
    );
    assert_eq!(
        state_of(&home, &TokenLoad::Loading, None),
        FleetState::Checking
    );
    assert_eq!(
        state_of(&home, &TokenLoad::Missing, None),
        FleetState::NoToken
    );
    assert_eq!(
        state_of(&home, &TokenLoad::Unavailable, None),
        FleetState::StoreUnavailable
    );
    assert_eq!(state_of(&home, &token, None), FleetState::Ready);
    let live = state_of(&home, &token, Some(Link::Live));
    assert_eq!(live, FleetState::Followed(Link::Live));
    assert!(live.is_live());
    assert_eq!(
        state_of(&home, &token, Some(Link::TokenRefused)),
        FleetState::Followed(Link::TokenRefused)
    );
    assert!(!FleetState::Followed(Link::Unreachable).is_live());
    assert_eq!(
        shared_fleet(&home, &TokenLoad::Loading).problem,
        Some("token-checking")
    );
    assert_eq!(
        shared_fleet(&home, &TokenLoad::Unavailable).problem,
        Some("token-store")
    );
    let shared = shared_fleet(&record(None), &token);
    assert_eq!(
        (shared.endpoint, shared.problem),
        (None, Some("no-endpoint"))
    );
    let shared = shared_fleet(&home, &token);
    assert_eq!(shared.problem, None);
    assert_eq!(
        shared
            .token
            .map(|token| token.expose().to_owned())
            .as_deref(),
        Some("fixture-fleet")
    );
}

#[test]
fn token_loads_follow_the_fleet_list() {
    let mut tokens = BTreeMap::new();
    tokens.insert("gone".to_owned(), TokenLoad::Missing);
    tokens.insert("aurora".to_owned(), TokenLoad::Missing);
    let new = retain_tokens(
        &mut tokens,
        &[record(None), FleetRecord::new("borealis", "Borealis", None)],
    );
    assert_eq!(new, ["borealis"]);
    assert_eq!(tokens.keys().collect::<Vec<_>>(), ["aurora", "borealis"]);
    assert!(matches!(tokens["aurora"], TokenLoad::Missing));
    assert!(matches!(tokens["borealis"], TokenLoad::Loading));
}

#[test]
fn every_state_has_a_label_and_problems_say_what_to_do() {
    assert_eq!(FleetState::Followed(Link::Live).label(), "Live");
    assert!(FleetState::Followed(Link::Live).help().is_none());
    assert!(FleetState::Followed(Link::TokenRefused).help().is_some());
    assert!(
        FleetState::AddressChanged { saved_for: None }
            .help()
            .is_some_and(|help| help.contains("paste the token again"))
    );
}
