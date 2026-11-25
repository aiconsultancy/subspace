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

mod diff {
    use super::answer_once;
    use crate::diff::{DiffFilePage, DiffMode, DiffStatus, DiffTarget};
    use crate::{Error, FleetClient};
    use subspace_framework::core::Secret;

    pub(super) const TARGET: DiffTarget<'static> = DiffTarget {
        ship: "example-ship",
        member: "example-crew",
        project: Some("example"),
        mode: DiffMode::Working,
    };
    const OK: &str = concat!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
        r#"{"ship":"example-ship","member":"example-crew","project":"example","mode":"working","branch":"example","base":{"ref":"origin/main","commit":"parent"},"head":"head","fingerprint":"snapshot","computedAt":123,"total":2,"additions":1,"deletions":0,"files":[{"path":"notes & plans.txt","previousPath":"old.txt","status":"renamed","additions":1,"deletions":0,"patch":"@@ -0,0 +1,1 @@\n","patchNextPage":"file+1"}],"nextCursor":"list+2"}"#,
    );
    const OK_FILE: &str = concat!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
        r#"{"fingerprint":"snapshot","file":{"path":"notes & plans.txt","previousPath":"old.txt","status":"renamed","additions":1,"deletions":0,"patch":"+example.com\n"}}"#,
    );

    #[test]
    fn file_list_reads_a_page_and_passes_the_opaque_cursor() {
        let (address, server) = answer_once(OK);
        let client = FleetClient::new(address, Secret::new("fixture-token"));
        let page = client.diff(TARGET, Some("list+1")).unwrap().unwrap();
        assert_eq!(page.next_cursor.as_deref(), Some("list+2"));
        assert_eq!(page.files[0].status, DiffStatus::Renamed);
        assert_eq!(page.files[0].patch_next_page.as_deref(), Some("file+1"));
        assert_eq!(
            server.join().unwrap().lines().next(),
            Some(
                "GET /stargate/api/v1/ships/example-ship/crew/example-crew/diff?mode=working&project=example&cursor=list%2B1 HTTP/1.1"
            )
        );
    }

    #[test]
    fn file_read_encodes_paths_and_pins_the_page_to_its_snapshot() {
        let (address, server) = answer_once(OK_FILE);
        let client = FleetClient::new(address, Secret::new("fixture-token"));
        let page = client
            .diff_file(
                TARGET,
                DiffFilePage {
                    path: "notes & plans.txt",
                    page: Some("file+1"),
                    at: "snapshot",
                    side: None,
                },
            )
            .unwrap();
        assert_eq!(page.fingerprint, "snapshot");
        assert_eq!(page.file.patch.as_deref(), Some("+example.com\n"));
        assert!(page.file.patch_next_page.is_none());
        assert_eq!(
            server.join().unwrap().lines().next(),
            Some(
                "GET /stargate/api/v1/ships/example-ship/crew/example-crew/diff/file?mode=working&project=example&path=notes+%26+plans.txt&at=snapshot&page=file%2B1 HTTP/1.1"
            )
        );
    }

    #[test]
    fn an_old_gateway_is_unavailable_but_a_missing_file_is_not() {
        let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (address, server) = answer_once(not_found);
        let client = FleetClient::new(address, Secret::new("fixture-token"));
        assert!(client.diff(TARGET, None).unwrap().is_none());
        server.join().unwrap();
        let (address, server) = answer_once(not_found);
        let client = FleetClient::new(address, Secret::new("fixture-token"));
        assert_eq!(
            client
                .diff_file(
                    TARGET,
                    DiffFilePage {
                        path: "gone.txt",
                        page: None,
                        at: "snapshot",
                        side: None,
                    }
                )
                .err(),
            Some(Error::NotFound)
        );
        server.join().unwrap();
    }

    #[test]
    fn stale_file_pages_keep_their_conflict_status() {
        let (address, server) =
            answer_once("HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let client = FleetClient::new(address, Secret::new("fixture-token"));
        assert_eq!(
            client
                .diff_file(
                    TARGET,
                    DiffFilePage {
                        path: "notes.txt",
                        page: Some("file+1"),
                        at: "old",
                        side: None,
                    }
                )
                .err(),
            Some(Error::StaleDiff)
        );
        server.join().unwrap();
    }

    #[test]
    fn cursor_conflicts_and_missing_cursor_pages_are_stale() {
        for status in [
            "HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ] {
            let (address, server) = answer_once(status);
            let client = FleetClient::new(address, Secret::new("fixture-token"));
            assert_eq!(
                client.diff(TARGET, Some("more")).err(),
                Some(Error::StaleDiff)
            );
            assert!(
                server
                    .join()
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .contains("&cursor=more ")
            );
        }
    }
}

mod diff_content;

#[test]
fn a_conflict_on_the_fleet_overview_is_an_unexpected_status() {
    let (address, server) =
        answer_once("HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let client = FleetClient::new(address, Secret::new("fixture-token"));
    assert_eq!(client.fleet().err(), Some(Error::UnexpectedStatus(409)));
    server.join().unwrap();
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
