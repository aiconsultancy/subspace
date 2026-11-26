use std::time::{Duration, Instant};

use chrono::NaiveDate;
use subspace_api_client::model::{AgentRank, DecisionStatus};

use super::{
    decided_short, decisions, empty, maker_full, maker_kind, maker_short, rank, row_label, status,
    supersession, updated,
};
use crate::fixtures;
use crate::model::{Choice, View};

fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[test]
fn a_maker_is_a_person_s_name_or_an_agent_s_name_and_rank() {
    let people = fixtures::people();
    let agent = fixtures::item("d4");
    assert_eq!(
        maker_short(&agent, &people),
        "aurora-admiral · rear admiral"
    );
    assert_eq!(
        maker_full(&agent, &people),
        "aurora-admiral · rear admiral · fixture-model (agent)"
    );
    assert_eq!(maker_kind(&agent), "Agent");
    let person = fixtures::item("d3");
    assert_eq!(maker_short(&person, &people), "local-operator");
    assert_eq!(maker_full(&person, &people), "local-operator");
    assert_eq!(maker_kind(&person), "Person");
    assert_eq!(maker_short(&person, &Default::default()), "A person");
}

#[test]
fn ranks_and_statuses_read_as_words() {
    assert_eq!(rank(Some(AgentRank::RearAdmiral)), Some("rear admiral"));
    assert_eq!(rank(Some(AgentRank::Crew)), Some("crew"));
    assert_eq!(rank(None), None);
    assert_eq!(status(DecisionStatus::Superseded), "Superseded");
    assert_eq!(status(DecisionStatus::Other), "Unknown status");
}

#[test]
fn a_row_shows_the_year_only_when_it_is_not_this_one() {
    let item = fixtures::item("d2");
    assert_eq!(decided_short(&item, day(2026, 11, 20)), "19 Nov");
    assert_eq!(decided_short(&item, day(2027, 1, 2)), "19 Nov 2026");
}

#[test]
fn a_row_marks_both_directions_of_supersession() {
    assert_eq!(
        supersession(&fixtures::item("d2")).as_deref(),
        Some("supersedes")
    );
    assert_eq!(
        supersession(&fixtures::item("d6")).as_deref(),
        Some("superseded")
    );
    assert_eq!(supersession(&fixtures::item("d3")), None);
}

#[test]
fn a_row_is_named_with_every_fact_its_lanes_show() {
    assert_eq!(
        row_label(&fixtures::item("d2"), &fixtures::people()),
        "Read the relay position after the correction is applied, Accepted, D-00002-NAV, \
         made by aurora-lyra · captain, decided 19 November 2026, supersedes, \
         2 linked issues"
    );
}

#[test]
fn counts_say_when_there_are_more_than_were_read() {
    assert_eq!(decisions(1, false), "1 decision");
    assert_eq!(decisions(8, false), "8 decisions");
    assert_eq!(decisions(1000, true), "1000+ decisions");
}

#[test]
fn the_empty_list_says_what_left_it_empty() {
    assert_eq!(
        empty(&Choice::default(), "relay").0,
        "No decisions match your search"
    );
    let proposed = Choice {
        view: View::Proposed,
        ..Choice::default()
    };
    assert_eq!(empty(&proposed, "").0, "No decisions match these choices");
    assert_eq!(empty(&Choice::default(), "").0, "No decisions yet");
}

#[test]
fn the_read_time_is_said_in_minutes_then_hours() {
    let read_at = Instant::now();
    assert_eq!(updated(read_at, read_at), "updated just now");
    assert_eq!(
        updated(read_at, read_at + Duration::from_secs(120)),
        "updated 2 min ago"
    );
    assert_eq!(
        updated(read_at, read_at + Duration::from_secs(7200)),
        "updated 2 h ago"
    );
}
