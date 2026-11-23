//! Parsing, completion and checks of the tracker commands. Fictional names
//! and issues only.

use std::collections::HashMap;

use subspace_client_irc::COMMANDS;
use subspace_framework::contracts::issues::IssueScope;

use super::parse::{Name, Typed, typed};
use super::plan::{AgentChoice, Answer, Facts, Known, Plan, Who, plan};
use crate::compose::suggest::{Offer, suggest};
use crate::markup::IssueKeys;

fn lyra() -> AgentChoice {
    AgentChoice {
        nick: "aurora-lyra".into(),
        user_id: "user-lyra".into(),
    }
}

fn orion() -> AgentChoice {
    AgentChoice {
        nick: "aurora-orion".into(),
        user_id: "user-orion".into(),
    }
}

fn known(title: &str) -> Known {
    Known {
        title: title.into(),
        ..Known::default()
    }
}

/// `#aurora-lyra`: NAV-2 and NAV-7 in play, NAV-99 missing.
fn facts() -> Facts {
    let scope = IssueScope::new("subspace-api", "Subspace API", "http://127.0.0.1:9")
        .with_project_keys(["NAV", "MAP"]);
    let mut answers = HashMap::new();
    answers.insert(
        "NAV-2".to_owned(),
        Answer::Known(known("Course correction preview")),
    );
    let mut assigned = known("Cancel tile decodes");
    assigned.assignee = Some("Wren".into());
    assigned.assignee_id = Some("user-wren".into());
    answers.insert("NAV-7".to_owned(), Answer::Known(assigned));
    answers.insert("NAV-99".to_owned(), Answer::Unavailable);
    Facts {
        keys: IssueKeys::new(&scope),
        projects: vec!["NAV".into(), "MAP".into()],
        in_play: vec!["NAV-7".into(), "NAV-2".into()],
        answers,
        captain: Some(lyra()),
        ship: Some("Lyra".into()),
        agents: vec![lyra(), orion()],
        tracker: Some(("subspace-api".into(), "http://127.0.0.1:9".into())),
    }
}

fn planned(text: &str) -> Result<Plan, String> {
    plan(&typed(text).unwrap(), &facts())
}

#[test]
fn the_commands_name_no_irc_client_command_and_parse_in_any_case() {
    for name in Name::ALL {
        assert!(COMMANDS.iter().all(|command| !command.answers(name.word())));
    }
    assert_eq!(
        typed("/ASSIGN nav-2   to aurora-lyra "),
        Some(Typed {
            name: Name::Assign,
            issue: Some("NAV-2".into()),
            rest: "to aurora-lyra".into(),
        })
    );
    assert_eq!(typed("/assigned NAV-2"), None);
    assert_eq!(typed("//assign NAV-2"), None, "a literal slash");
    assert_eq!(typed("assign NAV-2"), None);
    assert_eq!(typed("/ready").map(|typed| typed.issue), Some(None));
}

#[test]
fn assign_defaults_to_the_ship_s_captain_and_takes_an_agent_or_me() {
    assert_eq!(
        planned("/assign NAV-2"),
        Ok(Plan::Assign {
            issue: "NAV-2".into(),
            to: Who::Agent(lyra()),
        })
    );
    assert_eq!(
        planned("/assign NAV-2 to @Aurora-Orion"),
        Ok(Plan::Assign {
            issue: "NAV-2".into(),
            to: Who::Agent(orion()),
        })
    );
    assert_eq!(
        planned("/assign NAV-2 to me"),
        Ok(Plan::Assign {
            issue: "NAV-2".into(),
            to: Who::Me,
        })
    );
    let mut starbase = facts();
    starbase.captain = None;
    assert!(
        plan(&typed("/assign NAV-2").unwrap(), &starbase)
            .unwrap_err()
            .starts_with("Say whom")
    );
}

#[test]
fn checks_refuse_before_anything_is_written() {
    let refused = |text: &str| planned(text).unwrap_err();
    assert!(refused("/assign").starts_with("Name an issue"));
    assert!(refused("/assign SUB-1").contains("isn't an issue in this tracker"));
    assert!(refused("/assign NAV-99").contains("isn't in the tracker"));
    assert!(refused("/assign NAV-2 to mira").contains("isn't one of this fleet's agents"));
    assert!(refused("/assign NAV-2 for aurora-lyra").starts_with("Use /assign"));
    assert!(refused("/ready NAV-7").contains("assigned to Wren"));
    assert!(refused("/ready NAV-2 maybe").starts_with("Use /ready"));
    assert!(refused("/decision NAV-2").starts_with("Write the decision"));
    let mut lyra_has_it = facts();
    if let Some(Answer::Known(known)) = lyra_has_it.answers.get_mut("NAV-2") {
        known.assignee_id = Some("user-lyra".into());
    }
    let error = plan(&typed("/assign NAV-2").unwrap(), &lyra_has_it).unwrap_err();
    assert_eq!(error, "NAV-2 is already assigned to aurora-lyra.");
}

