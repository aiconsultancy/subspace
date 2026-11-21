//! What the person chose to look at: a view (a status), who made the
//! decisions, a project and a tag; and the list read that choice and the
//! search ask Subspace API for. Pure, so the tests read it without a window.

use subspace_api_client::DecisionQuery;
use subspace_api_client::model::{DecisionMakerType, DecisionStatus};
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::words;

/// The API reads at most this many characters of a search.
const MAX_SEARCH_CHARS: usize = 200;
/// A stored project choice starts with this, then the project's key.
const PROJECT_PREFIX: &str = "project:";
/// The stored value of every project.
const ALL_PROJECTS: &str = "all";

/// A navigator view: every decision, or the decisions in one status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum View {
    #[default]
    All,
    Proposed,
    Accepted,
    Rejected,
    Superseded,
}

impl View {
    /// The navigator's order, which is also the keys `1`–`5`.
    pub(crate) const ALL: [View; 5] = [
        View::All,
        View::Proposed,
        View::Accepted,
        View::Rejected,
        View::Superseded,
    ];

    /// Each view's stored value and the status it reads, in the order the
    /// variants are declared, so a view's discriminant is its entry.
    const READS: [(&'static str, Option<DecisionStatus>); 5] = [
        ("all", None),
        ("proposed", Some(DecisionStatus::Proposed)),
        ("accepted", Some(DecisionStatus::Accepted)),
        ("rejected", Some(DecisionStatus::Rejected)),
        ("superseded", Some(DecisionStatus::Superseded)),
    ];

    /// The view's stored value and the end of its navigator row's id.
    pub(crate) fn key(self) -> &'static str {
        Self::READS[self as usize].0
    }

    pub(crate) fn from_key(key: &str) -> Option<View> {
        View::ALL.into_iter().find(|view| view.key() == key)
    }

    /// The status the view reads, or none for every decision.
    pub(crate) fn status(self) -> Option<DecisionStatus> {
        Self::READS[self as usize].1
    }

    pub(crate) fn label(self) -> &'static str {
        self.status().map_or("All decisions", words::status)
    }

    pub(crate) fn icon(self) -> SvgIcon {
        self.status().map_or(IconName::List, words::status_icon)
    }
}

/// Who made the decisions the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Maker {
    #[default]
    Anyone,
    People,
    Agents,
}

impl Maker {
    pub(crate) const ALL: [Maker; 3] = [Maker::Anyone, Maker::People, Maker::Agents];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Maker::Anyone => "anyone",
            Maker::People => "people",
            Maker::Agents => "agents",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Maker> {
        Maker::ALL.into_iter().find(|maker| maker.key() == key)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Maker::Anyone => "Anyone",
            Maker::People => "People",
            Maker::Agents => "Agents",
        }
    }

    pub(crate) fn icon(self) -> SvgIcon {
        match self {
            Maker::Anyone => IconName::Users,
            Maker::People => IconName::User,
            Maker::Agents => IconName::Bot,
        }
    }

    /// The maker type the list read asks for, or none for anyone.
    pub(crate) fn kind(self) -> Option<DecisionMakerType> {
        match self {
            Maker::Anyone => None,
            Maker::People => Some(DecisionMakerType::Human),
            Maker::Agents => Some(DecisionMakerType::Agent),
        }
    }
}

/// Every project, or the one with this key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum ProjectChoice {
    #[default]
    All,
    Project(String),
}

impl ProjectChoice {
    /// The stored value, also the end of the navigator row's id.
    pub(crate) fn key(&self) -> String {
        match self {
            ProjectChoice::All => ALL_PROJECTS.to_owned(),
            ProjectChoice::Project(key) => format!("{PROJECT_PREFIX}{key}"),
        }
    }

    /// A stored value read back; anything else is every project.
    pub(crate) fn from_key(value: &str) -> ProjectChoice {
        value
            .strip_prefix(PROJECT_PREFIX)
            .filter(|key| !key.is_empty())
            .map_or(ProjectChoice::All, |key| {
                ProjectChoice::Project(key.to_owned())
            })
    }

    /// The project key the list read asks for.
    pub(crate) fn project_key(&self) -> Option<&str> {
        match self {
            ProjectChoice::All => None,
            ProjectChoice::Project(key) => Some(key),
        }
    }
}

