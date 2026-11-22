//! The fleet channel inspector's pure rules: its tabs, the reason a fleet
//! is not live for every state, and the words of the card, the crew and
//! Here now.

use chrono::{DateTime, Utc};
use subspace_client_fleet::{FleetState, Link};
use subspace_framework::contracts::github::PullRequestStatus;
use subspace_framework::contracts::issues::{IssueState, StateType};

use super::play::{Mark, PlayLine};
use super::reason::{SETTINGS, WAITING, reason};
use super::ship::{age, figures, here_line, subtitle, work};
use super::{InspectorTab, Subject, tabs};
use crate::fleet::ChannelKind;
use crate::live::tests::{NOW, lyra};

#[test]
fn a_ship_and_a_starbase_have_their_subject_then_people_and_command_has_no_tabs() {
    let ship = Subject::of(&ChannelKind::Ship {
        captain: "aurora-lyra".into(),
    })
    .unwrap();
    let labels = |tabs: Vec<subspace_framework::dashboard::workspace::InspectorTab>| {
        tabs.iter()
            .map(|tab| tab.accessible_label().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(tabs(&ship, Some(5))), ["Ship", "People, 5"]);
    let starbase = Subject::of(&ChannelKind::Starbase).unwrap();
    assert_eq!(labels(tabs(&starbase, None)), ["Starbase", "People"]);
    assert_eq!(Subject::of(&ChannelKind::Command), None);
    assert!(tabs(&ship, None).len() < subspace_framework::dashboard::workspace::INSPECTOR_MAX_TABS);
    assert_eq!(InspectorTab::at(1), InspectorTab::People);
    assert_eq!(
        InspectorTab::at(9),
        InspectorTab::Subject,
        "room for Thread later"
    );
    assert_eq!(InspectorTab::People.index(), 1);
}

#[test]
fn every_state_that_is_not_live_has_its_reason_in_the_client_s_words() {
    let states = [
        FleetState::NotConfigured,
        FleetState::InvalidAddress,
        FleetState::Checking,
        FleetState::NoToken,
        FleetState::AddressChanged { saved_for: None },
        FleetState::StoreUnavailable,
        FleetState::Ready,
        FleetState::Followed(Link::Connecting),
        FleetState::Followed(Link::Reconnecting),
        FleetState::Followed(Link::TokenRefused),
        FleetState::Followed(Link::Forbidden),
        FleetState::Followed(Link::Unreachable),
    ];
    for state in states {
        let (title, sentence) = reason(&state).unwrap();
        assert_eq!(title, state.label(), "{state:?}");
        if let Some(help) = state.help() {
            assert!(sentence.starts_with(help), "{state:?}: {sentence}");
        }
        let points = matches!(
            state,
            FleetState::NoToken | FleetState::AddressChanged { .. }
        );
        assert_eq!(sentence.ends_with(SETTINGS), points, "{state:?}");
    }
    assert_eq!(reason(&FleetState::Ready).unwrap().1, WAITING);
    assert_eq!(
        reason(&FleetState::Checking).unwrap().1,
        "Reading the fleet's saved token."
    );
    assert_eq!(reason(&FleetState::Followed(Link::Live)), None);
    assert_eq!(reason(&FleetState::Followed(Link::Polling)), None);
}

#[test]
fn the_card_shows_only_the_figures_the_api_gives() {
    let ship = lyra();
    assert_eq!(subtitle(&ship), "captain · claude-opus-5-5");
    assert_eq!(
        figures(&ship),
        [
            ("Crew busy", "3 of 4".to_owned()),
            ("Queued", "2".to_owned()),
            ("Done", "128".to_owned())
        ]
    );
    let bare = crate::live::ShipLive {
        model: None,
        crew: None,
        queued: None,
        done: None,
        ..ship
    };
    assert_eq!(subtitle(&bare), "captain");
    assert!(figures(&bare).is_empty(), "never a sample number");
}

#[test]
fn a_crew_line_says_what_each_is_on_and_since_when() {
    let ship = lyra();
    assert_eq!(
        work(&ship.members[0]),
        "Reviewing diff · Cancel tile decodes that leave the viewport (NAV-7)"
    );
    assert_eq!(work(&ship.members[2]), "Course correction preview (NAV-2)");
    assert_eq!(work(&ship.members[3]), "idle");
    let now = DateTime::<Utc>::from_timestamp_millis(NOW).unwrap();
    assert_eq!(age(NOW - 35 * 60_000, now), "35m");
    assert_eq!(age(NOW - 180 * 60_000, now), "3h");
}

#[test]
fn here_now_names_four_then_counts_the_rest() {
    let names = |count: usize| (0..count).map(|n| format!("p{n}")).collect::<Vec<_>>();
    assert_eq!(here_line(&names(2)), "p0, p1");
    assert_eq!(here_line(&names(5)), "p0, p1, p2, p3 and 1 other");
    assert_eq!(here_line(&names(7)), "p0, p1, p2, p3 and 3 others");
}

/// A reference in play, untitled.
fn in_play(key: &str, mark: Mark) -> PlayLine {
    PlayLine {
        key: key.into(),
        title: None,
        mark,
    }
}

#[test]
fn a_reference_in_play_says_its_state_once_answered() {
    let review = IssueState::new("In Review", StateType::Started);
    assert_eq!(
        in_play("NAV-7", Mark::Issue(Some(review)))
            .state()
            .as_deref(),
        Some("In Review")
    );
    assert_eq!(
        in_play("acme/web#492", Mark::PullRequest(None)).state(),
        None
    );
    let merged = Mark::PullRequest(Some(PullRequestStatus::Merged));
    assert_eq!(
        in_play("acme/web#492", merged).state().as_deref(),
        Some("Merged")
    );
}
