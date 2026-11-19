//! Exact filename ownership and retention before allocating another date.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use super::files;

pub(super) fn dated_path(live: &Path, date: NaiveDate) -> PathBuf {
    files::suffixed(live, &format!(".{}", date.format("%Y-%m-%d")))
}

fn date_suffix(name: &str, prefix: &str) -> Option<NaiveDate> {
    let suffix = name.strip_prefix(prefix)?.strip_prefix('.')?;
    let date = NaiveDate::parse_from_str(suffix, "%Y-%m-%d").ok()?;
    (suffix == date.format("%Y-%m-%d").to_string()).then_some(date)
}

pub(super) fn dates(live: &Path) -> io::Result<Vec<(NaiveDate, PathBuf)>> {
    let prefix = live
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "log filename must be Unicode")
        })?;
    let mut dates = Vec::new();
    for entry in fs::read_dir(files::directory(live)?)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_file()
            && !kind.is_symlink()
            && let Some(date) = date_suffix(&entry.file_name().to_string_lossy(), prefix)
        {
            dates.push((date, entry.path()));
        }
    }
    dates.sort_by_key(|(date, _)| *date);
    Ok(dates)
}

pub(super) fn prune(live: &Path, count: usize, active: Option<NaiveDate>) -> io::Result<()> {
    let mut others: Vec<_> = dates(live)?
        .into_iter()
        .filter(|(date, _)| Some(*date) != active)
        .collect();
    let keep = count.saturating_sub(usize::from(active.is_some()));
    let remove = others.len().saturating_sub(keep);
    for (_, path) in others.drain(..remove) {
        files::remove(&path)?;
    }
    Ok(())
}
