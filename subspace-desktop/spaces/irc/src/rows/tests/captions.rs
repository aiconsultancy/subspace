//! A fleet group's captions, and names that read alike told apart.

use super::super::{fleet_group_key, row_key};
use super::fleets::{aurora, joined};
use crate::fixtures::{channel, network_id};

#[test]
fn a_fleet_group_captions_its_ships_and_the_channels_after_them() {
    let state = joined([
        channel("#aurora-orion"),
        channel("#aurora-nav"),
        channel("#aurora-lyra"),
        channel("#aurora-starbase"),
    ]);
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    let fleet = fleet_group_key(&network_id(), "aurora");
    assert_eq!(
        layout.captions.get(&fleet),
        Some(&vec![
            (row_key(&channel("#aurora-lyra")), "Ships"),
            (row_key(&channel("#aurora-nav")), "Channels"),
        ])
    );
    let plain = joined([channel("#aurora-starbase"), channel("#aurora-nav")]);
    let layout = super::super::layout(&plain, &aurora(), &Default::default());
    assert_eq!(
        layout.captions.get(&fleet),
        None,
        "no ship, no Ships caption"
    );
}

#[test]
fn names_that_differ_only_in_case_still_read_as_their_full_channels() {
    use subspace_framework::dashboard::{Glyph, Row, Tone};
    let row = |title: &str| Row {
        key: title.to_owned().into(),
        icon: Glyph::new("#", Tone::Muted),
        id: None,
        title: title.to_owned().into(),
        trailing: Vec::new(),
        done: false,
        indent: 0,
    };
    let mut rows = vec![row("Lyra"), row("lyra"), row("nav")];
    super::super::groups::unclash(&mut rows, &["#Lyra", "#aurora-lyra", "#aurora-nav"]);
    let titles: Vec<&str> = rows.iter().map(|row| row.title.as_ref()).collect();
    assert_eq!(titles, ["#Lyra", "#aurora-lyra", "nav"]);
}
