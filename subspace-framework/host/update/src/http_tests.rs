//! A local HTTP server stands in for the GitHub API, so `check` and a plain
//! archive install run end to end with no network and no real credential.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use sha2::{Digest, Sha256};

use crate::archive::{plain_asset_name, rust_target};
use crate::test_archive::archive;
use crate::tests::known_product;
use crate::{Release, Updater};

const TOKEN: &str = "fixture-token";
const NEWEST: &str = "9.0.0";
const RUNNING: &str = "1.0.0";
const BINARY: &str = "subspace";
const RELEASE_URL: &str = "https://example.test/release";
const TAG_PREFIX: &str = "subspace-desktop-v";
/// `latest` keeps paging while a page is full.
const FULL_PAGE: usize = 100;

mod check;
mod install;
mod progress;
mod redirect;

/// One canned response, chosen by the first marker found in the request head.
struct Route {
    marker: String,
    body: Vec<u8>,
}

impl Route {
    fn new(marker: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            marker: marker.to_owned(),
            body: body.into(),
        }
    }
}

fn read_request(socket: &mut TcpStream) -> String {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set the fixture read timeout");
    let mut request = Vec::new();
    let mut byte = [0];
    while !request.ends_with(b"\r\n\r\n") {
        socket.read_exact(&mut byte).expect("read the request head");
        request.push(byte[0]);
    }
    String::from_utf8(request).expect("the request head is UTF-8")
}

fn write_body(socket: &mut TcpStream, data: &[u8]) {
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        data.len()
    )
    .expect("write the response head");
    socket.write_all(data).expect("write the response body");
}

/// Answers `rounds` requests from `routes`, asserting that every one carries
/// the fixture token.
fn serve(listener: TcpListener, routes: Vec<Route>, rounds: usize) -> thread::JoinHandle<()> {
    let authorization = format!("authorization: bearer {TOKEN}");
    thread::spawn(move || {
        for _ in 0..rounds {
            let (mut socket, _) = listener.accept().expect("accept a fixture request");
            let request = read_request(&mut socket);
            assert!(
                request.to_lowercase().contains(&authorization),
                "every API request carries the token: {request}"
            );
            let route = routes
                .iter()
                .find(|route| request.contains(&route.marker))
                .unwrap_or_else(|| panic!("no fixture route answers {request}"));
            write_body(&mut socket, &route.body);
        }
    })
}

fn local_api() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind the fixture API");
    let address = listener.local_addr().expect("the fixture address");
    (listener, format!("http://{address}"))
}

fn updater_against(base: &str) -> Updater {
    Updater::for_product(&known_product(), RUNNING)
        .expect("the standalone product has an update source")
        .against(base, TOKEN)
}

/// This host's plain release asset, so the test runs on every supported
/// target rather than only on one.
fn plain_asset() -> (String, bool) {
    let target = rust_target(std::env::consts::OS, std::env::consts::ARCH)
        .expect("this host is a supported release target");
    let zip = cfg!(windows);
    (plain_asset_name(BINARY, NEWEST, target, zip), zip)
}

fn checksum_line(asset: &str, body: &[u8]) -> String {
    format!("{}  {asset}\n", hex::encode(Sha256::digest(body)))
}

fn asset_url(base: &str, index: u32) -> String {
    format!("{base}/assets/{index}")
}

/// An installed executable with this platform's file name.
fn installed_executable(directory: &Path, zip: bool) -> PathBuf {
    let name = if zip {
        format!("{BINARY}.exe")
    } else {
        BINARY.to_owned()
    };
    let executable = directory.join(name);
    std::fs::write(&executable, b"old binary").expect("write the previous executable");
    executable
}

fn release_with_assets(base: &str, asset: &str) -> Release {
    Release {
        tag: format!("{TAG_PREFIX}{NEWEST}"),
        version: NEWEST.to_owned(),
        html_url: RELEASE_URL.to_owned(),
        assets: [
            (asset.to_owned(), asset_url(base, 1)),
            (format!("{asset}.sha256"), asset_url(base, 2)),
        ]
        .into_iter()
        .collect(),
    }
}

fn release_route(base: &str, asset: &str) -> Route {
    Route::new(
        "/releases/tags/subspace-desktop-v9.0.0 ",
        serde_json::json!({
            "tag_name": format!("{TAG_PREFIX}{NEWEST}"),
            "assets": [
                {"name": asset, "url": asset_url(base, 1)},
                {"name": format!("{asset}.sha256"), "url": asset_url(base, 2)},
            ],
        })
        .to_string(),
    )
}

/// Page 1 is full of releases for another tool, so `latest` asks for page 2.
fn first_page() -> String {
    let mut page = vec![serde_json::json!({"tag_name": "another-tool/v1.0.0"}); FULL_PAGE - 1];
    page.push(serde_json::json!({"tag_name": "subspace-desktop-v2.0.0"}));
    serde_json::to_string(&page).expect("the first page serializes")
}

/// Page 2 holds the newest release plus entries the selection must ignore: a
/// higher version of another app, a draft, a prerelease, a prerelease tag and
/// a tag with a leading zero.
fn second_page(base: &str, asset: &str) -> String {
    serde_json::json!([
        {"tag_name": "another-app-v99.0.0"},
        {"tag_name": "subspace-desktop-v99.1.0", "draft": true},
        {"tag_name": "subspace-desktop-v99.2.0", "prerelease": true},
        {"tag_name": "subspace-desktop-v99.3.0-rc1"},
        {"tag_name": "subspace-desktop-v099.4.0"},
        {
            "tag_name": format!("{TAG_PREFIX}{NEWEST}"),
            "html_url": RELEASE_URL,
            "assets": [
                {"name": asset, "url": asset_url(base, 1)},
                {"name": format!("{asset}.sha256"), "url": asset_url(base, 2)},
            ],
        },
        {"tag_name": "subspace-desktop-v3.0.0"},
    ])
    .to_string()
}
