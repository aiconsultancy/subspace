//! What a tracker command would write, checked before anything is sent:
//! the issue is in the tracker's projects, the assignee is someone Channels
//! can name, and the issue's state allows the change. The same checks run
//! again on the issue as the tracker returns it just before the write.
//! Pure.

use std::collections::HashMap;

pub(crate) use super::check::check;
use super::parse::{Name, Typed};
use crate::markup::IssueKeys;

/// The most characters of an issue's title a preview or notice quotes.
const TITLE_CHARS: usize = 60;
/// The most characters a decision recorded from a command may have.
pub(crate) const DECISION_CHARS: usize = 50_000;

/// What the tracker last said about an issue, as far as a command needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Known {
    pub title: String,
    pub assignee: Option<String>,
    pub assignee_id: Option<String>,
    pub ready: Option<bool>,
    pub tracker: bool,
    pub closed: bool,
}

/// What the tracker said about an identifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Known(Known),
    /// It cannot be shown with this account; it may not exist.
    Unavailable,
    /// Not answered yet.
    Unknown,
}

/// An agent Channels can assign to: a fleet agent the roster knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentChoice {
    pub nick: String,
    pub user_id: String,
}

/// What the commands know about the conversation and the tracker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub keys: IssueKeys,
    /// The tracker's project keys, for completing a new identifier.
    pub projects: Vec<String>,
    /// The issues this conversation has shown (its tile owners), newest
    /// first.
    pub in_play: Vec<String>,
    /// What the tracker said about each identifier asked for.
    pub answers: HashMap<String, Answer>,
    /// The ship's captain in a ship's channel: `/assign`'s default.
    pub captain: Option<AgentChoice>,
    /// The ship's name, such as `Lyra`, for the command's description.
    pub ship: Option<String>,
    /// Every fleet agent of the network, by nick.
    pub agents: Vec<AgentChoice>,
    /// The tracker the Issues scope names (its profile and API origin),
    /// which a run row is bound to.
    pub tracker: Option<(String, String)>,
}

impl Facts {
    /// What the tracker said about `identifier`.
    pub(crate) fn answer(&self, identifier: &str) -> &Answer {
        self.answers.get(identifier).unwrap_or(&Answer::Unknown)
    }

    fn known(&self, identifier: &str) -> Option<&Known> {
        match self.answer(identifier) {
            Answer::Known(known) => Some(known),
            _ => None,
        }
    }
}

/// The fleet agent of `agents` with `nick`, ignoring case.
fn agent<'a>(agents: &'a [AgentChoice], nick: &str) -> Option<&'a AgentChoice> {
    agents
        .iter()
        .find(|agent| agent.nick.eq_ignore_ascii_case(nick))
}

/// Whom an assignment names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Who {
    Agent(AgentChoice),
    /// The signed-in person.
    Me,
}

impl Who {
    pub(crate) fn words(&self) -> &str {
        match self {
            Self::Agent(agent) => &agent.nick,
            Self::Me => "you",
        }
    }
}

/// What a command will write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    Assign { issue: String, to: Who },
    Ready { issue: String, on: bool },
    Decision { issue: String, text: String },
}

impl Plan {
    pub(crate) fn issue(&self) -> &str {
        match self {
            Self::Assign { issue, .. }
            | Self::Ready { issue, .. }
            | Self::Decision { issue, .. } => issue,
        }
    }

    /// What it will do, as the completion row says before Return.
    pub(crate) fn preview(&self, known: Option<&Known>) -> String {
        let issue = match known {
            Some(known) => format!("{} · {}", self.issue(), clip(&known.title, TITLE_CHARS)),
            None => self.issue().to_owned(),
        };
        match self {
            Self::Assign { to, .. } => format!("Return assigns {issue} to {}", to.words()),
            Self::Ready { on: true, .. } => format!("Return flags {issue} Ready for agents"),
            Self::Ready { on: false, .. } => {
                format!("Return clears Ready for agents on {issue}")
            }
            Self::Decision { .. } => format!("Return records this decision on {issue}"),
        }
    }
}

