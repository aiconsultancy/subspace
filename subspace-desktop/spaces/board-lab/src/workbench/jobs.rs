//! Background job dispatch for [`super::Workbench`]: running the draft
//! recipe, saving and deleting presets, dashboard layout moves, and the
//! environment-manager actions (provision, pin version, add package/tool).

use gpui_kit::{AppContext as _, Context};

use crate::environments::EnvironmentManager;

use super::Workbench;

impl Workbench {
    pub(super) fn run(&mut self, cx: &mut Context<Self>) {
        if self.jobs.busy {
            return;
        }
        self.apply_parameter_inputs(cx);
        self.jobs.busy = true;
        self.jobs.last_error = None;
        self.jobs.status = "Running recipe through the managed environment...".into();
        let studio = self.studio.clone();
        let preset = self.draft.clone();
        self.jobs.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { studio.run(&preset) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.jobs.busy = false;
                match result {
                    Ok(run) => {
                        this.jobs.status = format!(
                            "Ran in {} ({} ms, {})",
                            run.runtime, run.elapsed_ms, run.dataset.provenance
                        )
                        .into();
                        this.last_result = Some(run);
                    }
                    Err(error) => {
                        this.jobs.status =
                            format!("Run failed. No unsandboxed fallback. {error:#}").into();
                        this.jobs.last_error = Some(format!("{error:#}"));
                        this.last_result = None;
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        if self.jobs.busy {
            return;
        }
        self.apply_parameter_inputs(cx);
        self.jobs.busy = true;
        self.jobs.status = "Saving preset...".into();
        let studio = self.studio.clone();
        let mut preset = self.draft.clone();
        preset.snapshot = self
            .last_result
            .as_ref()
            .filter(|result| crate::studio::snapshot_matches(&preset, &result.dataset))
            .map(|result| result.dataset.clone());
        self.jobs.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { studio.save(&preset) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.jobs.busy = false;
                match result {
                    Ok(saved) => {
                        this.jobs.status = format!("Saved revision {}", saved.revision).into();
                        this.draft = saved.clone();
                        this.presets.retain(|existing| existing.id != saved.id);
                        this.presets.push(saved);
                    }
                    Err(error) => this.jobs.status = format!("Save failed: {error:#}").into(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn delete(&mut self, id: String, cx: &mut Context<Self>) {
        if self.jobs.busy {
            return;
        }
        self.jobs.busy = true;
        let studio = self.studio.clone();
        let id_for_task = id.clone();
        self.jobs.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { studio.delete(&id_for_task) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.jobs.busy = false;
                match result {
                    Ok(()) => {
                        this.presets.retain(|preset| preset.id != id);
                        this.jobs.status = "Preset deleted".into();
                    }
                    Err(error) => this.jobs.status = format!("Delete failed: {error:#}").into(),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn provision(&mut self, id: String, cx: &mut Context<Self>) {
        self.environment_job(id, cx, |environments, id| environments.provision(&id));
    }

    pub(super) fn change_layout(
        &mut self,
        mode: Option<String>,
        movement: Option<(String, isize)>,
        cx: &mut Context<Self>,
    ) {
        if self.jobs.busy {
            return;
        }
        let mut layout = self
            .studio
            .dashboard()
            .unwrap_or_else(|_| self.dashboard_layout.clone());
        if let Some(mode) = mode {
            layout.mode = mode;
        }
        if let Some((id, direction)) = movement
            && let Some(index) = layout.order.iter().position(|current| current == &id)
        {
            let other = index
                .saturating_add_signed(direction)
                .min(layout.order.len().saturating_sub(1));
            layout.order.swap(index, other);
        }
        let studio = self.studio.clone();
        self.jobs.busy = true;
        self.jobs.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { studio.arrange(&layout) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.jobs.busy = false;
                match result {
                    Ok(layout) => {
                        this.dashboard_layout = layout;
                        this.jobs.status = "Dashboard layout saved".into();
                    }
                    Err(error) => this.jobs.status = format!("Layout failed: {error:#}").into(),
                };
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn set_version(&mut self, id: String, version: String, cx: &mut Context<Self>) {
        self.environment_job(id, cx, move |environments, id| {
            environments.set_version(&id, &version)
        });
    }

    pub(super) fn add_package(&mut self, id: String, package: String, cx: &mut Context<Self>) {
        self.environment_job(id, cx, move |environments, id| {
            environments.add_package(&id, &package)
        });
    }

    pub(super) fn add_tool(&mut self, id: String, request: String, cx: &mut Context<Self>) {
        let Some((tool, version)) = request.rsplit_once('@') else {
            self.jobs.status = "Enter a mise tool and exact version, e.g. rg@14.1.1".into();
            cx.notify();
            return;
        };
        let (tool, version) = (tool.to_owned(), version.to_owned());
        self.environment_job(id, cx, move |manager, id| {
            manager.add_tool(&id, &tool, &version)
        });
    }

    /// Runs one environment-manager call off the UI thread, then refreshes
    /// the catalog so readiness, pins and packages reflect its real result,
    /// success or failure.
    pub(super) fn environment_job(
        &mut self,
        id: String,
        cx: &mut Context<Self>,
        job: impl FnOnce(
            &EnvironmentManager,
            String,
        ) -> anyhow::Result<crate::contracts::EnvironmentInfo>
        + Send
        + 'static,
    ) {
        if self.jobs.busy {
            return;
        }
        self.jobs.busy = true;
        self.jobs.status = format!("Working on {id}...").into();
        let environments = self.environments.clone();
        self.jobs.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { job(&environments, id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.jobs.busy = false;
                match result {
                    Ok(info) => {
                        this.jobs.status = info.summary.clone().into();
                        this.selected_env = Some(info.spec.id.clone());
                        if let Some(existing) = this
                            .catalog
                            .iter_mut()
                            .find(|entry| entry.spec.id == info.spec.id)
                        {
                            *existing = info;
                        } else {
                            this.catalog.push(info);
                        }
                    }
                    Err(error) => {
                        this.jobs.status = format!("Environment action failed: {error:#}").into();
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn export_import_round_trip(&mut self, cx: &mut Context<Self>) {
        let Some(result) = self.last_result.clone() else {
            return;
        };
        match crate::studio::export_canvas(&self.draft, &result.dataset) {
            Ok(value) => match crate::studio::import_canvas(
                value,
                &format!("{} (imported copy)", self.draft.title),
            ) {
                Ok(imported) => match self.studio.save(&imported) {
                    Ok(saved) => {
                        self.presets.push(saved);
                        self.jobs.status =
                            "Exported canvas and saved an imported snapshot preset".into();
                    }
                    Err(error) => {
                        self.jobs.status = format!("Imported but could not save: {error:#}").into()
                    }
                },
                Err(error) => self.jobs.status = format!("Import failed: {error:#}").into(),
            },
            Err(error) => self.jobs.status = format!("Export failed: {error:#}").into(),
        }
        cx.notify();
    }
}
