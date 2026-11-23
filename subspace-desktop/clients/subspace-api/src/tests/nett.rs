//! The NETT backend end to end against `scripts/nett_api_test_server.py`,
//! a loopback mock built from NETT's OpenAPI document. No real NETT
//! environment is contacted.

mod features;
mod live;
mod planning;
mod process;
mod server;
mod tracker;

use crate::Client;
use server::NettMock;
use std::time::{Duration, Instant};
use subspace_framework::core::Secret;

pub(super) const TOKEN: &str = "nett-fixture-access";

pub(in crate::tests) fn signed_in(mock: &NettMock, org: Option<&str>) -> Client {
    Client::nett_loopback(&mock.api(), org, Some(Secret::new(TOKEN)))
        .unwrap()
        .without_live()
}

/// The caller's organisation called `name`.
pub(in crate::tests) fn organisation(mock: &NettMock, name: &str) -> String {
    let (_, orgs) = signed_in(mock, None).nett_context().unwrap();
    orgs.into_iter()
        .find(|org| org.name == name)
        .map(|org| org.id)
        .unwrap()
}

pub(in crate::tests) fn works(mock: &NettMock) -> Client {
    let org = organisation(mock, "Fixture Works");
    signed_in(mock, Some(&org))
}

/// Waits for `done`, with room for a machine busy with other builds; the
/// limit only bounds a failing run.
fn wait_for(what: &str, done: &dyn Fn() -> bool) {
    wait(what, Duration::from_secs(30), done);
}

fn wait(what: &str, limit: Duration, done: &dyn Fn() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
