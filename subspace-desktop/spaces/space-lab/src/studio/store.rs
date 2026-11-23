//! Atomic preset and dashboard-layout persistence.

use super::controls::{parse_id, validate_title};
use super::limits::{MAX_LAYOUT_BYTES, MAX_PRESET_FILE_BYTES, MAX_PRESETS};
use crate::contracts::{DashboardLayout, Preset};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

static STORE_WRITE: Mutex<()> = Mutex::new(());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize, Default)]
struct PresetStore {
    presets: Vec<Preset>,
}

pub(super) fn lock_presets() -> Result<MutexGuard<'static, ()>> {
    STORE_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("Preset writer lock failed"))
}

pub(super) fn lock_dashboard() -> Result<MutexGuard<'static, ()>> {
    STORE_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("Dashboard writer lock failed"))
}

pub(super) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub(super) fn draft_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        Utc::now().timestamp_micros(),
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

pub(super) fn presets(root: &Path) -> Result<Vec<Preset>> {
    let mut presets = load(root)?;
    presets.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(presets)
}

pub(super) fn upsert(root: &Path, preset: &Preset) -> Result<Preset> {
    let mut stored = load(root)?;
    let mut next = preset.clone();
    next.updated_at = now_rfc3339();
    if let Some(existing) = stored.iter_mut().find(|item| item.id == next.id) {
        next.revision = existing.revision.saturating_add(1);
        *existing = next.clone();
    } else {
        next.revision = next.revision.max(1);
        stored.push(next.clone());
    }
    persist(root, stored)?;
    Ok(next)
}

pub(super) fn delete(root: &Path, id: &str) -> Result<()> {
    let _write = lock_presets()?;
    parse_id(id)?;
    let mut stored = load(root)?;
    let before = stored.len();
    stored.retain(|preset| preset.id != id);
    if stored.len() == before {
        bail!("preset not found: {id}");
    }
    persist(root, stored)
}

pub(super) fn dashboard(root: &Path) -> Result<DashboardLayout> {
    let path = root.join("dashboard-layout.json");
    let mut layout = if path.exists() {
        read_layout(&path)?
    } else {
        DashboardLayout::default()
    };
    let presets = presets(root)?;
    layout
        .order
        .retain(|id| presets.iter().any(|preset| preset.id == *id));
    for preset in presets {
        if !layout.order.contains(&preset.id) {
            layout.order.push(preset.id);
        }
    }
    Ok(layout)
}

pub(super) fn arrange(root: &Path, layout: &DashboardLayout) -> Result<DashboardLayout> {
    let _write = lock_dashboard()?;
    validate_title(&layout.title)?;
    if !["grid", "wide", "rows"].contains(&layout.mode.as_str()) {
        bail!("Unknown dashboard layout mode");
    }
    if layout.order.len() > MAX_PRESETS {
        bail!("Too many dashboard cards");
    }
    validate_order(root, layout)?;
    fs::create_dir_all(root)?;
    let tmp = tmp_path(root, "dashboard");
    write_atomic(
        &tmp,
        &root.join("dashboard-layout.json"),
        &serde_json::to_vec_pretty(layout)?,
    )?;
    Ok(layout.clone())
}

fn validate_order(root: &Path, layout: &DashboardLayout) -> Result<()> {
    let known = presets(root)?;
    let mut ids = std::collections::HashSet::new();
    for id in &layout.order {
        parse_id(id)?;
        if !ids.insert(id) || !known.iter().any(|preset| preset.id == *id) {
            bail!("Unknown or duplicate dashboard card");
        }
    }
    Ok(())
}

fn read_layout(path: &Path) -> Result<DashboardLayout> {
    let bytes = fs::read(path)?;
    if bytes.len() > MAX_LAYOUT_BYTES {
        bail!("Dashboard layout exceeds 16 KiB");
    }
    Ok(serde_json::from_slice::<DashboardLayout>(&bytes)?)
}

fn load(root: &Path) -> Result<Vec<Preset>> {
    let path = root.join("presets.json");
    match fs::read(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        Ok(bytes) => parse_store(&path, &bytes),
    }
}

fn parse_store(path: &Path, bytes: &[u8]) -> Result<Vec<Preset>> {
    if bytes.len() > MAX_PRESET_FILE_BYTES {
        bail!(
            "presets.json exceeds the {MAX_PRESET_FILE_BYTES} byte limit at {}",
            path.display()
        );
    }
    let store: PresetStore =
        serde_json::from_slice(bytes).with_context(|| format!("parsing {}", path.display()))?;
    Ok(store.presets)
}

fn persist(root: &Path, presets: Vec<Preset>) -> Result<()> {
    if presets.len() > MAX_PRESETS {
        bail!("at most {MAX_PRESETS} presets can be stored");
    }
    fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    let bytes =
        serde_json::to_vec_pretty(&PresetStore { presets }).context("serializing presets.json")?;
    if bytes.len() > MAX_PRESET_FILE_BYTES {
        bail!("presets.json would exceed the {MAX_PRESET_FILE_BYTES} byte limit");
    }
    write_atomic(
        &tmp_path(root, "presets"),
        &root.join("presets.json"),
        &bytes,
    )
}

fn tmp_path(root: &Path, prefix: &str) -> PathBuf {
    root.join(format!(
        "{prefix}-{}-{}.tmp",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write_atomic(tmp: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    {
        let mut file = File::create(tmp).with_context(|| format!("creating {}", tmp.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("writing {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing {}", tmp.display()))?;
    }
    fs::rename(tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}