#[test]
fn ready_and_decision_plan_what_they_say() {
    let mut unknown = facts();
    unknown.answers.remove("NAV-2");
    assert_eq!(
        plan(&typed("/ready NAV-2 off").unwrap(), &unknown),
        Ok(Plan::Ready {
            issue: "NAV-2".into(),
            on: false,
        })
    );
    assert_eq!(
        planned("/decision NAV-2 Decode tiles on a worker pool."),
        Ok(Plan::Decision {
            issue: "NAV-2".into(),
            text: "Decode tiles on a worker pool.".into(),
        })
    );
    let mut tracker = facts();
    if let Some(Answer::Known(known)) = tracker.answers.get_mut("NAV-2") {
        known.tracker = true;
    }
    assert!(
        plan(&typed("/ready NAV-2").unwrap(), &tracker)
            .unwrap_err()
            .contains("is a tracker")
    );
}

fn labels(offer: Option<Offer>) -> Vec<String> {
    offer
        .map(|offer| {
            offer
                .choices
                .into_iter()
                .map(|choice| choice.item.label().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn completion() -> crate::compose::suggest::Facts {
    crate::compose::suggest::Facts::default().tracking(Some(facts()))
}

#[test]
fn completion_offers_the_commands_then_issues_then_whom() {
    let commands = labels(suggest("/", 1, &completion()));
    assert!(commands.starts_with(&["/assign".into(), "/ready".into(), "/decision".into()]));
    let offer = suggest("/as", 3, &completion()).unwrap();
    assert_eq!(offer.choices[0].insert, "/assign ");
    assert!(
        offer.choices[0]
            .item
            .spoken_label()
            .contains("Assign an issue to Lyra"),
        "the ship's captain by its ship's name"
    );
    assert_eq!(
        labels(suggest("/assign ", 8, &completion())),
        ["NAV-7", "NAV-2", "NAV-", "MAP-"],
        "newest in play first, then the tracker's projects"
    );
    assert_eq!(labels(suggest("/assign m", 9, &completion())), ["MAP-"]);
    let whom = labels(suggest("/assign NAV-2 t", 15, &completion()));
    assert_eq!(whom, ["to aurora-lyra", "to aurora-orion", "to me"]);
    assert_eq!(
        labels(suggest("/ready NAV-2 ", 13, &completion()))[1],
        "off"
    );
    assert!(
        crate::compose::suggest::Facts::default().tracker.is_none()
            && labels(suggest(
                "/as",
                3,
                &crate::compose::suggest::Facts::default()
            ))
            .iter()
            .all(|label| label != "/assign"),
        "not offered where the tracker is not"
    );
}

#[test]
fn a_whole_command_s_first_row_says_what_return_will_do() {
    let text = "/assign NAV-2 ";
    let offer = suggest(text, text.len(), &completion()).unwrap();
    let first = &offer.choices[0];
    assert!(first.run, "choosing it runs the draft");
    assert_eq!(
        first.item.spoken_label().as_ref(),
        "Return assigns NAV-2 · Course correction preview to aurora-lyra"
    );
    let text = "/ready NAV-7 ";
    let offer = suggest(text, text.len(), &completion()).unwrap();
    assert!(
        offer.choices.iter().all(|choice| !choice.run),
        "a command that would be refused offers nothing to run"
    );
    let text = "/decision NAV-2 Decode on a pool";
    let offer = suggest(text, text.len(), &completion()).unwrap();
    assert_eq!(
        offer.choices[0].item.spoken_label().as_ref(),
        "Return records this decision on NAV-2 · Course correction preview"
    );
}

mod message;
mod write;

#[test]
fn a_captain_without_a_user_is_no_default_assignee() {
    use crate::fleet::Roster;
    use crate::fleet::tests::{agent, fleet};
    let mut lyra = agent("aurora-lyra", "captain", Some("aurora-lyra"), Some("Lyra"));
    lyra["userId"] = serde_json::json!("");
    let roster = Roster::new([&fleet("aurora", vec![lyra])]);
    let facts = super::gather(
        &crate::issues::IssueReferences::default(),
        &crate::tiles::Tiles::default(),
        &roster,
        &crate::fixtures::channel("#aurora-lyra"),
        None,
    );
    assert!(facts.captain.is_none(), "no assigneeId \"\" is ever sent");
    assert_eq!(facts.ship.as_deref(), Some("Lyra"));
    assert!(
        plan(&typed("/assign NAV-2").unwrap(), &facts).is_err(),
        "asks whom"
    );
}

#[test]
fn a_decision_too_long_in_utf_16_units_gets_no_run_row() {
    // 25,001 emoji: 25,001 characters but 50,002 UTF-16 units.
    let text = format!("/decision NAV-2 {}", "\u{1F680}".repeat(25_001));
    assert_eq!(
        planned(&text),
        Err("This decision is too long to record. Shorten it.".to_owned())
    );
}
