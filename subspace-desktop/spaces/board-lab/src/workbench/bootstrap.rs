//! Initial Workbench state loaded from disk: the Studio, environment
//! manager, catalog, presets, draft, dashboard layout and recipe source.
//! Pure beyond filesystem reads; no `Window`/`Context` dependency so it is
//! directly testable.

use std::path::PathBuf;

use crate::contracts::{DashboardLayout, EnvironmentInfo, Preset, RunResult};
use crate::environments::EnvironmentManager;
use crate::studio::Studio;

use super::draft::fallback_preset;
use super::state::Profile;

pub(super) struct Bootstrap {
    pub(super) studio: Studio,
    pub(super) environments: EnvironmentManager,
    pub(super) catalog: Vec<EnvironmentInfo>,
    pub(super) presets: Vec<Preset>,
    pub(super) draft: Preset,
    pub(super) last_result: Option<RunResult>,
    pub(super) dashboard_layout: DashboardLayout,
    pub(super) recipe_source: String,
    pub(super) networked: bool,
}

/// Loads the Studio and environment manager rooted at `root`, picks an
/// initial draft (the saved market preset, else a fresh market draft, else
/// an offline fallback), and reads the values the view needs to render it.
pub(super) fn load(root: PathBuf) -> Bootstrap {
    let studio = Studio::new(root.clone());
    let environments = EnvironmentManager::new(root.join("environments"));
    let catalog = environments.catalog().unwrap_or_default();
    let presets = studio.presets().unwrap_or_default();
    let draft = presets
        .iter()
        .find(|preset| preset.recipe == "market")
        .cloned()
        .unwrap_or_else(|| {
            studio
                .new_draft(Profile::Market.key())
                .unwrap_or_else(|_| fallback_preset())
        });
    let last_result = draft.snapshot.clone().map(|dataset| RunResult {
        dataset,
        stdout: String::new(),
        runtime: "saved snapshot · press Run to refresh".into(),
        elapsed_ms: 0,
    });
    let dashboard_layout = studio.dashboard().unwrap_or_default();
    let recipe_source = studio.recipe_source(&draft).unwrap_or_default();
    let networked = studio.recipe_network(&draft).unwrap_or(false);

    Bootstrap {
        studio,
        environments,
        catalog,
        presets,
        draft,
        last_result,
        dashboard_layout,
        recipe_source,
        networked,
    }
}

#[cfg(test)]
mod tests {
    use super::load;

    #[test]
    fn missing_state_falls_back_to_a_market_draft() {
        let directory = tempfile::tempdir().expect("temporary test directory");
        let loaded = load(directory.path().join("board-lab"));
        assert_eq!(loaded.draft.recipe, "market");
        assert!(loaded.last_result.is_none());
        assert!(!loaded.catalog.is_empty());
    }
}
