use crate::{Client, tests::row};
use serde_json::json;
use subspace_framework::client_test_support::Mock;

#[test]
fn tracker_pages_are_used_without_ancestry_reads_or_text_refiltering() {
    let mock = Mock::new(|request| {
        assert!(request.path.starts_with("/api/v1/issues?"));
        let next = request.path.contains("cursor=fixture-next");
        let mut tracker = row(if next { "6" } else { "4" });
        tracker["type"] = json!("tracker");
        tracker["title"] = json!("Another topic");
        tracker["description"] = json!("navigation");
        let mut excluded = row("1");
        excluded["type"] = json!("tracker");
        (
            200,
            json!({"items":[excluded,tracker],"nextCursor":(!next).then_some("fixture-next")})
                .to_string(),
        )
    });
    let client = Client::loopback(&format!("{}/api", mock.base_url()), None).unwrap();
    let page = super::tracker_parents(&client, " navigation ", None, Some("SUB-1")).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].fields.identifier, "SUB-4");
    let next =
        super::tracker_parents(&client, " navigation ", page.next_cursor, Some("SUB-1")).unwrap();
    assert_eq!(next.items.len(), 1);
    assert_eq!(next.items[0].fields.identifier, "SUB-6");
    assert!(next.next_cursor.is_none());
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.path.contains("q=navigation")
        && r.path.contains("type=tracker")
        && r.path.contains("parentFor=SUB-1")));
    assert!(requests[1].path.contains("cursor=fixture-next"));
    let by_id = super::tracker_parents(&client, " navigation ", None, Some("1")).unwrap();
    assert_eq!(by_id.items.len(), 1);
    assert_eq!(by_id.items[0].fields.id, "4");
}
