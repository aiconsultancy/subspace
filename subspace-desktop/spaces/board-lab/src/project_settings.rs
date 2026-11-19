//! Native editor for private project environment settings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Window};
use subspace_framework::core::Secret;

use crate::private_settings::{Edit, ImportPreview, Metadata, PrivateSettings};
use crate::project::LoadedProject;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Keep,
    Replace,
    Clear,
}

struct Row {
    input: Entity<InputState>,
    choice: Choice,
}

/// Events contain no private values.
pub enum SettingsEvent {
    Saved,
    Close,
}

/// Owns the transient input and import preview for one project.
pub struct ProjectSettings {
    root: PathBuf,
    id: String,
    declared: Vec<String>,
    metadata: Option<Metadata>,
    rows: BTreeMap<String, Row>,
    path: Entity<InputState>,
    preview: Option<ImportPreview>,
    approved: BTreeSet<String>,
    busy: bool,
    error: Option<&'static str>,
}

impl EventEmitter<SettingsEvent> for ProjectSettings {}

impl ProjectSettings {
    pub fn new(
        root: PathBuf,
        project: LoadedProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let path = cx.new(|cx| InputState::new(window, cx));
        let mut view = Self {
            root,
            id: project.definition.id,
            declared: project.definition.environment_keys,
            metadata: None,
            rows: BTreeMap::new(),
            path,
            preview: None,
            approved: BTreeSet::new(),
            busy: false,
            error: None,
        };
        view.reload(window, cx);
        view
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let root = self.root.clone();
        let id = self.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { PrivateSettings::new(&root, &id)?.metadata() })
                .await;
            _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(metadata) => this.accept_metadata(metadata, window, cx),
                    Err(_) => this.error = Some("Could not load private settings. Try again."),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn accept_metadata(&mut self, metadata: Metadata, window: &mut Window, cx: &mut Context<Self>) {
        for name in names(&self.declared, metadata.names()) {
            self.rows.entry(name).or_insert_with(|| {
                let input = cx.new(|cx| {
                    let mut state = InputState::new(window, cx);
                    state.set_masked(true, window, cx);
                    state
                });
                Row {
                    input,
                    choice: Choice::Keep,
                }
            });
        }
        self.metadata = Some(metadata);
        self.preview = None;
        self.approved.clear();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(metadata) = &self.metadata else {
            return;
        };
        if self.busy {
            return;
        }
        let revision = metadata.revision();
        let edits = replacement_edits(self.rows.iter().map(|(name, row)| {
            let value = if row.choice == Choice::Replace {
                row.input.read(cx).value().to_string()
            } else {
                String::new()
            };
            (name.clone(), row.choice, value)
        }));
        self.busy = true;
        self.error = None;
        let root = self.root.clone();
        let id = self.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    PrivateSettings::new(&root, &id)?.update(revision, edits)
                })
                .await;
            _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(metadata) => {
                        this.clear_inputs(window, cx);
                        this.accept_metadata(metadata, window, cx);
                        cx.emit(SettingsEvent::Saved);
                    }
                    Err(_) => {
                        this.error =
                            Some("Could not save. Settings may have changed; reload and try again.")
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn clear_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for row in self.rows.values_mut() {
            row.input
                .update(cx, |input, cx| input.set_value("", window, cx));
            row.choice = Choice::Keep;
        }
    }
}

fn names(declared: &[String], saved: &[String]) -> Vec<String> {
    declared
        .iter()
        .chain(saved)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn replacement_edits(
    rows: impl IntoIterator<Item = (String, Choice, String)>,
) -> BTreeMap<String, Edit> {
    rows.into_iter()
        .map(|(name, choice, value)| {
            let edit = match choice {
                Choice::Keep => Edit::Keep,
                Choice::Replace => Edit::Replace(Secret::new(value)),
                Choice::Clear => Edit::Clear,
            };
            (name, edit)
        })
        .collect()
}

fn all_approved(conflicts: &[String], approved: &BTreeSet<String>) -> bool {
    conflicts.len() == approved.len() && conflicts.iter().all(|name| approved.contains(name))
}

mod import;
mod view;

#[cfg(test)]
mod tests;