/// What `typed` would write, or why not, in plain words.
pub(crate) fn plan(typed: &Typed, facts: &Facts) -> Result<Plan, String> {
    let issue = issue(typed, facts)?;
    let plan = match typed.name {
        Name::Assign => Plan::Assign {
            to: assignee(&issue, &typed.rest, facts)?,
            issue,
        },
        Name::Ready => Plan::Ready {
            on: ready_on(&issue, &typed.rest)?,
            issue,
        },
        Name::Decision => Plan::Decision {
            text: decision(&issue, &typed.rest)?,
            issue,
        },
    };
    match facts.known(plan.issue()) {
        Some(known) => check(&plan, known, assigned_id(&plan)).map(|()| plan),
        None => Ok(plan),
    }
}

/// The user an assignment names, when the plan already knows it; `Me`
/// is only known once the tracker says who is signed in.
fn assigned_id(plan: &Plan) -> Option<&str> {
    match plan {
        Plan::Assign {
            to: Who::Agent(agent),
            ..
        } => Some(&agent.user_id),
        _ => None,
    }
}

/// The issue `typed` names, in the tracker's projects and not known to be
/// missing.
fn issue(typed: &Typed, facts: &Facts) -> Result<String, String> {
    let word = typed.name.word();
    let Some(issue) = typed.issue.clone() else {
        return Err(format!(
            "Name an issue after /{word}, such as /{word} NAV-2."
        ));
    };
    if facts.keys.leading(&issue) != Some(issue.len()) {
        return Err(format!(
            "{} isn't an issue in this tracker's projects.",
            clip(&issue, TITLE_CHARS)
        ));
    }
    if facts.answer(&issue) == &Answer::Unavailable {
        return Err(missing(&issue));
    }
    Ok(issue)
}

/// Why an issue cannot be written: it does not exist or this account
/// cannot see it.
pub(crate) fn missing(issue: &str) -> String {
    format!("{issue} isn't in the tracker, or you can't see it.")
}

fn assignee(issue: &str, rest: &str, facts: &Facts) -> Result<Who, String> {
    if rest.is_empty() {
        return facts.captain.clone().map(Who::Agent).ok_or_else(|| {
            format!("Say whom: /assign {issue} to <agent>, or /assign {issue} to me.")
        });
    }
    let mut words = rest.split_whitespace();
    let (Some(to), Some(name), None) = (words.next(), words.next(), words.next()) else {
        return Err(format!("Use /assign {issue} to <agent>, or to me."));
    };
    if !to.eq_ignore_ascii_case("to") {
        return Err(format!("Use /assign {issue} to <agent>, or to me."));
    }
    let name = name.trim_start_matches('@');
    if name.eq_ignore_ascii_case("me") {
        return Ok(Who::Me);
    }
    agent(&facts.agents, name).cloned().map(Who::Agent).ok_or_else(|| {
        format!(
            "{} isn't one of this fleet's agents. Channels assigns to an agent or to you; assign anyone else in Issues.",
            clip(name, TITLE_CHARS)
        )
    })
}

fn ready_on(issue: &str, rest: &str) -> Result<bool, String> {
    match rest.to_ascii_lowercase().as_str() {
        "" | "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!(
            "Use /ready {issue}, or /ready {issue} off to clear the flag."
        )),
    }
}

fn decision(issue: &str, rest: &str) -> Result<String, String> {
    if rest.is_empty() {
        return Err(format!(
            "Write the decision after the issue: /decision {issue} <decision>."
        ));
    }
    if rest.chars().count() > DECISION_CHARS {
        return Err("This decision is too long to record. Shorten it.".to_owned());
    }
    Ok(rest.to_owned())
}

/// `text` on one line, without control characters, cut to `limit` UTF-16
/// units (as the API counts) with an ellipsis.
pub(crate) fn clip(text: &str, limit: usize) -> String {
    let line = text
        .replace(
            |c: char| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'),
            " ",
        )
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if line.encode_utf16().count() <= limit {
        return line;
    }
    let mut cut = String::new();
    let mut units = 0;
    for character in line.chars() {
        units += character.len_utf16();
        if units > limit.saturating_sub(1) {
            break;
        }
        cut.push(character);
    }
    cut.push('…');
    cut
}
