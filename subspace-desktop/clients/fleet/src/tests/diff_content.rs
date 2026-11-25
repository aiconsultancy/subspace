use subspace_framework::core::Secret;

use crate::FleetClient;
use crate::diff::{DiffFilePage, DiffSide, V1DiffFilePage};

use super::{answer_once, diff::TARGET};

const OLD: &str = concat!(
    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
    r#"{"fingerprint":"snapshot","file":{"path":"icon.png","status":"removed","binary":true,"size":3},"content":{"side":"old","encoding":"base64","offset":0,"size":3,"data":"AAA=","nextPage":"old-next"}}"#,
);
const OLD_NEXT: &str = concat!(
    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
    r#"{"fingerprint":"snapshot","file":{"path":"icon.png","status":"removed","binary":true,"size":3},"content":{"side":"old","encoding":"base64","offset":2,"size":3,"data":"/w=="}}"#,
);
const FIRST: &str = concat!(
    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
    r#"{"fingerprint":"snapshot","file":{"path":"asset.bin","status":"added","binary":true},"content":{"side":"new","encoding":"base64","offset":0,"size":4,"data":"AEE=","nextPage":"more+1"}}"#,
);
const NEXT: &str = concat!(
    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
    r#"{"fingerprint":"snapshot","unstable":true,"file":{"path":"asset.bin","status":"added","binary":true},"content":{"side":"new","encoding":"base64","offset":2,"size":4,"data":"/0I="}}"#,
);

fn read(reply: &'static str, request: DiffFilePage<'_>) -> (V1DiffFilePage, String) {
    let (address, server) = answer_once(reply);
    let client = FleetClient::new(address, Secret::new("fixture-token"));
    let page = client.diff_file(TARGET, request).unwrap();
    let line = server.join().unwrap().lines().next().unwrap().to_owned();
    (page, line)
}

#[test]
fn reads_the_old_binary_side_of_a_removal() {
    let mut request = DiffFilePage {
        path: "icon.png",
        at: "snapshot",
        page: None,
        side: Some(DiffSide::Old),
    };
    let (page, line) = read(OLD, request);
    assert!(page.file.binary);
    assert_eq!(page.file.size, Some(3));
    let content = page.content.unwrap();
    assert_eq!(content.side, DiffSide::Old);
    assert_eq!(content.bytes, [0, 0]);
    assert!(line.contains("&at=snapshot&side=old "));
    request.page = content.next_page.as_deref();
    let (next, line) = read(OLD_NEXT, request);
    assert!(line.contains("&at=snapshot&page=old-next&side=old "));
    let next = next.content.unwrap();
    assert_eq!(next.offset, 2);
    assert_eq!(next.bytes, [255]);
    assert!(next.next_page.is_none());
}

#[test]
fn reads_a_binary_addition_through_its_pinned_continuation() {
    let mut request = DiffFilePage {
        path: "asset.bin",
        at: "snapshot",
        page: None,
        side: Some(DiffSide::New),
    };
    let (first, line) = read(FIRST, request);
    assert!(line.contains("&at=snapshot&side=new "));
    assert!(!first.unstable);
    let first = first.content.unwrap();
    assert_eq!(first.bytes, [0, b'A']);
    request.page = first.next_page.as_deref();
    let (next, line) = read(NEXT, request);
    assert!(line.contains("&at=snapshot&page=more%2B1&side=new "));
    assert!(next.unstable);
    let next = next.content.unwrap();
    assert_eq!(next.offset, 2);
    assert_eq!(next.bytes, [255, b'B']);
    assert!(next.next_page.is_none());
}
