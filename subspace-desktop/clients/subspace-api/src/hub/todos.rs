//! The open issue's to-dos: NETT's named issue lists and their items. Each
//! check carries the item's version; a conflict reads the lists again
//! rather than overwriting what someone else changed. Subspace API has no
//! issue lists, so nothing is read for it.

use super::{Hub, HubError, ReadPriority, Shared, lock, worker::Job, worker::Task};
use crate::Error;
use crate::issue_writes::IssueSaveError;
use crate::nett::{TodoItem, TodoList};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// One named list with its items.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TodoListView {
    pub list: TodoList,
    pub items: Vec<TodoItem>,
}

impl TodoListView {
    /// Items done, and items in all, as shown beside the list's name.
    pub fn counts(&self) -> (usize, usize) {
        (
            self.items.iter().filter(|item| item.completed).count(),
            self.items.len(),
        )
    }
}

#[derive(Clone, Default)]
pub(super) struct Todos {
    pub lists: Option<Vec<TodoListView>>,
    pub loading: bool,
    pub failed: bool,
    pub notice: Option<&'static str>,
    /// Items being saved, each with the save that holds it: only that
    /// save's answer clears the mark.
    pub saving: HashMap<String, u64>,
}

/// Numbers each save, so an answer clears only its own pending mark.
static SAVES: AtomicU64 = AtomicU64::new(1);

const CONFLICT: &str = "A to-do changed elsewhere. The lists were read again.";

/// Whether the open issue has to-dos to read.
pub(super) fn wanted(shared: &Shared) -> bool {
    shared.client.service() == crate::Backend::Nett
}

/// Queues a read of the open issue's lists.
pub(super) fn queue(shared: &Shared, state: &mut super::State) {
    if !wanted(shared) {
        return;
    }
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    issue.todos.loading = true;
    let identifier = issue.identifier.clone();
    shared.enqueue(state, Task::Todos(identifier), ReadPriority::Visible);
}

pub(super) fn load(shared: &Shared, job: &Job, identifier: &str) {
    let result = read(shared, identifier);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|issue| issue.identifier == identifier)
    {
        issue.todos.loading = false;
        match result {
            Ok(lists) => {
                let held = issue.todos.lists.take();
                issue.todos.lists = Some(merge(lists, held.as_deref()));
                issue.todos.failed = false;
            }
            Err(Error::NotFound) => {
                issue.todos.lists = Some(Vec::new());
                issue.todos.failed = false;
            }
            Err(_) => issue.todos.failed = true,
        }
    }
    state.changed();
}

/// A saved item replaces the one shown only if it is newer: a save whose
/// answer arrived late never undoes a newer version read meanwhile.
fn apply_saved(lists: Option<&mut [TodoListView]>, saved: &TodoItem) {
    for list in lists.into_iter().flatten() {
        if let Some(row) = list
            .items
            .iter_mut()
            .find(|row| row.id == saved.id && row.version < saved.version)
        {
            *row = saved.clone();
        }
    }
}

/// A read that started before a check was saved must not undo it: an item
/// already held at a newer version than the read's is kept.
fn merge(mut read: Vec<TodoListView>, held: Option<&[TodoListView]>) -> Vec<TodoListView> {
    let Some(held) = held else {
        return read;
    };
    for list in &mut read {
        for item in &mut list.items {
            if let Some(newer) = held
                .iter()
                .flat_map(|list| &list.items)
                .find(|kept| kept.id == item.id && kept.version > item.version)
            {
                *item = newer.clone();
            }
        }
    }
    read
}

fn read(shared: &Shared, identifier: &str) -> Result<Vec<TodoListView>, Error> {
    let lists = shared.client.issue_lists(identifier)?;
    lists
        .into_iter()
        .map(|list| {
            let items = shared.client.issue_list_items(identifier, &list.id)?;
            Ok(TodoListView { list, items })
        })
        .collect()
}

impl Hub {
    /// Reads the open issue's to-do lists again.
    pub fn reload_todos(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let loaded = state
            .snapshot
            .issue
            .as_ref()
            .is_some_and(|issue| issue.detail.is_some() && !issue.todos.loading);
        if loaded {
            queue(&self.0, &mut state);
            state.changed();
        }
        Ok(())
    }

