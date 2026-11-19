use anyhow::{Context as _, Result, ensure};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::{AppContext as _, Context, ParentElement as _, Window};
use subspace_framework::dashboard::action::{Intent, dialog_button};
use subspace_framework::dashboard::panel::IconName;

use super::{Outcome, ProjectBrowser};
use crate::project::{ProjectDefinition, ProjectRevision};
use crate::project_settings::{ProjectSettings, SettingsEvent};

impl ProjectBrowser {
    pub(super) fn apply_observation(
        &mut self,
        result: Result<(
            crate::project_service::ProjectPresentation,
            Vec<crate::project_service::ActionOutcome>,
        )>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((presentation, outcomes)) => {
                self.accept(presentation, window, cx);
                self.observe_outcomes(outcomes, cx);
            }
            Err(error) => self.fail(error.to_string(), cx),
        }
    }

    pub(super) fn open(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.run(window, cx, move |service| {
            Ok(Outcome::Opened(Box::new(service.open(&id)?)))
        });
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id() else {
            return;
        };
        self.run(window, cx, move |service| {
            service.close(&id)?;
            Ok(Outcome::Closed)
        });
    }

    pub(super) fn stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id() else {
            return;
        };
        self.run(window, cx, move |service| {
            service.stop(&id)?;
            Ok(Outcome::Opened(Box::new(service.open(&id)?)))
        });
    }

    pub(super) fn confirm_launch(
        &self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.presentation else {
            return;
        };
        let id = project.working.definition.id.clone();
        let revision = project.working.revision.clone();
        let disclosure = launch_disclosure(&project.working.definition, &revision);
        let owner = cx.weak_entity();
        let label = if replace { "Apply & Restart" } else { "Start" };
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let id = id.clone();
            let revision = revision.clone();
            dialog
                .title(format!("{label} project “{id}”?"))
                .child(disclosure.clone())
                .footer(confirmation_footer(label))
                .on_ok(move |_, window, cx| {
                    _ = owner.update(cx, |this, cx| {
                        if this.selected_id().as_deref() != Some(&id) {
                            return;
                        }
                        let id = id.clone();
                        let revision = revision.clone();
                        this.run(window, cx, move |service| {
                            let presentation = if replace {
                                service.apply_and_restart(&id, &revision)?
                            } else {
                                service.start(&id, &revision)?
                            };
                            Ok(Outcome::Opened(Box::new(presentation)))
                        });
                    });
                    true
                })
        });
    }

    pub(super) fn show_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.presentation else {
            return;
        };
        let settings = cx
            .new(|cx| ProjectSettings::new(self.root.clone(), project.working.clone(), window, cx));
        self.settings_subscription =
            Some(
                cx.subscribe_in(&settings, window, |this, _, event, window, cx| {
                    match event {
                        SettingsEvent::Saved => {
                            this.status =
                                "Private settings saved. Apply & Restart to use changes.".into()
                        }
                        SettingsEvent::Close => window.close_dialog(cx),
                    }
                    cx.notify();
                }),
            );
        self.settings = Some(settings.clone());
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog
                .title("Project settings")
                .child(settings.clone())
                .on_close(move |_, _, cx| {
                    _ = owner.update(cx, |this, cx| {
                        this.settings = None;
                        this.settings_subscription = None;
                        cx.notify();
                    });
                })
        });
    }

    pub(super) fn edit(
        &mut self,
        editor: crate::editors::InstalledEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.presentation else {
            return;
        };
        let directory = project.working.directory.clone();
        self.run(window, cx, move |_| {
            editor.open_project(&directory)?;
            Ok(Outcome::Notice(format!(
                "Opened project in {}. Saved edits do not restart its worker.",
                editor.title
            )))
        });
    }

    pub(super) fn confirm_export(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.presentation else {
            return;
        };
        let id = project.working.definition.id.clone();
        let revision = project.working.revision.clone();
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let id = id.clone();
            let revision = revision.clone();
            dialog.title(format!("Export “{id}”?"))
                .child("Review source for hardcoded credentials before sharing. Exact configured credentials are rejected, but transformed or unknown secrets cannot be detected reliably. Only declared shareable files are exported; private settings and destination grants are excluded. Existing archives are never replaced.")
                .footer(confirmation_footer("Export"))
                .on_ok(move |_, window, cx| {
                    _ = owner.update(cx, |this, cx| {
                        let id = id.clone();
                        let revision = revision.clone();
                        this.run(window, cx, move |service| {
                            let output = export_path(service.root(), &id, &revision)?;
                            service.export(&id, &revision, &output)?;
                            Ok(Outcome::Notice(format!("Exported {}", output.display())))
                        });
                    });
                    true
                })
        });
    }
}

pub(super) fn launch_disclosure(
    definition: &ProjectDefinition,
    revision: &ProjectRevision,
) -> String {
    format!(
        "Definition revision: {}\nSource revision: {}\nEnvironment: {}\nSelected variable names: {}\nDestination grants: {}\nProject code can read and transmit selected values. Execution remains limited to the managed runtime policy. Saving alone does not authorize execution.",
        revision.definition,
        revision.source,
        definition.environment,
        if definition.environment_keys.is_empty() {
            "None".into()
        } else {
            definition.environment_keys.join(", ")
        },
        if definition.destinations.is_empty() {
            "None".into()
        } else {
            definition.destinations.join(", ")
        },
    )
}

fn confirmation_footer(label: &'static str) -> DialogFooter {
    DialogFooter::new()
        .child(DialogClose::new().child(dialog_button(
            "project-confirm-cancel",
            Intent::Neutral,
            Some(IconName::X),
            "Cancel",
        )))
        .child(DialogAction::new().child(dialog_button(
            "project-confirm-accept",
            Intent::Primary,
            Some(IconName::Check),
            label,
        )))
}

fn export_path(
    root: &std::path::Path,
    id: &str,
    revision: &ProjectRevision,
) -> Result<std::path::PathBuf> {
    ensure!(
        id.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "Invalid project ID"
    );
    ensure!(
        revision.source.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Invalid source revision"
    );
    let root = root.canonicalize()?;
    let directory = root.join("project-exports");
    if !directory.exists() {
        match std::fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let metadata =
        std::fs::symlink_metadata(&directory).context("Cannot inspect project export directory")?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Unsafe project export directory"
    );
    ensure!(
        directory.canonicalize()?.parent() == Some(root.as_path()),
        "Export path escaped root"
    );
    Ok(directory.join(format!("{id}-{}.zip", revision.source)))
}
