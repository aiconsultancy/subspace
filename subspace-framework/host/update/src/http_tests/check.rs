//! What a check reports for each answer: a newer release, up to date, no
//! release yet, and a failed request, which stays an error.

use std::{io::Write, thread};

use crate::CheckOutcome;
use crate::tests::known_product;

use super::{Route, local_api, read_request, serve, updater_against};

fn repository() -> String {
    known_product()
        .update
        .expect("the standalone product has an update source")
        .repository
}

#[test]
fn a_repository_without_releases_is_no_release_for_check_and_an_error_for_upgrade() {
    let (listener, base) = local_api();
    let routes = vec![Route::new("page=1 ", "[]")];
    let server = serve(listener, routes, 2);
    let updater = updater_against(&base);
    let outcome = updater.check().expect("an empty list is not a failure");
    assert_eq!(
        outcome,
        CheckOutcome::NoRelease {
            repository: repository()
        }
    );
    let error = updater.latest().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("has no stable release tagged subspace-desktop-vX.Y.Z"),
        "{error}"
    );
    server.join().expect("the fixture server finishes");
}

#[test]
fn an_older_or_equal_newest_release_is_up_to_date() {
    let (listener, base) = local_api();
    let page = serde_json::json!([{"tag_name": "subspace-desktop-v1.0.0"}]).to_string();
    let server = serve(listener, vec![Route::new("page=1 ", page)], 1);
    let outcome = updater_against(&base)
        .check()
        .expect("the check reaches the fixture API");
    assert_eq!(outcome, CheckOutcome::UpToDate);
    server.join().expect("the fixture server finishes");
}

#[test]
fn a_404_from_github_stays_an_error_rather_than_no_release() {
    let (listener, base) = local_api();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept a fixture request");
        read_request(&mut socket);
        write!(
            socket,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .expect("write the 404");
    });
    let error = updater_against(&base).check().unwrap_err();
    assert!(error.to_string().contains("GitHub returned 404"), "{error}");
    assert!(error.to_string().contains(&repository()), "{error}");
    server.join().expect("the fixture server finishes");
}
