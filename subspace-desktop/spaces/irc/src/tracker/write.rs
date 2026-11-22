//! The writes themselves, run on a background thread with the desktop's
//! Subspace API client as the signed-in person. Each reads the issue as
//! the tracker has it now, checks the change again against that, then
//! writes once; nothing is retried. Results and refusals come back as the
//! notice's plain words, never with a token or a server body in them.

use subspace_api_client::decision_writes::{DecisionText, IrcEvidence, NewDecision};
use subspace_api_client::issue_writes::{IssueChanges, IssueProperty, IssueSave};
use subspace_api_client::model::{IssueDetail, IssueFields};
use subspace_api_client::{Client, Error};

use super::plan::{Known, Plan, Who, check, clip, missing};

/// The most characters of a title a notice quotes.
const NOTICE_TITLE_CHARS: usize = 80;
/// The API's longest issue or decision title.
pub(crate) const TITLE_CHARS: usize = 200;
/// The API's longest issue summary.
pub(crate) const SUMMARY_CHARS: usize = 300;

/// An issue to create from a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IssueDraft {
    /// The project's key, such as `NAV`.
    pub project: String,
    pub title: String,
    pub summary: String,
    pub description: String,
}

/// A decision to record on an issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecisionDraft {
    pub issue: String,
    pub title: String,
    pub text: String,
    /// Where it was recorded from, such as `Recorded in Channels from
    /// #aurora-lyra on Acme IRC.`
    pub context: String,
    /// The message it was taken from: a reference back to it and its
    /// words, when it came from a message.
    pub message: Option<(String, String)>,
}

/// One write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Job {
    Assign { issue: String, to: Who },
    Ready { issue: String, on: bool },
    Decide(DecisionDraft),
    Create(IssueDraft),
}

impl Job {
    /// The write a command's plan asks for; `context` says where a
    /// decision was recorded from.
    pub(crate) fn from_plan(plan: Plan, context: String) -> Self {
        match plan {
            Plan::Assign { issue, to } => Self::Assign { issue, to },
            Plan::Ready { issue, on } => Self::Ready { issue, on },
            Plan::Decision { issue, text } => Self::Decide(DecisionDraft {
                title: title_of(&text),
                issue,
                text,
                context,
                message: None,
            }),
        }
    }

    /// What the notice says while it runs.
    pub(crate) fn running(&self) -> String {
        match self {
            Self::Assign { issue, .. } => format!("Assigning {issue}…"),
            Self::Ready { issue, on: true } => format!("Flagging {issue} Ready for agents…"),
            Self::Ready { issue, on: false } => format!("Clearing Ready for agents on {issue}…"),
            Self::Decide(draft) => format!("Recording the decision on {}…", draft.issue),
            Self::Create(draft) => format!("Creating the issue in {}…", draft.project),
        }
    }
}

/// A write that landed: what the notice says, and the issue whose shown
/// state is now out of date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Done {
    pub notice: String,
    pub changed: String,
}

/// A title from the first line of `text`, within the API's limit.
pub(crate) fn title_of(text: &str) -> String {
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text);
    clip(first, TITLE_CHARS)
}

/// Runs `job` with `client`; blocking.
pub(crate) fn run(client: &Client, job: &Job) -> Result<Done, String> {
    match job {
        Job::Assign { issue, to } => assign(client, issue, to),
        Job::Ready { issue, on } => ready(client, issue, *on),
        Job::Decide(draft) => decide(client, draft),
        Job::Create(draft) => create(client, draft),
    }
}

fn assign(client: &Client, issue: &str, to: &Who) -> Result<Done, String> {
    let me = client.me().map_err(|error| read_error(issue, &error))?;
    let detail = read(client, issue)?;
    let (user, name) = match to {
        Who::Agent(agent) => (agent.user_id.clone(), agent.nick.clone()),
        Who::Me => (me.user_id, "you".to_owned()),
    };
    let before = Known::of(&detail.fields);
    let plan = Plan::Assign {
        issue: issue.to_owned(),
        to: to.clone(),
    };
    check(&plan, &before, Some(&user))?;
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Assignee(Some(user)));
    let after = save(client, &detail, &changes)?;
    let unflagged = before.ready == Some(true) && after.ready != Some(true);
    let note = if unflagged {
        " It is no longer flagged Ready for agents."
    } else {
        ""
    };
    Ok(Done {
        notice: format!("Assigned {issue} to {name}.{note}"),
        changed: issue.to_owned(),
    })
}

