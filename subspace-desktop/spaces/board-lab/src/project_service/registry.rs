use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Result, ensure};

use super::ProjectService;
use crate::project::ProjectStore;

type Services = BTreeMap<PathBuf, Arc<ProjectService>>;
static SERVICES: OnceLock<Mutex<Services>> = OnceLock::new();

/// Strong root ownership prevents closing a view or tool call from ending a session.
pub fn shared(root: &Path) -> Result<Arc<ProjectService>> {
    ensure!(root.is_absolute(), "Project service root must be absolute");
    let store = ProjectStore::new(root)?;
    let key = store.root().to_owned();
    let mut services = SERVICES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("Project service registry unavailable"))?;
    Ok(services
        .entry(key)
        .or_insert_with(|| {
            Arc::new(ProjectService {
                store,
                state: Mutex::default(),
                changes: tokio::sync::watch::channel(()).0,
            })
        })
        .clone())
}

/// Called by the application's quit hook, not when a board becomes hidden.
pub fn shutdown_all() -> Result<()> {
    let Some(registry) = SERVICES.get() else {
        return Ok(());
    };
    let services = registry
        .lock()
        .map_err(|_| anyhow::anyhow!("Project service registry unavailable"))?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let errors = services
        .iter()
        .filter_map(|service| service.shutdown().err())
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    ensure!(
        errors.is_empty(),
        "Project shutdown failed: {}",
        errors.join("; ")
    );
    Ok(())
}
