//! The designer and resize handles share the same persistence guard.

use gpui_kit::App;

use crate::density::{self, Density, DensityPreview};
use crate::prefs;

/// Stores density overrides unless the window is showing an appearance draft.
pub fn save_density(cx: &mut App) {
    if !density_persistence_enabled(cx) {
        return;
    }
    let json = cx.try_global::<Density>().map(Density::to_json);
    if let Some(json) = json {
        prefs::set(cx, density::STATE_KEY, json);
    }
}

fn density_persistence_enabled(cx: &App) -> bool {
    should_persist_density(
        cx.try_global::<DensityPreview>()
            .is_some_and(|preview| preview.0),
    )
}

fn should_persist_density(preview: bool) -> bool {
    !preview
}

#[cfg(test)]
mod tests {
    use super::should_persist_density;

    #[test]
    fn preview_density_is_not_persisted() {
        assert!(should_persist_density(false));
        assert!(!should_persist_density(true));
    }
}
