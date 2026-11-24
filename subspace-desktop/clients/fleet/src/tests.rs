//! The client and the feed against the loopback Fleet API fixture, and
//! against raw loopback servers for what the fixture cannot show.

mod fixture;

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::time::Duration;

use subspace_framework::core::Secret;

use crate::feed::{Feed, Link, Pace};
use crate::{Error, FleetAddress, FleetClient};
pub(crate) use fixture::{FleetFixture, TOKEN, closed_address, eventually};

fn quick() -> Pace {
    Pace {
        first_retry: Duration::from_millis(100),
        max_retry: Duration::from_millis(400),
        stable: Duration::from_secs(1),
        poll: Duration::from_millis(200),
        stream_retry: Duration::from_secs(2),
    }
}

#[test]
fn reads_the_fleet_and_a_ship_with_its_crew() {
    let fixture = FleetFixture::start(&[]);
    let client = fixture.client(TOKEN);
    let fleet = client.fleet().unwrap();
    assert_eq!(fleet.api_version, 1);
    assert!(!fleet.name.is_empty());
    assert!(fleet.admiral.is_some());
    assert!(fleet.host.as_ref().is_some_and(|host| host.cpu.cores > 0));
    let first = fleet.ships.first().expect("the fixture has ships");
    assert!(first.crew.is_some_and(|crew| crew.people > 0));
    let detail = client.ship(&first.id).unwrap();
    assert_eq!(detail.ship.id, first.id);
    assert!(!detail.crew_members.is_empty());
    assert!(
        detail
            .crew_members
            .iter()
            .all(|member| !member.contexts.is_empty())
    );
    assert_eq!(client.ship("missing").err(), Some(Error::NotFound));
}

#[test]
fn reads_the_selected_member_and_its_projects() {
    let (address, server) = answer_once(concat!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
        r#"{"ship":"lyra","member":{"id":"wren","contexts":[{"project":null,"git":null},{"project":"maps","current":{"progress":"Two checks left"},"git":{"branch":"feat/maps","changedFiles":0},"pullRequest":{"number":3,"url":"https://example.com/maps/pull/3"}}]}}"#
    ));
    let detail = FleetClient::new(address, Secret::new(TOKEN))
        .crew("lyra", "wren")
        .unwrap();
    let head = server.join().unwrap();
    assert_eq!(
        head.lines().next().unwrap(),
        "GET /stargate/api/v1/ships/lyra/crew/wren HTTP/1.1"
    );
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case(&format!("authorization: Bearer {TOKEN}")))
    );
    let projects = &detail.member.contexts;
    assert_eq!(projects.len(), 2);
    assert!(projects[0].project.is_none() && projects[0].git.is_none());
    assert_eq!(
        projects[1].current.as_ref().unwrap().progress,
        "Two checks left"
    );
    assert_eq!(projects[1].git.as_ref().unwrap().changed_files, Some(0));
    assert_eq!(projects[1].pull_request.as_ref().unwrap().number, 3);
}

#[test]
fn a_crew_read_reports_missing_or_mismatched_members() {
    for (reply, error) in [
        (
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            Error::NotFound,
        ),
        (
            concat!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n",
                r#"{"ship":"lyra","member":{"id":"other"}}"#
            ),
            Error::InvalidResponse,
        ),
    ] {
        let (address, server) = answer_once(reply);
        assert_eq!(
            FleetClient::new(address, Secret::new(TOKEN)).crew("lyra", "wren"),
            Err(error)
        );
        server.join().unwrap();
    }
}

#[test]
fn a_refused_token_is_reported_and_the_feed_stops_using_it() {
    let fixture = FleetFixture::start(&[]);
    let client = fixture.client("not-the-token");
    assert_eq!(client.fleet().err(), Some(Error::TokenRefused));
    let feed = Feed::start_with(client, quick());
    assert!(eventually(|| feed.link() == Link::TokenRefused));
    assert!(feed.snapshot().overview.is_none());
}

#[test]
fn an_unreachable_fleet_is_reported_and_retried() {
    let client = FleetClient::new(closed_address(), Secret::new(TOKEN));
    assert_eq!(client.fleet().err(), Some(Error::Unreachable));
    let feed = Feed::start_with(client, quick());
    assert!(eventually(|| feed.link() == Link::Unreachable));
}

#[test]
fn the_event_stream_delivers_the_fleet_its_ships_and_activity() {
    let fixture = FleetFixture::start(&[]);
    let feed = Feed::start_with(fixture.client(TOKEN), quick());
    assert!(eventually(|| {
        let snapshot = feed.snapshot();
        snapshot.link == Link::Live
            && snapshot.overview.as_ref().is_some_and(|fleet| {
                !fleet.ships.is_empty() && snapshot.ships.len() == fleet.ships.len()
            })
    }));
    // The fixture's crew works every 0.3 s.
    let before = feed.revision();
    assert!(eventually(|| !feed.snapshot().activity.is_empty()));
    assert!(feed.revision() > before);
}

#[test]
fn a_refused_stream_falls_back_to_reading_on_a_timer() {
    let fixture = FleetFixture::start(&["--refuse-events"]);
    let feed = Feed::start_with(fixture.client(TOKEN), quick());
    assert!(eventually(|| {
        let snapshot = feed.snapshot();
        snapshot.link == Link::Polling
            && snapshot
                .overview
                .as_ref()
                .is_some_and(|fleet| snapshot.ships.len() == fleet.ships.len())
    }));
}

/// A one-request loopback server that answers `reply` and hands back the
/// request's head.
fn answer_once(reply: &'static str) -> (FleetAddress, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut head = String::new();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            head.push_str(&line);
        }
        let _ = stream.write_all(reply.as_bytes());
        head
    });
    let address = FleetAddress::parse(&format!("http://127.0.0.1:{port}/stargate")).unwrap();
    (address, server)
}

#[test]
fn the_token_goes_only_in_the_authorization_header_and_redirects_are_not_followed() {
    let (address, server) = answer_once(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.2:9/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let client = FleetClient::new(address, Secret::new("fixture-secret-token"));
    assert_eq!(client.fleet().err(), Some(Error::UnexpectedStatus(302)));
    let head = server.join().unwrap();
    let request_line = head.lines().next().unwrap();
    assert_eq!(request_line, "GET /stargate/api/v1/fleet HTTP/1.1");
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case("authorization: Bearer fixture-secret-token"))
    );
    assert_eq!(head.matches("fixture-secret-token").count(), 1);
}

#[test]
fn unknown_fields_from_a_newer_stargate_are_ignored() {
    let (address, server) = answer_once(concat!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
        r#"{"apiVersion":1,"name":"Future","warpDrive":{"x":1},"ships":[{"id":"s","name":"Lyra","available":true,"captain":{"id":"s","activity":"hyperspace","queue":{"queued":1}},"hull":"steel"}],"admiral":null}"#
    ));
    let fleet = FleetClient::new(address, Secret::new(TOKEN))
        .fleet()
        .unwrap();
    server.join().unwrap();
    assert_eq!(fleet.name, "Future");
    assert_eq!(
        fleet.ship("s").map(|ship| ship.captain.queued("queued")),
        Some(1)
    );
}

#[test]
fn a_page_that_is_not_json_is_an_invalid_response() {
    let (address, server) = answer_once(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html>Sign in</html>",
    );
    let result = FleetClient::new(address, Secret::new(TOKEN)).fleet();
    server.join().unwrap();
    assert_eq!(result.err(), Some(Error::InvalidResponse));
}
