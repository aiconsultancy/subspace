//! Diagnostics keep stable identifiers and counts, never provider text.
use super::{
    IssueDetail, IssueFields, IssueGroup, IssueGroups, IssueListItem, IssueMedia, IssueProvenance,
    IssueReference, Label, Me, MentionStatus, Page, Person, Project, Relation, RelayedCrew,
    StateSummary, Todo, WorkflowState,
};

impl std::fmt::Debug for IssueGroups {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueGroups")
            .field("group_by", &self.group_by)
            .field("group_count", &self.groups.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueGroup")
            .field("state_id", &self.state_id)
            .field("state_type", &self.state_type)
            .field("count", &self.count)
            .field("item_count", &self.items.len())
            .field("has_more", &self.next_cursor.is_some())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("name", &"[redacted]")
            .field("state_count", &self.states.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for WorkflowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowState")
            .field("id", &self.id)
            .field("project_id", &self.project_id)
            .field("name", &"[redacted]")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueFields")
            .field("id", &self.id)
            .field("identifier", &self.identifier)
            .field("project_id", &self.project_id)
            .field("project_key", &self.project_key)
            .field("title", &"[redacted]")
            .field("comment_count", &self.comment_count)
            .field("todo_counts", &self.todo_counts)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueListItem")
            .field("fields", &self.fields)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueDetail")
            .field("fields", &self.fields)
            .field("description", &"[redacted]")
            .field("todo_count", &self.todos.len())
            .field("children", &self.children)
            .field("relations", &self.relations)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueMedia")
            .field("media_id", &self.media_id)
            .field("file_name", &"[redacted]")
            .field("size_bytes", &self.size_bytes)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for MentionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MentionStatus")
            .field("user_id", &self.user_id)
            .field("status", &self.status)
            .field("text", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueProvenance")
            .field("original_identifier", &self.original_identifier)
            .field("text", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for StateSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateSummary")
            .field("id", &self.id)
            .field("name", &"[redacted]")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Person {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Person")
            .field("user_id", &self.user_id)
            .field("display_name", &"[redacted]")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Label")
            .field("id", &self.id)
            .field("name", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for IssueReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssueReference")
            .field("id", &self.id)
            .field("identifier", &self.identifier)
            .field("title", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Relation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relation")
            .field("issue", &self.issue)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Me {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Me")
            .field("user_id", &self.user_id)
            .field("display_name", &"[redacted]")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for RelayedCrew {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayedCrew")
            .field("text", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Todo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Todo")
            .field("id", &self.id)
            .field("issue_id", &self.issue_id)
            .field("text", &"[redacted]")
            .field("done", &self.done)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl<T> std::fmt::Debug for Page<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("item_count", &self.items.len())
            .field("has_more", &self.next_cursor.is_some())
            .finish_non_exhaustive()
    }
}
