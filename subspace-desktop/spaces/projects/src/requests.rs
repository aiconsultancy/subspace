//! Background reads and writes for the current project.

#[cfg(test)]
mod tests;

use gpui_kit::{AppContext as _, Context, Window};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::projects::{MilestoneWriteError, PlanningProject};
use subspace_api_client::{Client, Error, signed_in_generation};
use subspace_framework::dashboard::{DashboardEvent, Toast};

use crate::{Projects, Record, Write};

// qual:allow(coupling, oi) reason: "The I/O methods bind background requests to the Space's selected account and project."
impl Projects {
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.editor.is_some() || self.write_task.is_some() {
            return;
        }
        self.initiatives.update(cx, Catalog::refresh);
        let account = self.account;
        let read = cx.background_spawn(async { Client::new()?.planning_projects() });
        self.directory_task = Some(cx.spawn(async move |this, cx| {
            let result = read.await;
            _ = this.update(cx, |this, cx| {
                if signed_in_generation() == account {
                    this.apply_directory(result, cx);
                }
            });
        }));
        cx.notify();
    }

    fn apply_directory(
        &mut self,
        result: Result<Vec<PlanningProject>, Error>,
        cx: &mut Context<Self>,
    ) {
        self.directory_task = None;
        self.projects = result.map(|mut rows| {
            rows.sort_by_cached_key(|project| (project.archived, project.name.to_lowercase()));
            rows
        });
        let previous = self.selected.clone();
        self.reconcile_selection(cx);
        if self.selected == previous {
            self.read_record(cx);
        }
        cx.notify();
    }

    pub(super) fn read_record(&mut self, cx: &mut Context<Self>) {
        self.record_task = None;
        let Some(id) = self.selected.clone() else {
            return;
        };
        let account = self.account;
        let read = cx.background_spawn(async move { read_project(&id) });
        self.record_task = Some(cx.spawn(async move |this, cx| {
            let result = read.await;
            _ = this.update(cx, |this, cx| {
                if signed_in_generation() != account {
                    return;
                }
                this.apply_record(result, cx);
            });
        }));
    }

    fn apply_record(&mut self, result: Result<Record, Error>, cx: &mut Context<Self>) {
        self.record_task = None;
        if let Ok(record) = &result {
            if record.milestones.is_ok() && self.editor.is_none() {
                self.blocked_write = false;
                self.error = None;
            }
            if let Ok(rows) = &mut self.projects
                && let Some(row) = rows.iter_mut().find(|row| row.id == record.project.id)
            {
                row.clone_from(&record.project);
            }
        }
        self.record = Some(result);
        self.reconcile_selection(cx);
        cx.notify();
    }

    pub(super) fn may_write(&self) -> bool {
        !self.blocked_write
            && self.directory_task.is_none()
            && self.record_task.is_none()
            && self.write_task.is_none()
            && self.record.as_ref().is_some_and(|record| {
                record.as_ref().is_ok_and(|record| {
                    self.in_scope(&record.project)
                        && !record.project.archived
                        && record.milestones.is_ok()
                })
            })
    }

    pub(super) fn send(&mut self, write: Write, window: &mut Window, cx: &mut Context<Self>) {
        if !self.may_write() || self.account != signed_in_generation() {
            return;
        }
        let Some(id) = self.selected.clone() else {
            return;
        };
        let account = self.account;
        let initiative = self.initiative_id.clone();
        let send = cx.background_spawn(async move {
            let client = Client::new()?;
            if account != signed_in_generation() {
                return Err(MilestoneWriteError::Read(Error::SignInNeeded));
            }
            if let Some(initiative) = initiative
                && client.planning_project(&id)?.initiative_id.as_ref() != Some(&initiative)
            {
                return Err(MilestoneWriteError::Conflict);
            }
            match write {
                Write::Add(fields) => client.add_milestone(&id, &fields),
                Write::Change(row, change) => client.change_milestone(&id, &row, change),
            }
        });
        self.error = None;
        self.write_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = send.await;
            _ = this.update_in(cx, |this, window, cx| {
                if signed_in_generation() == account {
                    this.finish_write(result, window, cx);
                }
            });
        }));
        cx.notify();
    }

    fn finish_write(
        &mut self,
        result: Result<(), MilestoneWriteError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.write_task = None;
        let previous = self.selected.clone();
        match result {
            Ok(()) => {
                if self.editor.take().is_some() {
                    self.focus.focus(window, cx);
                }
                self.blocked_write = true;
                self.reconcile_selection(cx);
                if self.selected == previous {
                    self.read_record(cx);
                }
            }
            Err(error) => {
                self.blocked_write = matches!(
                    error,
                    MilestoneWriteError::Conflict | MilestoneWriteError::Unconfirmed
                );
                self.reconcile_selection(cx);
                if self.selected == previous {
                    self.error = Some(error.to_string());
                } else {
                    cx.emit(DashboardEvent::Toast(Toast::error(error.to_string())));
                }
            }
        }
        cx.notify();
    }
}

fn read_project(id: &str) -> Result<Record, Error> {
    let client = Client::new()?;
    let project = client.planning_project(id)?;
    let milestones = client.project_milestones(id);
    let update = client.latest_project_update(id);
    Ok(Record {
        project,
        milestones,
        update,
    })
}
