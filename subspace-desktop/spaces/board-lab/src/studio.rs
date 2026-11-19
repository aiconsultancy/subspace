//! Disposable Studio service for the managed-runtimes workbench spike.
//!
//! Persistence is `root/presets.json` via temp-file + fsync + rename.
//! Recipe execution always goes through `EnvironmentManager::run_source`.
//! There is no fabricated worker output if an environment is missing or fails.

mod canvas;
mod controls;
mod dataset;
mod limits;
mod recipes;
mod store;

use crate::contracts::{DashboardLayout, Preset, RunResult};
use anyhow::Result;
use std::path::PathBuf;

pub use canvas::{export_canvas, import_canvas};
pub use controls::snapshot_matches;

#[derive(Clone)]
pub struct Studio {
    pub root: PathBuf,
}

impl Studio {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn presets(&self) -> Result<Vec<Preset>> {
        store::presets(&self.root)
    }

    pub fn dashboard(&self) -> Result<DashboardLayout> {
        store::dashboard(&self.root)
    }

    pub fn arrange(&self, layout: &DashboardLayout) -> Result<DashboardLayout> {
        store::arrange(&self.root, layout)
    }

    pub fn new_draft(&self, profile: &str) -> Result<Preset> {
        recipes::new_draft(&self.root, profile)
    }

    pub fn run(&self, preset: &Preset) -> Result<RunResult> {
        recipes::run(&self.root, preset)
    }

    pub fn save(&self, preset: &Preset) -> Result<Preset> {
        let _write = store::lock_presets()?;
        recipes::validate_save(&self.root, preset)?;
        store::upsert(&self.root, preset)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        store::delete(&self.root, id)
    }

    pub fn add_control(&self, preset: &mut Preset, parameter: &str) -> Result<()> {
        recipes::add_bound_control(&self.root, preset, parameter)
    }

    pub fn recipe_source(&self, preset: &Preset) -> Result<String> {
        recipes::source(&self.root, &preset.recipe)
    }

    pub fn recipe_network(&self, preset: &Preset) -> Result<bool> {
        recipes::network(&self.root, &preset.recipe)
    }
}

#[cfg(test)]
mod tests {
    use super::Studio;
    use crate::contracts::{DashboardLayout, Parameters};

    #[test]
    fn draft_does_not_create_storage_and_binds_market_defaults() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let root = directory.path().join("board-lab");
        let studio = Studio::new(root.clone());
        let draft = studio.new_draft("market").expect("market draft");
        assert_eq!(draft.recipe, "market");
        assert_eq!(draft.environment, "python-data");
        assert_eq!(draft.parameters.start, Parameters::default().start);
        assert!(!studio.recipe_source(&draft).expect("source").is_empty());
        assert!(!studio.recipe_network(&draft).expect("network"));
        assert!(!root.exists());
    }

    #[test]
    fn save_round_trip_and_dashboard_order_stay_on_disk() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let studio = Studio::new(directory.path().join("board-lab"));
        let mut draft = studio.new_draft("market").expect("draft");
        studio.add_control(&mut draft, "currency").expect("control");
        let saved = studio.save(&draft).expect("save");
        assert_eq!(saved.revision, 1);
        let listed = studio.presets().expect("presets");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, saved.id);
        let layout = studio.dashboard().expect("dashboard");
        assert_eq!(layout.order.as_slice(), std::slice::from_ref(&saved.id));
        let arranged = studio
            .arrange(&DashboardLayout {
                title: "Saved board dashboard".into(),
                mode: "wide".into(),
                order: vec![saved.id.clone()],
            })
            .expect("arrange");
        assert_eq!(arranged.mode, "wide");
        studio.delete(&saved.id).expect("delete");
        assert!(studio.presets().expect("empty").is_empty());
    }

    #[test]
    fn run_rejects_environment_mismatch_without_starting_a_worker() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let studio = Studio::new(directory.path().join("board-lab"));
        let mut draft = studio.new_draft("market").expect("draft");
        draft.environment = "missing-runtime".into();
        let error = studio.run(&draft).expect_err("mismatch");
        assert!(format!("{error:#}").contains("requires environment python-data"));
    }
}
