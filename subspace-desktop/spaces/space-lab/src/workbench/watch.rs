//! Background polling that keeps the draft canvas, environment catalog and
//! dashboard layout in sync with changes another board tool makes on disk.

use gpui_kit::{AppContext as _, Context, Window};

use crate::contracts::{DashboardLayout, EnvironmentInfo, Preset};

use super::Workbench;

/// The three values the poll reads off the background executor each tick.
struct PolledValues {
    presets: anyhow::Result<Vec<Preset>>,
    catalog: anyhow::Result<Vec<EnvironmentInfo>>,
    layout: anyhow::Result<DashboardLayout>,
}

pub(super) fn watch_presets(
    this: &mut Workbench,
    window: &mut Window,
    cx: &mut Context<Workbench>,
) {
    let studio = this.studio.clone();
    let environments = this.environments.clone();
    this.jobs.watcher = Some(cx.spawn_in(window, async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let (studio, environments) = (studio.clone(), environments.clone());
            let values = cx
                .background_spawn(async move {
                    PolledValues {
                        presets: studio.presets(),
                        catalog: environments.catalog(),
                        layout: studio.dashboard(),
                    }
                })
                .await;
            if this
                .update_in(cx, |this, window, cx| {
                    apply_polled_values(this, values, window, cx)
                })
                .is_err()
            {
                break;
            }
        }
    }));
}

fn apply_polled_values(
    this: &mut Workbench,
    values: PolledValues,
    window: &mut Window,
    cx: &mut Context<Workbench>,
) {
    if let Ok(presets) = values.presets {
        apply_presets(this, presets, window, cx);
    }
    if let Ok(catalog) = values.catalog {
        this.catalog = catalog;
    }
    if let Ok(layout) = values.layout
        && !this.jobs.busy
    {
        this.dashboard_layout = layout;
    }
    cx.notify();
}

/// Replaces the preset list and, if the person isn't mid-edit, reopens the
/// draft in place when a board tool saved a newer revision of it.
fn apply_presets(
    this: &mut Workbench,
    presets: Vec<Preset>,
    window: &mut Window,
    cx: &mut Context<Workbench>,
) {
    let changed = presets
        .iter()
        .find(|preset| preset.id == this.draft.id && preset.revision > this.draft.revision)
        .map(|preset| preset.id.clone());
    this.presets = presets;
    if this.jobs.busy || !this.inputs_clean(cx) {
        return;
    }
    if let Some(id) = changed {
        this.open_preset(&id, window, cx);
        this.jobs.status = "Preset updated by a board tool".into();
    }
}