/// Everything the navigator and the toolbar choose, apart from the search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Choice {
    pub(crate) view: View,
    pub(crate) maker: Maker,
    pub(crate) project: ProjectChoice,
    pub(crate) tag: Option<String>,
}

impl Choice {
    /// Whether anything narrows the list beyond the search.
    pub(crate) fn narrows(&self) -> bool {
        self.view != View::All
            || self.maker != Maker::Anyone
            || self.project != ProjectChoice::All
            || self.tag.is_some()
    }
}

/// The list read for `choice` and the committed `search` text.
pub(crate) fn query(choice: &Choice, search: &str) -> DecisionQuery {
    let mut query = DecisionQuery::default();
    query.status = choice.view.status().into_iter().collect();
    query.made_by_type = choice.maker.kind();
    query.project = choice.project.project_key().map(str::to_owned);
    query.tag = choice.tag.clone();
    query.q = Some(search.to_owned()).filter(|text| !text.is_empty());
    query
}

/// The search text as the list reads it: trimmed, and no longer than the
/// API accepts.
pub(crate) fn search_text(text: &str) -> String {
    text.trim().chars().take(MAX_SEARCH_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use subspace_api_client::model::{DecisionMakerType, DecisionStatus};

    use super::{Choice, Maker, ProjectChoice, View, query, search_text};

    #[test]
    fn every_choice_reads_back_from_its_stored_value() {
        for view in View::ALL {
            assert_eq!(View::from_key(view.key()), Some(view));
        }
        for maker in Maker::ALL {
            assert_eq!(Maker::from_key(maker.key()), Some(maker));
        }
        let project = ProjectChoice::Project("NAV".into());
        assert_eq!(ProjectChoice::from_key(&project.key()), project);
        assert_eq!(ProjectChoice::from_key("all"), ProjectChoice::All);
        assert_eq!(ProjectChoice::from_key("project:"), ProjectChoice::All);
        assert_eq!(ProjectChoice::from_key("unknown"), ProjectChoice::All);
        assert_eq!(View::from_key("archived"), None);
    }

    #[test]
    fn the_views_follow_the_navigator_order_and_name_their_status() {
        let labels: Vec<_> = View::ALL.into_iter().map(View::label).collect();
        assert_eq!(
            labels,
            [
                "All decisions",
                "Proposed",
                "Accepted",
                "Rejected",
                "Superseded"
            ]
        );
        assert_eq!(View::All.status(), None);
        assert_eq!(View::Superseded.status(), Some(DecisionStatus::Superseded));
    }

    #[test]
    fn the_query_carries_the_navigator_choices_and_the_search() {
        let choice = Choice {
            view: View::Accepted,
            maker: Maker::Agents,
            project: ProjectChoice::Project("WARP".into()),
            tag: Some("telemetry".into()),
        };
        let read = query(&choice, "relay");
        assert_eq!(read.status, vec![DecisionStatus::Accepted]);
        assert_eq!(read.made_by_type, Some(DecisionMakerType::Agent));
        assert_eq!(read.project.as_deref(), Some("WARP"));
        assert_eq!(read.tag.as_deref(), Some("telemetry"));
        assert_eq!(read.q.as_deref(), Some("relay"));
        assert!(read.issue.is_none());
        assert!(choice.narrows());
    }

    #[test]
    fn every_decision_with_no_search_asks_for_no_filter() {
        let read = query(&Choice::default(), "");
        assert!(read.status.is_empty());
        assert!(read.made_by_type.is_none());
        assert!(read.project.is_none() && read.tag.is_none() && read.q.is_none());
        assert!(!Choice::default().narrows());
        let people = query(
            &Choice {
                maker: Maker::People,
                ..Choice::default()
            },
            "",
        );
        assert_eq!(people.made_by_type, Some(DecisionMakerType::Human));
    }

    #[test]
    fn the_search_is_trimmed_and_bounded() {
        assert_eq!(search_text("  relay  "), "relay");
        assert_eq!(search_text(&"x".repeat(300)).chars().count(), 200);
        assert_eq!(search_text("   "), "");
    }
}
