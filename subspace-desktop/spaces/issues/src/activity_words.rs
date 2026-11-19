use subspace_api_client::model::{ActivityActor, ActivityActorType, ActivityKind, LastActivity};

pub(crate) fn summary(activity: Option<&LastActivity>, age: &str) -> String {
    let mut summary = activity.map_or_else(
        || "Created".into(),
        |entry| change(entry.kind(), entry.completed_item_count()),
    );
    if let Some(actor) = activity.and_then(|entry| actor_name(entry.actor())) {
        summary.push_str(" · ");
        summary.push_str(&actor);
    }
    if !age.is_empty() {
        summary.push_str(" · ");
        summary.push_str(age);
    }
    summary
}

pub(crate) fn actor_name(actor: &ActivityActor) -> Option<String> {
    let name = actor
        .person()
        .map(|person| person.display_name.as_str())
        .or_else(|| actor.label())
        .filter(|name| !name.is_empty());
    match (actor.kind(), name) {
        (ActivityActorType::Agent, Some(name)) => Some(format!("{name} (agent)")),
        (ActivityActorType::Agent, None) => Some("Agent".into()),
        (ActivityActorType::Integration, None) => Some("Integration".into()),
        (ActivityActorType::System, None) => Some("System".into()),
        (_, name) => name.map(str::to_owned),
    }
}

pub(crate) fn change(kind: ActivityKind, completed_item_count: Option<u32>) -> String {
    let phrase = match kind {
        ActivityKind::IssueCreated => "Created",
        ActivityKind::IssueMoved => "Moved to another project",
        ActivityKind::IssueStateDerived => "Tracker status changed",
        ActivityKind::IssueArchived => "Archived",
        ActivityKind::IssueUnarchived => "Unarchived",
        ActivityKind::IssueRelationAdded => "Relation added",
        ActivityKind::IssueRelationRemoved => "Relation removed",
        ActivityKind::CommentCreated => "Comment added",
        ActivityKind::CommentEdited => "Comment edited",
        ActivityKind::CommentDeleted => "Comment deleted",
        ActivityKind::ThreadResolved => "Thread resolved",
        ActivityKind::ThreadUnresolved => "Thread reopened",
        ActivityKind::DocumentsReferenced => "Document linked",
        ActivityKind::DocumentsUnreferenced => "Document unlinked",
        ActivityKind::GithubPrLinked => "Pull request linked",
        ActivityKind::GithubMergeCompleted => "Pull request merged",
        ActivityKind::DecisionLogged => "Decision logged",
        ActivityKind::ReviewSubmitted => "Review submitted",
        ActivityKind::ItemCreated => "Checklist item added",
        ActivityKind::ItemEdited => "Checklist item edited",
        ActivityKind::ItemCompleted => "Checklist item completed",
        ActivityKind::ItemReopened => "Checklist item reopened",
        ActivityKind::ItemRemoved => "Checklist item removed",
        ActivityKind::ItemMoved => "Checklist item moved",
        ActivityKind::ListCreated => "Checklist added",
        ActivityKind::ListRenamed => "Checklist renamed",
        ActivityKind::ListRemoved => "Checklist removed",
        ActivityKind::ListMoved => "Checklist moved",
        ActivityKind::ListItemsCompleted => {
            return completed_item_count.map_or_else(
                || "Checklist items completed".into(),
                |count| format!("{count} checklist items completed"),
            );
        }
        _ => "Updated",
    };
    phrase.into()
}
