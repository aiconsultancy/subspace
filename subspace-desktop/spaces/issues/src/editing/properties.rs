//! Sparse selections. Names stay in the view; only IDs go to Subspace API.
use crate::model::IssuePriority;
use std::collections::HashMap;
use subspace_api_client::{
    issue_writes::{ChoiceSource, IssueChanges, IssueChoice, IssueProperty},
    model::{IssueDetail, Project},
};
use subspace_framework::lucide::{IconName, SvgIcon};
mod labels;
use labels::{keep_edits, same_ids, toggle};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Field {
    Project,
    State,
    Priority,
    Assignee,
    Labels,
    Estimate,
    Type,
    Milestone,
    Cycle,
}
impl Field {
    pub(super) const ALL: [Self; 9] = [
        Self::Project,
        Self::Type,
        Self::State,
        Self::Priority,
        Self::Assignee,
        Self::Labels,
        Self::Estimate,
        Self::Milestone,
        Self::Cycle,
    ];
    pub(super) fn facts(self) -> (&'static str, &'static str, SvgIcon) {
        const FACTS: [(&str, &str, SvgIcon); 9] = [
            ("issue-edit-project", "Project", IconName::Folder),
            ("issue-edit-state", "State", IconName::CircleDot),
            ("issue-edit-priority", "Priority", IconName::SignalHigh),
            ("issue-edit-assignee", "Assignee", IconName::UserRound),
            ("issue-edit-labels", "Labels", IconName::Tags),
            ("issue-edit-estimate", "Estimate", IconName::Gauge),
            ("issue-edit-type", "Type", IconName::Layers),
            ("issue-edit-milestone", "Milestone", IconName::Flag),
            ("issue-edit-cycle", "Cycle", IconName::Calendar),
        ];
        FACTS[self as usize]
    }

    pub(super) fn empty_name(self) -> &'static str {
        const NAMES: [&str; 9] = [
            "Choose project",
            "Project default",
            "None",
            "Unassigned",
            "No labels",
            "No estimate",
            "Issue",
            "No milestone",
            "No cycle",
        ];
        NAMES[self as usize]
    }

    pub(super) fn source(self, values: &Properties, projects: &[Project]) -> Option<ChoiceSource> {
        let key = projects
            .iter()
            .find(|p| Some(p.id.as_str()) == values.id(Self::Project))
            .map(|p| p.key.clone());
        match self {
            Self::State => key.map(ChoiceSource::States),
            Self::Assignee => Some(ChoiceSource::Assignees),
            Self::Labels => Some(ChoiceSource::Labels),
            Self::Estimate => key.map(ChoiceSource::Estimates),
            Self::Milestone => key.map(ChoiceSource::Milestones),
            Self::Cycle => key.map(ChoiceSource::Cycles),
            _ => None,
        }
    }
    pub(super) fn can_clear(self) -> bool {
        matches!(
            self,
            Self::Assignee | Self::Estimate | Self::Milestone | Self::Cycle
        )
    }
}

