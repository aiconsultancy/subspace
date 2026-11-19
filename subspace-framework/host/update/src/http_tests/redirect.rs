use std::{io::Write, thread};

use crate::github::Payload;

use super::{asset_url, local_api, read_request, updater_against, write_body};

#[test]
fn a_redirect_drops_the_authorization_header_even_on_the_same_host() {
    let (listener, base) = local_api();
    let redirect_to = base.clone();
    let server = thread::spawn(move || {
        for redirected in [false, true] {
            let (mut socket, _) = listener.accept().expect("accept a fixture request");
            let request = read_request(&mut socket).to_lowercase();
            assert_eq!(
                request.contains("authorization:"),
                !redirected,
                "the redirected request must not carry the token: {request}"
            );
            if redirected {
                write_body(&mut socket, b"ok");
            } else {
                write!(
                    socket,
                    "HTTP/1.1 302 Found\r\nLocation: {redirect_to}/storage\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .expect("write the redirect");
            }
        }
    });

    let updater = updater_against(&base);
    let client = updater.client().expect("a fixed token needs no lookup");
    let bytes = client
        .get(&asset_url(&base, 1), Payload::Asset)
        .expect("the redirect is followed");
    assert_eq!(bytes, b"ok");
    server.join().expect("the fixture server finishes");
}