fn ready(client: &Client, issue: &str, on: bool) -> Result<Done, String> {
    client.me().map_err(|error| read_error(issue, &error))?;
    let detail = read(client, issue)?;
    let plan = Plan::Ready {
        issue: issue.to_owned(),
        on,
    };
    check(&plan, &Known::of(&detail.fields), None)?;
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::ReadyForAgents(on));
    save(client, &detail, &changes)?;
    let notice = if on {
        format!("Flagged {issue} Ready for agents.")
    } else {
        format!("{issue} is no longer flagged Ready for agents.")
    };
    Ok(Done {
        notice,
        changed: issue.to_owned(),
    })
}

fn decide(client: &Client, draft: &DecisionDraft) -> Result<Done, String> {
    let issue = &draft.issue;
    client.me().map_err(|error| read_error(issue, &error))?;
    let detail = read(client, issue)?;
    let fields = &detail.fields;
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let message = draft
        .message
        .as_ref()
        .map(|(reference, excerpt)| IrcEvidence { reference, excerpt });
    let text = DecisionText {
        title: &draft.title,
        decision: &draft.text,
        context: Some(&draft.context),
        decided_on: &today,
        message,
    };
    let refused = |error: subspace_api_client::decision_writes::DecisionWriteError| {
        format!("Couldn't record the decision on {issue}: {error}")
    };
    let decision = NewDecision::new(&fields.project_id, &[&fields.id], &text).map_err(refused)?;
    let recorded = client.log_decision(&decision).map_err(refused)?;
    Ok(Done {
        notice: format!(
            "Recorded a decision on {issue}: “{}”.",
            clip(&recorded.title, NOTICE_TITLE_CHARS)
        ),
        changed: issue.clone(),
    })
}

fn create(client: &Client, draft: &IssueDraft) -> Result<Done, String> {
    let project = &draft.project;
    let refused = |words: String| format!("Couldn't create the issue in {project}: {words}");
    client.me().map_err(|error| refused(error.to_string()))?;
    let projects = client
        .all_projects()
        .map_err(|error| refused(error.to_string()))?;
    let Some(found) = projects
        .iter()
        .find(|row| row.key == *project && row.archived_at.is_none())
    else {
        return Err(refused(
            "the project isn't open in this tracker.".to_owned(),
        ));
    };
    let mut changes = IssueChanges::default();
    changes.set(IssueProperty::Title(draft.title.clone()));
    changes.set(IssueProperty::Summary(draft.summary.clone()));
    changes.set(IssueProperty::Description(Some(draft.description.clone())));
    let IssueSave::Saved(created) = client
        .create_issue(&found.id, &changes)
        .map_err(|error| refused(error.to_string()))?;
    let fields = &created.fields;
    Ok(Done {
        notice: format!(
            "Created {}: {}.",
            fields.identifier,
            clip(&fields.title, NOTICE_TITLE_CHARS)
        ),
        changed: fields.identifier.clone(),
    })
}

/// The issue as the tracker has it now.
fn read(client: &Client, issue: &str) -> Result<IssueDetail, String> {
    client
        .issue(issue)
        .map_err(|error| read_error(issue, &error))
}

/// Writes `changes` to `detail`'s issue at the version just read; what
/// the issue says after.
fn save(client: &Client, detail: &IssueDetail, changes: &IssueChanges) -> Result<Known, String> {
    let fields = &detail.fields;
    let issue = &fields.identifier;
    let version = u32::try_from(fields.version)
        .map_err(|_| format!("Couldn't change {issue}: the tracker sent an unexpected version."))?;
    let IssueSave::Saved(saved) = client
        .update_issue(&fields.id, version, changes)
        .map_err(|error| format!("Couldn't change {issue}: {error}"))?;
    Ok(Known::of(&saved.fields))
}

/// A failed read before a write, in plain words.
fn read_error(issue: &str, error: &Error) -> String {
    match error {
        Error::NotFound => missing(issue),
        Error::SignInNeeded => {
            "Sign in under Settings › Account to change the tracker from Channels.".to_owned()
        }
        Error::Forbidden => format!("You don't have permission to see {issue}."),
        other => format!("Couldn't reach the tracker for {issue}: {other}"),
    }
}

impl Known {
    /// What an issue's fields say, as a command needs.
    pub(crate) fn of(fields: &IssueFields) -> Self {
        Self {
            title: fields.title.clone(),
            assignee: fields
                .assignee
                .as_ref()
                .map(|person| person.display_name.clone()),
            assignee_id: fields
                .assignee
                .as_ref()
                .map(|person| person.user_id.clone()),
            ready: fields.ready_for_agents,
            tracker: fields.kind == "tracker",
            closed: matches!(
                fields.state.kind.as_str(),
                "completed" | "canceled" | "duplicate"
            ),
        }
    }
}