    /// Checks or unchecks one of the open issue's to-dos at the version on
    /// screen.
    pub fn set_todo_done(&self, item: &str, completed: bool) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let generation = state.snapshot.generation;
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return Ok(());
        };
        let Some(found) = issue
            .todos
            .lists
            .iter()
            .flatten()
            .flat_map(|list| &list.items)
            .find(|row| row.id == item)
            .cloned()
        else {
            return Ok(());
        };
        if found.completed == completed || issue.todos.saving.contains_key(&found.id) {
            return Ok(());
        }
        let operation = SAVES.fetch_add(1, Ordering::Relaxed);
        issue.todos.saving.insert(found.id.clone(), operation);
        issue.todos.notice = None;
        let identifier = issue.identifier.clone();
        // The issue as opened now: an answer that lands after it was
        // closed or opened again belongs to that earlier opening.
        let revision = state.issue_revision;
        state.changed();
        let shared = Arc::clone(&self.0);
        std::thread::spawn(move || {
            let result = shared
                .client
                .set_issue_list_item_done(&identifier, &found, completed);
            save(
                &shared,
                (generation, revision),
                (&identifier, &found.id, operation),
                result,
            );
        });
        Ok(())
    }
}

fn save(
    shared: &Shared,
    (generation, revision): (super::ScopeGeneration, u64),
    (identifier, item, operation): (&str, &str, u64),
    result: Result<TodoItem, IssueSaveError>,
) {
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return;
    }
    let reopened = state.issue_revision != revision;
    let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|issue| issue.identifier == identifier)
    else {
        return;
    };
    // Only this save's own mark goes: a later save of the same item, from
    // this opening or another, stays pending until its own answer.
    if issue.todos.saving.get(item) == Some(&operation) {
        issue.todos.saving.remove(item);
    }
    if reopened {
        // The issue was read again since, perhaps before this save was
        // stored: a stored save is reconciled with a fresh read.
        let stored = result.is_ok();
        state.changed();
        if stored {
            queue(shared, &mut state);
        }
        return;
    }
    let conflict = matches!(result, Err(IssueSaveError::Conflict));
    match result {
        Ok(saved) => apply_saved(issue.todos.lists.as_deref_mut(), &saved),
        Err(IssueSaveError::Conflict) => issue.todos.notice = Some(CONFLICT),
        Err(IssueSaveError::SignIn) => {
            issue.todos.notice = Some("Sign in under Settings › Account, then try again.");
        }
        Err(IssueSaveError::Forbidden) => {
            issue.todos.notice = Some("You don't have permission to change this to-do.");
        }
        Err(IssueSaveError::RateLimited) => {
            issue.todos.notice = Some("NETT is busy. Wait a moment, then try again.");
        }
        Err(_) => issue.todos.notice = Some("Couldn't save the to-do. Try again."),
    }
    state.changed();
    if conflict {
        queue(shared, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::{TodoListView, merge};

    fn list(items: &[(&str, bool, u64)]) -> Vec<TodoListView> {
        let list = serde_json::from_value(serde_json::json!(
            {"id": "l1", "name": "Checklist", "version": 1}
        ))
        .unwrap();
        let items = items
            .iter()
            .map(|(id, completed, version)| {
                serde_json::from_value(serde_json::json!({
                    "id": id, "listId": "l1", "text": id, "completed": completed, "version": version
                }))
                .unwrap()
            })
            .collect();
        vec![TodoListView { list, items }]
    }

    #[test]
    fn a_late_save_does_not_undo_a_newer_read() {
        // Someone else's change was read at version 3 before this save's
        // answer, at version 2, arrived.
        let mut held = list(&[("a", true, 3)]);
        let late: crate::nett::TodoItem = serde_json::from_value(serde_json::json!({
            "id": "a", "listId": "l1", "text": "a", "completed": false, "version": 2
        }))
        .unwrap();
        super::apply_saved(Some(&mut held), &late);
        assert_eq!(
            (held[0].items[0].completed, held[0].items[0].version),
            (true, 3)
        );
        let newer: crate::nett::TodoItem = serde_json::from_value(serde_json::json!({
            "id": "a", "listId": "l1", "text": "a", "completed": false, "version": 4
        }))
        .unwrap();
        super::apply_saved(Some(&mut held), &newer);
        assert_eq!(held[0].items[0].version, 4);
    }

    #[test]
    fn a_read_from_before_a_saved_check_does_not_undo_it() {
        // The check saved item b at version 2; a read that left before the
        // save lands afterwards with version 1.
        let held = list(&[("a", false, 1), ("b", true, 2)]);
        let late = list(&[("a", true, 3), ("b", false, 1)]);
        let merged = merge(late, Some(&held));
        let items: Vec<_> = merged[0]
            .items
            .iter()
            .map(|item| (item.id.as_str(), item.completed, item.version))
            .collect();
        assert_eq!(items, [("a", true, 3), ("b", true, 2)]);
    }
}
