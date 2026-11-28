//! The fleet channel inspector's pure rules: its tabs, the reason a fleet
//! is not live for every state, and the words of the card, the crew and
//! Here now.

use chrono::{DateTime, Utc};
use subspace_client_fleet::{FleetState, Link};
use subspace_framework::contracts::github::PullRequestStatus;
use subspace_framework::contracts::issues::{IssueState, StateType};

use super::play::{Answer, Mark, PlayLine};
use super::reason::{SETTINGS, WAITING, reason};
use super::ship::{age, figures, here_line, subtitle, work};
use super::{InspectorTab, Subject, tabs};
use crate::fleet::ChannelKind;
use crate::live::tests::{NOW, lyra};

#[test]
fn all_fleet_channels_have_their_subject_thread_and_people() {
    let ship = Subject::of(&ChannelKind::Ship {
        captain: "aurora-lyra".into(),
    });
    let labels = |tabs: Vec<subspace_framework::dashboard::workspace::InspectorTab>| {
        tabs.iter()
            .map(|tab| tab.accessible_label().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(tabs(&ship, Some(5))),
        ["Ship", "Thread", "People, 5"]
    );
    let starbase = Subject::of(&ChannelKind::Starbase);
    assert_eq!(
        labels(tabs(&starbase, None)),
        ["Starbase", "Thread", "People"]
    );
    for (kind, label) in [
        (ChannelKind::Command, "Command"),
        (ChannelKind::Audit, "Fleet audit"),
    ] {
        assert_eq!(
            labels(tabs(&Subject::of(&kind), Some(7))),
            [label, "Thread", "People, 7"]
        );
    }
    assert_eq!(
        tabs(&ship, None).len(),
        subspace_framework::dashboard::workspace::INSPECTOR_MAX_TABS,
        "three, the most the shell draws"
    );
    assert_eq!(InspectorTab::at(1), InspectorTab::Thread);
    assert_eq!(InspectorTab::at(2), InspectorTab::People);
    assert_eq!(InspectorTab::at(9), InspectorTab::Subject);
    assert_eq!(InspectorTab::People.index(), 2);
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

/// A reference in play, untitled, answered when its mark has a state.
fn in_play(key: &str, mark: Mark) -> PlayLine {
    let known = match &mark {
        Mark::Issue(state) => state.is_some(),
        Mark::PullRequest(status) => status.is_some(),
    };
    PlayLine {
        key: key.into(),
        title: None,
        mark,
        answer: if known {
            Answer::Known
        } else {
            Answer::Loading
        },
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
        in_play("acme/web#492", Mark::PullRequest(None))
            .state()
            .as_deref(),
        Some("Loading"),
        "never a bare key"
    );
    let unavailable = PlayLine {
        answer: Answer::Unavailable,
        ..in_play("NAV-6", Mark::Issue(None))
    };
    assert_eq!(unavailable.state().as_deref(), Some("Unavailable"));
    let merged = Mark::PullRequest(Some(PullRequestStatus::Merged));
    assert_eq!(
        in_play("acme/web#492", merged).state().as_deref(),
        Some("Merged")
    );
}

#[test]
fn a_live_fleet_with_no_current_overview_never_says_not_listed() {
    use super::reason::{FIRST, KEPT, NOT_LISTED, unshown};
    use crate::live::FleetLive;
    let live = FleetState::Followed(Link::Live);
    let first = FleetLive::new(live.clone(), None);
    assert_eq!(unshown(Some(&first)), FIRST, "before the first overview");
    let mut kept = crate::live::tests::snapshot();
    kept.overview.as_mut().unwrap().overview_stale = true;
    let kept = FleetLive::new(live.clone(), Some(&kept));
    assert_eq!(unshown(Some(&kept)), KEPT, "data kept, not current");
    let current = FleetLive::new(live, Some(&crate::live::tests::snapshot()));
    assert!(current.ship("aurora-ghost").is_none());
    assert_eq!(
        unshown(Some(&current)),
        NOT_LISTED,
        "a current overview without it"
    );
    assert_eq!(unshown(None), FIRST);
}

#[test]
fn a_kept_crew_detail_says_so_rather_than_not_arrived() {
    use super::ship::crew_note;
    let mut kept = crate::live::tests::snapshot();
    kept.ships.get_mut("aurora-lyra").unwrap().detail_stale = true;
    let lyra_kept = crate::live::ship::of(&kept, "aurora-lyra").unwrap();
    assert!(lyra_kept.members.is_empty());
    assert_eq!(
        crew_note(&lyra_kept),
        "The crew's detail is kept from an earlier read and not current, so it is not shown."
    );
    let mut pending = crate::live::tests::snapshot();
    pending.ships.remove("aurora-lyra");
    let lyra_pending = crate::live::ship::of(&pending, "aurora-lyra").unwrap();
    assert_eq!(
        crew_note(&lyra_pending),
        "The crew's detail has not arrived yet."
    );
    let none = crate::live::ShipLive {
        crew: None,
        members: Vec::new(),
        ..lyra()
    };
    assert_eq!(crew_note(&none), "This ship has no crew.");
}

#[test]
fn an_unanswered_reference_says_loading_and_a_refused_one_unavailable() {
    use super::play::standing;
    let none: Option<&()> = None;
    assert_eq!(
        standing(none, false, false),
        Answer::Loading,
        "not asked yet"
    );
    assert_eq!(standing(Some(&()), false, true), Answer::Loading, "pending");
    assert_eq!(standing(Some(&()), false, false), Answer::Unavailable);
    assert_eq!(standing(Some(&()), true, false), Answer::Known);
}

#[test]
fn a_follow_up_request_s_tab_says_its_replies_are_the_thread_s() {
    use super::thread::replies_heading;
    let mut thread = crate::thread::Thread {
        root: crate::state::Entry {
            seq: 0,
            line: crate::fixtures::message(&crate::fixtures::channel("#a"), "mira", "Hi.", 1),
            highlight: false,
        },
        root_id: "root".into(),
        timeline: None,
        replies: Vec::new(),
        reply_count: 0,
        nested: false,
    };
    assert_eq!(replies_heading(&thread), "Replies");
    thread.nested = true;
    assert_eq!(replies_heading(&thread), "Replies in this thread");
}
