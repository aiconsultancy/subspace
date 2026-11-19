//! One-time, best-effort import of bounded complete legacy JSON records.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, BufRead as _, BufReader, Read as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDate};

use super::{Limits, files, marker, retention};

const LEGACY_ARCHIVES: usize = 5;

struct Source {
    path: PathBuf,
    modified: SystemTime,
    index: usize,
}

struct Prepared {
    sources: Vec<Source>,
    chunk: Vec<u8>,
}

pub(super) fn once(live: &Path, limits: Limits) {
    match marker::exists(live) {
        Ok(true) => repair_marker(live),
        Ok(false) => {
            let _ = import(live, limits);
            let _ = marker::advance(live);
        }
        Err(_) => {}
    }
}

fn repair_marker(live: &Path) {
    if marker::generation(live).is_err() {
        let _ = marker::advance(live);
    }
}

fn sources(live: &Path) -> BTreeMap<NaiveDate, Vec<Source>> {
    let mut dates: BTreeMap<NaiveDate, Vec<Source>> = BTreeMap::new();
    for index in (0..=LEGACY_ARCHIVES).rev() {
        let path = if index == 0 {
            live.to_path_buf()
        } else {
            files::suffixed(live, &format!(".{index}"))
        };
        let Ok(Some(metadata)) = files::regular(&path) else {
            continue;
        };
        if metadata.len() == 0 {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let date = DateTime::<Local>::from(modified).date_naive();
        dates.entry(date).or_default().push(Source {
            path,
            modified,
            index,
        });
    }
    for group in dates.values_mut() {
        group.sort_by_key(|source| (source.modified, std::cmp::Reverse(source.index)));
    }
    dates
}

fn import(live: &Path, limits: Limits) -> io::Result<()> {
    let sources = prepare_sources(sources(live), limits.max_bytes);
    let existing = retention::dates(live)?;
    let selected: BTreeSet<_> = sources
        .keys()
        .copied()
        .chain(existing.iter().map(|(date, _)| *date))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .rev()
        .take(limits.max_dates)
        .collect();
    remove_unselected(&existing, &selected)?;
    retire_unselected_sources(live, &sources, &selected);
    for (date, group) in pending_sources(sources, &selected, &existing) {
        let _ = import_chunk(live, date, &group.sources, &group.chunk);
    }
    Ok(())
}

fn retire_unselected_sources(
    live: &Path,
    sources: &BTreeMap<NaiveDate, Prepared>,
    selected: &BTreeSet<NaiveDate>,
) {
    for group in excluded_groups(sources, selected) {
        remove_archives(live, &group.sources);
    }
}

fn excluded_groups<'a>(
    sources: &'a BTreeMap<NaiveDate, Prepared>,
    selected: &BTreeSet<NaiveDate>,
) -> Vec<&'a Prepared> {
    sources
        .iter()
        .filter(|(date, _)| !selected.contains(date))
        .map(|(_, group)| group)
        .collect()
}

fn remove_unselected(
    existing: &[(NaiveDate, PathBuf)],
    selected: &BTreeSet<NaiveDate>,
) -> io::Result<()> {
    for (date, path) in existing {
        if !selected.contains(date) {
            files::remove(path)?;
        }
    }
    Ok(())
}

fn pending_sources(
    sources: BTreeMap<NaiveDate, Prepared>,
    selected: &BTreeSet<NaiveDate>,
    existing: &[(NaiveDate, PathBuf)],
) -> BTreeMap<NaiveDate, Prepared> {
    sources
        .into_iter()
        .filter(|(date, _)| {
            selected.contains(date) && !existing.iter().any(|(present, _)| present == date)
        })
        .collect()
}

fn prepare_sources(
    sources: BTreeMap<NaiveDate, Vec<Source>>,
    limit: u64,
) -> BTreeMap<NaiveDate, Prepared> {
    sources
        .into_iter()
        .filter_map(|(date, group)| prepare_group(date, group, limit))
        .collect()
}

fn prepare_group(
    date: NaiveDate,
    sources: Vec<Source>,
    limit: u64,
) -> Option<(NaiveDate, Prepared)> {
    let chunks = source_chunks(&sources, limit).ok()?;
    let chunk = merge_chunks(&chunks, limit);
    nonempty_group(date, sources, chunk)
}

fn nonempty_group(
    date: NaiveDate,
    sources: Vec<Source>,
    chunk: Vec<u8>,
) -> Option<(NaiveDate, Prepared)> {
    (!chunk.is_empty()).then_some((date, Prepared { sources, chunk }))
}

fn import_chunk(live: &Path, date: NaiveDate, sources: &[Source], chunk: &[u8]) -> io::Result<()> {
    if chunk.is_empty() {
        return Ok(());
    }
    let path = retention::dated_path(live, date);
    files::replace(&path, chunk)?;
    remove_archives(live, sources);
    Ok(())
}

fn source_chunks(sources: &[Source], limit: u64) -> io::Result<Vec<Vec<u8>>> {
    sources
        .iter()
        .map(|source| bounded_records(&source.path, limit))
        .collect()
}

fn merge_chunks(chunks: &[Vec<u8>], limit: u64) -> Vec<u8> {
    let mut chunk = Vec::new();
    for records in chunks {
        for line in records.split_inclusive(|byte| *byte == b'\n') {
            if chunk.len() as u64 + line.len() as u64 > limit {
                chunk.clear();
            }
            chunk.extend_from_slice(line);
        }
    }
    chunk
}

fn remove_archives(live: &Path, sources: &[Source]) {
    for source in sources {
        if source.path != live {
            let _ = files::remove(&source.path);
        }
    }
}

pub(super) fn bounded_records(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    files::regular(path)?;
    collect_records(File::open(path)?, limit)
}

fn collect_records(file: File, limit: u64) -> io::Result<Vec<u8>> {
    let mut reader = BufReader::new(file);
    let mut chunk = Vec::new();
    loop {
        let mut line = Vec::new();
        let count = reader
            .by_ref()
            .take(limit.saturating_add(1))
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            reader.skip_until(b'\n')?;
            continue;
        }
        if line.len() as u64 <= limit
            && serde_json::from_slice::<serde_json::Value>(&line)
                .is_ok_and(|value| value.is_object())
        {
            if chunk.len() as u64 + line.len() as u64 > limit {
                chunk.clear();
            }
            chunk.extend_from_slice(&line);
        }
    }
    Ok(chunk)
}