#[derive(Default)]
pub(super) struct Properties {
    basis: HashMap<Field, Vec<IssueChoice>>,
    edits: HashMap<Field, Vec<IssueChoice>>,
}
impl Properties {
    pub(super) fn new(projects: &[Project], project: &str, detail: Option<&IssueDetail>) -> Self {
        let mut result = Self::default();
        result.install_baseline(projects, project, detail);
        result
    }
    pub(super) fn install_baseline(
        &mut self,
        projects: &[Project],
        project: &str,
        detail: Option<&IssueDetail>,
    ) {
        let previous_milestone = self.basis.remove(&Field::Milestone).unwrap_or_default();
        let previous_labels = self.basis.remove(&Field::Labels).unwrap_or_default();
        self.basis.clear();
        self.basis.insert(
            Field::Project,
            projects
                .iter()
                .filter(|p| p.id == project)
                .map(project_choice)
                .collect(),
        );
        self.basis
            .insert(Field::Type, vec![IssueChoice::new("issue", "Issue")]);
        if let Some(detail) = detail {
            self.issue_baseline(detail);
        }
        if let Some(previous) = previous_milestone.first()
            && self
                .basis
                .get(&Field::Milestone)
                .and_then(|values| values.first())
                .is_some_and(|current| current.id() == previous.id())
        {
            self.name_milestone(previous.name());
        }
        keep_edits(
            &previous_labels,
            self.basis.get(&Field::Labels).map_or(&[], Vec::as_slice),
            &mut self.edits,
        );
    }
    fn issue_baseline(&mut self, issue: &IssueDetail) {
        let f = &issue.fields;
        self.planning_baseline(issue);
        self.basis.extend([
            (
                Field::State,
                vec![IssueChoice::new(&f.state.id, &f.state.name)],
            ),
            (Field::Priority, vec![priority_choice(f.priority)]),
            (
                Field::Assignee,
                f.assignee
                    .as_ref()
                    .map(|p| IssueChoice::new(&p.user_id, &p.display_name))
                    .into_iter()
                    .collect(),
            ),
            (
                Field::Labels,
                f.labels
                    .iter()
                    .map(|l| IssueChoice::new(&l.id, &l.name))
                    .collect(),
            ),
            (
                Field::Estimate,
                f.estimate
                    .map(|v| IssueChoice::new(v.to_string(), v.to_string()))
                    .into_iter()
                    .collect(),
            ),
            (
                Field::Type,
                vec![IssueChoice::new(
                    &f.kind,
                    if f.kind == "tracker" {
                        "Tracker"
                    } else {
                        "Issue"
                    },
                )],
            ),
        ]);
    }
    fn planning_baseline(&mut self, issue: &IssueDetail) {
        let f = &issue.fields;
        self.basis.extend([
            (
                Field::Milestone,
                f.milestone_id
                    .as_ref()
                    .map(|id| IssueChoice::new(id, "Current milestone"))
                    .into_iter()
                    .collect(),
            ),
            (
                Field::Cycle,
                f.cycle
                    .as_ref()
                    .map(|c| IssueChoice::new(&c.id, format!("Cycle {} · {}", c.number, c.state)))
                    .or_else(|| {
                        f.cycle_id
                            .as_ref()
                            .map(|id| IssueChoice::new(id, "Current cycle"))
                    })
                    .into_iter()
                    .collect(),
            ),
        ]);
    }
    pub(super) fn name_milestone(&mut self, name: &str) {
        if let Some(choice) = self
            .basis
            .get_mut(&Field::Milestone)
            .and_then(|values| values.first_mut())
        {
            *choice = IssueChoice::new(choice.id(), name);
        }
    }
    pub(super) fn selected(&self, field: Field) -> &[IssueChoice] {
        self.edits
            .get(&field)
            .or_else(|| self.basis.get(&field))
            .map_or(&[], Vec::as_slice)
    }
    pub(super) fn id(&self, field: Field) -> Option<&str> {
        self.selected(field).first().map(IssueChoice::id)
    }
    pub(super) fn tracker(&self) -> bool {
        self.id(Field::Type) == Some("tracker")
    }
    pub(super) fn pick(&mut self, field: Field, choice: Option<IssueChoice>) {
        let selection = if field == Field::Labels {
            toggle(self.selected(field), choice)
        } else {
            choice.into_iter().collect()
        };
        if field == Field::Project && !same_ids(&selection, self.selected(field)) {
            self.edits.remove(&Field::State);
            self.edits.remove(&Field::Estimate);
            self.edits.remove(&Field::Milestone);
            self.edits.remove(&Field::Cycle);
        }
        if same_ids(
            &selection,
            self.basis.get(&field).map_or(&[], Vec::as_slice),
        ) {
            self.edits.remove(&field);
        } else {
            self.edits.insert(field, selection);
        }
    }
    pub(super) fn install(&self, changes: &mut IssueChanges) {
        for (&field, values) in &self.edits {
            if self.tracker() && matches!(field, Field::State | Field::Estimate | Field::Cycle) {
                continue;
            }
            if let Some(property) = property(field, values) {
                changes.set(property);
            }
        }
    }
    pub(super) fn has_edits(&self) -> bool {
        !self.edits.is_empty()
    }
}
fn property(field: Field, values: &[IssueChoice]) -> Option<IssueProperty> {
    let id = values.first().map(|v| v.id().to_owned());
    Some(match field {
        Field::Project => return None,
        Field::State => IssueProperty::State(id?),
        Field::Priority => IssueProperty::Priority(id?.parse().ok()?),
        Field::Assignee => IssueProperty::Assignee(id),
        Field::Labels => IssueProperty::Labels(values.iter().map(|v| v.id().to_owned()).collect()),
        Field::Estimate => IssueProperty::Estimate(id.and_then(|v| v.parse().ok())),
        Field::Type => IssueProperty::Type(id?),
        Field::Milestone => IssueProperty::Milestone(id),
        Field::Cycle => IssueProperty::Cycle(id),
    })
}
pub(super) fn project_choice(project: &Project) -> IssueChoice {
    IssueChoice::new(&project.id, format!("{} · {}", project.key, project.name))
}
pub(super) fn priority_choice(value: i32) -> IssueChoice {
    IssueChoice::new(
        value.to_string(),
        IssuePriority::from_value(value).look().label,
    )
}

#[cfg(test)]
mod tests {
    use super::{Field, Properties};
    use serde_json::json;
    use subspace_api_client::{issue_writes::IssueChanges, model::IssueDetail};

    #[test]
    fn known_milestone_name_labels_the_baseline_without_changing_the_link() {
        let mut value = crate::tests::offline::row("1", "Planning fixture");
        value["milestoneId"] = json!("milestone-1");
        value["todos"] = json!([]);
        let issue: IssueDetail = serde_json::from_value(value).unwrap();
        let mut properties = Properties::new(&[], "project-SUB", Some(&issue));
        properties.name_milestone("First delivery");
        assert_eq!(
            properties.selected(Field::Milestone)[0].name(),
            "First delivery"
        );
        let mut changes = IssueChanges::default();
        properties.install(&mut changes);
        assert!(changes.is_empty());
        properties.install_baseline(&[], "project-SUB", Some(&issue));
        assert_eq!(
            properties.selected(Field::Milestone)[0].name(),
            "First delivery"
        );
        let mut other = issue;
        other.fields.milestone_id = Some("milestone-2".into());
        properties.install_baseline(&[], "project-SUB", Some(&other));
        assert_ne!(
            properties.selected(Field::Milestone)[0].name(),
            "First delivery"
        );
    }
}
