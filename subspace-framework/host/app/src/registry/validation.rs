//! Checks on what the included boards declare: settings sections, feature
//! keys and contract roles.

use subspace_core::features::{FeatureSpec, FeatureStage};
use subspace_dashboard::settings::SettingsSection;

use super::BoardRegistration;

/// Rejects invalid or duplicate settings sections and feature keys, and
/// contract declarations that `subspace_dashboard::contracts::validate` refuses.
pub(super) fn validate(boards: &[BoardRegistration]) -> Result<(), String> {
    validate_settings(boards)?;
    subspace_dashboard::contracts::validate(&super::background::declared_contracts(boards))
}

fn validate_settings(boards: &[BoardRegistration]) -> Result<(), String> {
    let mut feature_ids = std::collections::HashSet::new();
    for board in boards {
        validate_features(board.id, board.settings.features, &mut feature_ids)?;
        validate_section_shapes(board.id, board.settings.sections)?;
        let features = section_features(board.settings.sections);
        validate_features(board.id, &features, &mut feature_ids)?;
    }
    Ok(())
}

fn validate_section_shapes(board_id: &str, sections: &[SettingsSection]) -> Result<(), String> {
    let mut levels = vec![sections];
    while let Some(level) = levels.pop() {
        let mut section_ids = std::collections::HashSet::new();
        for section in level {
            let valid_id = !section.id.is_empty()
                && section
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
            if !valid_id || section.title.trim().is_empty() {
                return Err(format!(
                    "invalid settings section {} for board {board_id}",
                    section.id
                ));
            }
            if !section_ids.insert(section.id) {
                return Err(format!(
                    "duplicate settings section {} for board {board_id}",
                    section.id
                ));
            }
            levels.push(section.children);
        }
    }
    Ok(())
}

fn section_features(sections: &[SettingsSection]) -> Vec<FeatureSpec> {
    let mut features = Vec::new();
    let mut pending = vec![sections];
    while let Some(level) = pending.pop() {
        for section in level {
            features.extend_from_slice(section.features);
            pending.push(section.children);
        }
    }
    features
}

fn validate_features(
    board_id: &str,
    features: &[FeatureSpec],
    ids: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    let owner = format!("board.{board_id}.");
    for feature in features {
        let invalid = !subspace_core::features::valid_id(feature.id)
            || !feature.id.starts_with(&owner)
            || feature.title.trim().is_empty()
            || feature.description.trim().is_empty()
            || (feature.stage == FeatureStage::Experimental && feature.default_enabled);
        if invalid {
            return Err(format!(
                "invalid feature {} for board {board_id}",
                feature.id
            ));
        }
        if !ids.insert(feature.id.to_owned()) {
            return Err(format!("duplicate feature key {}", feature.id));
        }
    }
    Ok(())
}
