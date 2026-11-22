//! Whom `/ask` addresses in a Starbase: one rear admiral only when its
//! fleet names exactly one.

use super::super::Roster;
use super::{agent, fleet};

#[test]
fn a_starbase_whose_fleet_names_two_rear_admirals_asks_neither_even_when_one_clashes() {
    let one = fleet(
        "aurora",
        vec![agent("vance", "rearAdmiral", Some("aurora-admiral"), None)],
    );
    let roster = Roster::new([&one]);
    assert_eq!(roster.asked_in("#aurora-starbase"), Some("aurora-admiral"));
    let two = fleet(
        "aurora",
        vec![
            agent("vance", "rearAdmiral", Some("aurora-admiral"), None),
            agent("reed", "rearAdmiral", Some("shared-nick"), None),
        ],
    );
    // Another fleet's agent claims the second admiral's nick, so the
    // roster drops it: the Starbase still has two and asks neither.
    let other = fleet(
        "borealis",
        vec![agent(
            "shared",
            "captain",
            Some("shared-nick"),
            Some("Pike"),
        )],
    );
    let roster = Roster::new([&two, &other]);
    assert_eq!(roster.rank("shared-nick"), None, "the clash is unknown");
    assert_eq!(roster.asked_in("#aurora-starbase"), None);
}
