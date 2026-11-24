use super::{CrewMember, Error, Roster, ShipDetail, shown};

#[test]
fn missing_route_uses_the_roster_but_member_removal_wins_over_any_read() {
    let mut ship: ShipDetail = serde_json::from_str(
        r#"{"id":"lyra","available":true,"crewMembers":[{"id":"wren","name":"Wren"}]}"#,
    )
    .unwrap();
    let member = ship.crew_members[0].clone();
    let (shown_member, note) = shown(
        Roster::find(Some(&ship), "wren"),
        false,
        Some(&Err(Error::NotFound)),
    );
    assert_eq!(shown_member, Some(&member));
    assert!(note.unwrap().contains("gateway"));

    ship.detail_stale = true;
    let read = Ok(member.clone());
    let (shown_member, note) = shown(Roster::find(Some(&ship), "wren"), false, Some(&read));
    assert!(shown_member.is_none());
    assert!(note.unwrap().contains("unavailable"));

    ship.detail_stale = false;
    ship.crew_members.clear();
    let (shown_member, note) = shown(Roster::find(Some(&ship), "wren"), false, Some(&read));
    assert!(shown_member.is_none());
    assert!(note.unwrap().contains("no longer"));
}

#[test]
fn the_one_shot_read_yields_to_new_live_data() {
    fn member(model: &str) -> CrewMember {
        serde_json::from_value(serde_json::json!({"id":"wren", "model": model})).unwrap()
    }
    let baseline = member("first");
    let endpoint = member("second");
    let latest = member("third");
    let read = Ok(endpoint.clone());
    let select = |member, follow| shown(Roster::Present(member), follow, Some(&read)).0;
    assert_eq!(select(&baseline, false), Some(&endpoint));
    assert_eq!(select(&latest, true), Some(&latest));
}
