use std::path::Path;

use super::actions::{Request, Write};
use crate::project::{LoadedProject, ProjectDefinition, ProjectRevision};
use crate::project_runtime::SessionStatus;
use crate::project_service::{InputRevision, ProjectService, shared};
use anyhow::{Result, ensure};

pub(super) struct Prepared {
    pub request: Request,
    pub summary: String,
}

pub(super) fn prepare(root: &Path, request: Request) -> Result<Prepared> {
    let service = shared(root)?;
    let summary = match &request {
        Request::Read(_) => String::new(),
        Request::Write(write) => describe(&service, write)?,
    };
    Ok(Prepared { request, summary })
}

fn current(
    service: &ProjectService,
    id: &str,
    expected: &ProjectRevision,
) -> Result<LoadedProject> {
    let project = service.load(id)?;
    ensure!(&project.revision == expected, "revision conflict");
    Ok(project)
}

fn execution_summary(
    operation: &str,
    definition: &ProjectDefinition,
    revision: &ProjectRevision,
) -> Result<String> {
    Ok(format!(
        "{operation} project {}. Definition revision {}; source revision {}. Environment {}; selected environment variable names {}; requested HTTPS destinations {}. Project code will execute.",
        definition.id,
        revision.definition,
        revision.source,
        definition.environment,
        serde_json::to_string(&definition.environment_keys)?,
        serde_json::to_string(&definition.destinations)?
    ))
}

fn input_summary(service: &ProjectService, id: &str, expected: &InputRevision) -> Result<String> {
    let presentation = service.open(id)?;
    let live = service.live_state(id)?;
    ensure!(
        live.session_id() == expected.session_id
            && live.input_revision() == expected.input_revision,
        "input revision conflict"
    );
    ensure!(
        presentation.live.session_id() == live.session_id(),
        "session changed"
    );
    ensure!(
        presentation.live.definition() == live.definition(),
        "session definition changed"
    );
    let details = match presentation.applied_revision.as_ref() {
        Some(revision) => execution_summary("Update an input in", live.definition(), revision)?,
        None => "Update a stopped project input without executing code.".into(),
    };
    Ok(format!(
        "{} Session {}; input revision {}.",
        details, expected.session_id, expected.input_revision
    ))
}

fn invoke_summary(
    service: &ProjectService,
    id: &str,
    name: &str,
    expected: &InputRevision,
) -> Result<String> {
    let presentation = service.open(id)?;
    let live = service.live_state(id)?;
    ensure!(
        presentation.status == SessionStatus::Running,
        "project is not running"
    );
    ensure!(
        live.session_id() == expected.session_id
            && live.input_revision() == expected.input_revision,
        "input revision conflict"
    );
    ensure!(
        presentation.live.session_id() == live.session_id(),
        "session changed"
    );
    ensure!(
        presentation.live.definition() == live.definition(),
        "session definition changed"
    );
    live.validate_action(name)?;
    let revision = presentation
        .applied_revision
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no applied revision"))?;
    Ok(format!(
        "{} Action {}; session {}; input revision {}.",
        execution_summary(
            "Invoke an action in",
            presentation.live.definition(),
            revision
        )?,
        name,
        expected.session_id,
        expected.input_revision
    ))
}

fn describe(service: &ProjectService, write: &Write) -> Result<String> {
    match write {
        Write::Start { id, expected } | Write::Apply { id, expected } => {
            let project = current(service, id, expected)?;
            execution_summary(
                if matches!(write, Write::Apply { .. }) {
                    "Apply and restart"
                } else {
                    "Start"
                },
                &project.definition,
                expected,
            )
        }
        Write::Invoke { id, name, expected } => invoke_summary(service, id, name, expected),
        Write::SetValue { id, expected, .. } => input_summary(service, id, expected),
        Write::Export { id, expected } => {
            let project = current(service, id, expected)?;
            let output = super::execute::export_path(service.root(), &project);
            Ok(format!(
                "Export project {} revision {} / {} to {}. Export checks known credentials, but review source for credentials before sharing.",
                id,
                expected.definition,
                expected.source,
                output.display()
            ))
        }
        _ => describe_edit(service, write),
    }
}

fn describe_edit(service: &ProjectService, write: &Write) -> Result<String> {
    match write {
        Write::Create { definition, .. } => {
            definition.validate()?;
            Ok(format!(
                "Create project {} without executing it.",
                definition.id
            ))
        }
        Write::Save {
            definition,
            expected,
        } => {
            definition.validate()?;
            current(service, &definition.id, expected)?;
            Ok(format!(
                "Save project {} without executing it.",
                definition.id
            ))
        }
        Write::EditFile {
            id, path, expected, ..
        } => {
            let project = current(service, id, expected)?;
            crate::project::validate_source_path(path)?;
            ensure!(project.definition.files.contains(path), "undeclared file");
            Ok(format!(
                "Edit declared source in project {id} without executing it."
            ))
        }
        Write::Stop { id } => {
            service.load(id)?;
            Ok(format!("Stop project {id}."))
        }
        Write::Close { id } => {
            service.load(id)?;
            Ok(format!("Close project {id}."))
        }
        _ => anyhow::bail!("unexpected project action"),
    }
}
