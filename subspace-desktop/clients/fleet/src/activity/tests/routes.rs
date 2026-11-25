//! The HTTP routes: the snapshot and a paged item read in full, never
//! cached, and the answers that mean a missing route or a bad query.

use super::*;

/// A loopback server that answers each request with `answer(target)` and
/// records each request's head.
fn serve(
    count: usize,
    answer: impl Fn(&str) -> String + Send + 'static,
) -> (FleetAddress, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let heads = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&heads);
    std::thread::spawn(move || {
        for _ in 0..count {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let target = head.split(' ').nth(1).unwrap_or_default().to_owned();
            seen.lock().unwrap().push(head);
            let _ = stream.write_all(answer(&target).as_bytes());
        }
    });
    let address = FleetAddress::parse(&format!("http://127.0.0.1:{port}")).unwrap();
    (address, heads)
}

fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

#[test]
fn the_routes_read_the_snapshot_and_a_paged_item_in_full() {
    let item = text("i3", 6, &"x".repeat(100)).to_string();
    let (a, b) = item.split_at(60);
    let pages = [a.to_owned(), b.to_owned()];
    let size = item.len();
    let (address, heads) = serve(4, move |target| {
        if !target.contains("item=") {
            return ok(&snapshot_json("g1", 9).to_string());
        }
        if target.contains("page=9") {
            return NOT_FOUND.to_owned();
        }
        let number: usize = usize::from(target.ends_with("page=1"));
        let body = json!({"agent": AGENT, "turn": "t-now", "item": "i3", "seq": 6,
                          "page": number, "pages": 2, "bytes": size, "data": pages[number]});
        ok(&body.to_string())
    });
    let client = FleetClient::new(address, Secret::new("example-token"));
    let activity = client.activity(AGENT, None).unwrap();
    assert_eq!(activity.turns.len(), 2);
    let ActivityItem::Text(whole) = client
        .activity_item(AGENT, "t-now", "i3", &AtomicBool::new(false))
        .unwrap()
        .unwrap()
    else {
        panic!("the text item");
    };
    assert_eq!(whole.text.len(), 100);
    assert_eq!(
        client.activity_item_page(AGENT, "t-now", "i3", 9).err(),
        Some(Error::NotFound)
    );
    let heads = heads.lock().unwrap();
    assert!(heads[0].starts_with("GET /api/v1/agents/example-lyra/activity HTTP/1.1"));
    assert!(heads[1].starts_with(
        "GET /api/v1/agents/example-lyra/activity?turn=t-now&item=i3&page=0 HTTP/1.1"
    ));
    for head in heads.iter() {
        assert!(
            head.lines()
                .any(|line| line.eq_ignore_ascii_case("cache-control: no-store"))
        );
    }
}

#[test]
fn a_missing_route_is_not_found_and_a_bad_query_is_an_error() {
    let (address, _) = serve(3, |target| {
        if target.contains("turn=") {
            "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
        } else if target.contains("other") {
            ok(&snapshot_json("g1", 9).to_string())
        } else {
            NOT_FOUND.into()
        }
    });
    let client = FleetClient::new(address, Secret::new("example-token"));
    assert_eq!(client.activity(AGENT, None).err(), Some(Error::NotFound));
    assert_eq!(
        client.activity(AGENT, Some("t-gone")).err(),
        Some(Error::UnexpectedStatus(400))
    );
    // An answer about another officer is not this one's.
    assert_eq!(
        client.activity("example-other", None).err(),
        Some(Error::InvalidResponse)
    );
}
