//! The catalogue's own state, without an app: fleet retries, whom the
//! memberships were read for, and what an account or organisation change
//! retires.

use std::time::Duration;

use super::super::fleet_retry::FLEET_RETRY_MAX;
use super::super::{Catalog, Error, Read};
use crate::model::MemberInitiative;

/// The wait before the catalogue's next fleet retry, as the next read
/// takes it.
fn next_fleet_retry(catalog: &mut Catalog) -> Option<Duration> {
    let unresolved = catalog.items.iter().any(super::super::fleet_unresolved);
    catalog.fleet_retry.next(unresolved)
}

/// Takes in a read without a remembered choice, as a test of the pure state
/// does.
fn take(catalog: &mut Catalog, actor: &str, result: Result<Vec<MemberInitiative>, Error>) {
    let read = Read::for_test(Some(actor.into()), catalog.scope, result);
    catalog.replace(read, None);
}

#[test]
fn a_failed_fleet_lookup_is_retried_with_capped_backoff_until_it_resolves() {
    let mut catalog = Catalog::default();
    let unresolved = serde_json::from_str(
        r##"[{"id":"one","name":"One","channels":"#one","fleetId":"fleet-1","fleet":null}]"##,
    )
    .unwrap();
    take(&mut catalog, "first", Ok(unresolved));
    let delays: Vec<_> = (0..9).map(|_| next_fleet_retry(&mut catalog)).collect();
    assert_eq!(delays[0], Some(Duration::from_secs(5)));
    assert_eq!(delays[1], Some(Duration::from_secs(10)));
    assert_eq!(delays[5], Some(Duration::from_secs(160)));
    assert_eq!(delays[8], Some(FLEET_RETRY_MAX));

    let resolved = serde_json::from_str(
        r##"[{"id":"one","name":"One","channels":"#one","fleetId":"fleet-1",
             "fleet":{"id":"fleet-1","ircHost":"irc.example.com","ircPort":6697}}]"##,
    )
    .unwrap();
    take(&mut catalog, "first", Ok(resolved));
    assert_eq!(next_fleet_retry(&mut catalog), None);
    assert_eq!(
        catalog.fleet_retry.failures, 0,
        "the next failure starts again"
    );
}

#[test]
fn the_memberships_name_the_account_they_were_read_for() {
    let mut catalog = Catalog::default();
    assert_eq!(catalog.verified_for(), None);
    catalog.fence_scope(Some(7));
    let entries = serde_json::from_str(r##"[{"id":"a","name":"A","channels":"#a"}]"##).unwrap();
    take(&mut catalog, "user-ada", Ok(entries));
    assert_eq!(catalog.verified_for(), Some((Some(7), "user-ada")));
    catalog.fence_scope(Some(8));
    assert_eq!(catalog.verified_for(), None, "another account's scope");
    take(&mut catalog, "user-ada", Err(Error::Network));
    assert_eq!(catalog.verified_for(), None, "a failed read is not known");
}

#[test]
fn verified_identity_change_or_refusal_fences_the_loaded_catalogue() {
    let mut catalog = Catalog::default();
    let entries =
        serde_json::from_str(r##"[{"id":"test","name":"Fixture","channels":"#data"}]"##).unwrap();
    take(&mut catalog, "first", Ok(entries));
    assert_eq!(
        catalog.selected().map(|item| item.id.as_str()),
        Some("test")
    );
    assert!(!catalog.fence_identity(&Ok("first".into())));
    assert!(!catalog.fence_identity(&Err(Error::Network)));
    assert!(catalog.selected().is_some());

    assert!(catalog.fence_identity(&Ok("second".into())));
    assert!(catalog.selected().is_none());
    assert!(catalog.place.is_none(), "nothing is remembered for it");
    assert!(!catalog.has_memberships());
    assert!(catalog.fence_identity(&Err(Error::Forbidden)));
    assert!(catalog.actor.is_none());
}

#[test]
fn another_organisation_retires_the_catalogue_and_its_read_in_flight() {
    let mut catalog = Catalog::default();
    assert!(catalog.fence_scope(Some(1)));
    let entries = serde_json::from_str(r##"[{"id":"a","name":"A","channels":"#a"}]"##).unwrap();
    take(&mut catalog, "person", Ok(entries));
    assert!(catalog.selected().is_some());
    assert!(!catalog.fence_scope(Some(1)), "the same scope keeps it");
    // The same person in organisation B: nothing of A survives, even a
    // read for A that was still in flight or failed in transit.
    assert!(catalog.fence_scope(Some(2)));
    assert!(!catalog.has_memberships());
    assert!(catalog.selected().is_none());
    assert!(catalog.place.is_none());
    assert_eq!(catalog.actor.as_deref(), Some("person"));
    assert!(catalog.fetch.is_none());
    take(&mut catalog, "person", Err(Error::Network));
    assert!(catalog.selected().is_none() && !catalog.has_memberships());
}

#[test]
fn an_older_identity_check_cannot_reinstate_the_previous_account() {
    let mut catalogue = Catalog::default();
    let initial = std::time::Instant::now();
    let later = initial + std::time::Duration::from_secs(1);
    assert!(catalogue.record_identity(&(initial, Ok("first".into()))));
    assert!(catalogue.record_identity(&(later, Err(Error::Forbidden))));
    assert!(!catalogue.record_identity(&(initial, Ok("first".into()))));
    assert!(catalogue.actor.is_none());
}
