use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use super::actions::{Read, Request, Write};
use crate::private_settings::PrivateSettings;
use crate::project::{LoadedProject, ProjectRevision};
use crate::project_service::{ProjectPresentation, ProjectService, shared};

pub(super) fn run(root: &Path, request: Request) -> Result<Value> {
    let service = shared(root)?;
    match request {
        Request::Read(read) => inspect(&service, read),
        Request::Write(write) => edit(&service, *write),
    }
}

fn project(project: &LoadedProject) -> Value {
    json!({"definition":project.definition, "revision":project.revision})
}

pub(super) fn state(presentation: &ProjectPresentation, live: &crate::project::LiveState) -> Value {
    let options: BTreeMap<_, _> = live
        .definition()
        .inputs
        .keys()
        .filter_map(|key| live.options(key).map(|values| (key, values)))
        .collect();
    let mut enabled = BTreeMap::new();
    let mut pending: Vec<_> = live.definition().components.iter().collect();
    while let Some(component) = pending.pop() {
        enabled.insert(&component.id, live.is_enabled(&component.id));
        pending.extend(component.children());
    }
    json!({"session_id":live.session_id(), "input_revision":live.input_revision(),
        "values":live.values(), "options":options, "enabled":enabled, "results":live.results(), "status":presentation.status,
        "applied_revision":presentation.applied_revision})
}

fn safe_state(service: &ProjectService, presentation: &ProjectPresentation) -> Result<Value> {
    let id = &presentation.working.definition.id;
    let live = service.live_state(id)?;
    ensure!(
        presentation.live.session_id() == live.session_id(),
        "session changed"
    );
    let mut output = state(presentation, &live);
    output["action_outcomes"] = serde_json::to_value(service.action_outcomes(id)?)?;
    PrivateSettings::new(service.root(), id)?.redact_readback(output)
}

fn inspect(service: &ProjectService, read: Read) -> Result<Value> {
    match read {
        Read::Schemas => Ok(
            json!({"definition":serde_json::from_str::<Value>(crate::project::DEFINITION_SCHEMA)?, "protocol":serde_json::from_str::<Value>(crate::project::PROTOCOL_SCHEMA)?, "example":serde_json::from_str::<Value>(crate::project::EXAMPLE_DEFINITION)?}),
        ),
        Read::List => {
            Ok(json!({"projects":service.list()?.iter().map(project).collect::<Vec<_>>()}))
        }
        Read::Project { id } => Ok(project(&service.load(&id)?)),
        Read::Source { id, path } => {
            crate::project::validate_source_path(&path)?;
            let loaded = service.load(&id)?;
            ensure!(loaded.definition.files.contains(&path), "undeclared source");
            let source = std::str::from_utf8(loaded.files.get(&path).context("source missing")?)?;
            ensure!(
                source.len() <= crate::project::limits::FILE_BYTES,
                "source too large"
            );
            Ok(json!({"path":path, "source":source, "revision":loaded.revision}))
        }
        Read::State { id } => {
            let presentation = service.open(&id)?;
            safe_state(service, &presentation)
        }
        Read::SettingsNames { id } => {
            service.load(&id)?;
            let metadata = PrivateSettings::new(service.root(), &id)?.metadata()?;
            Ok(json!({"revision":metadata.revision(), "names":metadata.names()}))
        }
    }
}

fn edit(service: &ProjectService, write: Write) -> Result<Value> {
    match write {
        Write::Create { definition, files } => {
            let files = files
                .into_iter()
                .map(|(name, text)| (name, text.into_bytes()))
                .collect();
            Ok(project(&service.create(&definition, files)?))
        }
        Write::Save {
            definition,
            expected,
        } => Ok(project(&service.save(&definition, &expected)?)),
        Write::EditFile {
            id,
            path,
            source,
            expected,
        } => Ok(project(&service.edit_file(
            &id,
            &path,
            source.as_bytes(),
            &expected,
        )?)),
        Write::Start { id, expected } => presentation(service, service.start(&id, &expected)?),
        Write::Apply { id, expected } => {
            presentation(service, service.apply_and_restart(&id, &expected)?)
        }
        Write::Stop { id } => {
            service.stop(&id)?;
            Ok(json!({"stopped":true}))
        }
        Write::Close { id } => {
            service.close(&id)?;
            Ok(json!({"closed":true}))
        }
        Write::SetValue {
            id,
            key,
            value,
            expected,
        } => Ok(json!({"input_revision":service.set_value(&id, &key, value, &expected)?})),
        Write::Invoke { id, name, expected } => {
            Ok(json!({"request_id":service.invoke(&id, &name, &expected)?}))
        }
        Write::Export { id, expected } => export(service, &id, &expected),
    }
}

fn presentation(service: &ProjectService, presentation: ProjectPresentation) -> Result<Value> {
    safe_state(service, &presentation)
}

pub(super) fn export_path(root: &Path, project: &LoadedProject) -> PathBuf {
    root.join("project-exports").join(format!(
        "{}-{}.zip",
        project.definition.id, project.revision.source
    ))
}

fn export(service: &ProjectService, id: &str, expected: &ProjectRevision) -> Result<Value> {
    let loaded = service.load(id)?;
    ensure!(&loaded.revision == expected, "revision conflict");
    let directory = service.root().join("project-exports");
    if !directory.exists() {
        fs::create_dir(&directory)?;
    }
    let metadata = fs::symlink_metadata(&directory)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "invalid export directory"
    );
    let path = export_path(service.root(), &loaded);
    service.export(id, expected, &path)?;
    Ok(json!({"path":path}))
}
