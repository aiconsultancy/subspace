//! Bounded JSON Lines chunks for the most recent local dates with writes.
//! The dated file is canonical; the stable live file mirrors its current chunk.

mod files;
mod lock;
mod marker;
mod migration;
mod retention;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Local, NaiveDate};

pub use marker::generation;

const DEFAULT_MAX_BYTES: u64 = 5 << 20;
const DEFAULT_MAX_DATES: usize = 5;

/// A date's chunk bound and the number of dates retained, including the active date.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    max_bytes: u64,
    max_dates: usize,
}

impl Limits {
    pub fn new(max_file_bytes: u64, max_dates: usize) -> Self {
        Self {
            max_bytes: max_file_bytes,
            max_dates: max_dates.max(1),
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_dates: DEFAULT_MAX_DATES,
        }
    }
}

/// One writer, externally synchronized when shared between recorders.
pub struct RunDayLog {
    live: PathBuf,
    limits: Limits,
    clock: Arc<dyn Fn() -> NaiveDate + Send + Sync>,
    active: Option<NaiveDate>,
    generation: Option<String>,
}

impl RunDayLog {
    pub fn open(live_path: &Path, limits: Limits) -> io::Result<Self> {
        Self::open_with_clock(live_path, limits, Arc::new(|| Local::now().date_naive()))
    }

    /// Supplies date selection without changing callers' serialized timestamps.
    pub fn open_with_clock(
        live_path: &Path,
        limits: Limits,
        clock: Arc<dyn Fn() -> NaiveDate + Send + Sync>,
    ) -> io::Result<Self> {
        files::prepare(live_path, limits.max_bytes)?;
        let _guard = lock::acquire(live_path)?;
        migration::once(live_path, limits);
        retention::prune(live_path, limits.max_dates, None)?;
        files::bound_existing(live_path, limits.max_bytes)?;
        Ok(Self {
            live: live_path.to_path_buf(),
            limits,
            clock,
            active: None,
            generation: None,
        })
    }

    /// Appends one complete line, rejecting oversized input before any rollover.
    /// A filesystem failure drops this write; it never enables unbounded appends.
    pub fn append(&mut self, complete_line: &[u8]) -> io::Result<()> {
        validate_line(complete_line, self.limits.max_bytes)?;
        let _guard = lock::acquire(&self.live)?;
        let date = (self.clock)();
        let dated = retention::dated_path(&self.live, date);
        files::regular(&dated)?;
        retention::prune(&self.live, self.limits.max_dates, Some(date))?;
        self.prepare_chunk(date, &dated, complete_line.len() as u64)?;
        files::append(&dated, complete_line, self.limits.max_bytes)?;
        files::append(&self.live, complete_line, self.limits.max_bytes)
    }

    pub fn live_path(&self) -> &Path {
        &self.live
    }

    fn prepare_chunk(&mut self, date: NaiveDate, dated: &Path, bytes: u64) -> io::Result<()> {
        let size = files::size(dated)?;
        let rollover = exceeds_chunk(size, bytes, self.limits.max_bytes);
        truncate_if_full(dated, rollover)?;
        let current = generation(&self.live)?;
        let state = ChunkState {
            date,
            generation: current,
            sizes: (files::size(&self.live)?, files::size(dated)?),
            rollover,
        };
        self.restore_if_changed(state, dated)
    }

    fn restore_if_changed(&mut self, state: ChunkState, dated: &Path) -> io::Result<()> {
        if state.needs_restore(self.active, self.generation.as_deref()) {
            self.restore(state.date, dated)?;
        }
        Ok(())
    }

    fn restore(&mut self, date: NaiveDate, dated: &Path) -> io::Result<()> {
        let chunk = files::read_chunk(dated, self.limits.max_bytes)?;
        let token = marker::reset(&self.live, || files::replace(&self.live, &chunk))?;
        self.active = Some(date);
        self.generation = Some(token);
        Ok(())
    }
}

struct ChunkState {
    date: NaiveDate,
    generation: Option<String>,
    sizes: (u64, u64),
    rollover: bool,
}

impl ChunkState {
    fn needs_restore(&self, active: Option<NaiveDate>, generation: Option<&str>) -> bool {
        active != Some(self.date)
            || generation != self.generation.as_deref()
            || self.rollover
            || self.sizes.0 != self.sizes.1
    }
}

fn exceeds_chunk(size: u64, bytes: u64, limit: u64) -> bool {
    size > limit.saturating_sub(bytes)
}

fn truncate_if_full(path: &Path, full: bool) -> io::Result<()> {
    if full {
        files::truncate(path)?;
    }
    Ok(())
}

fn validate_line(line: &[u8], limit: u64) -> io::Result<()> {
    if line.len() as u64 > limit
        || !line.ends_with(b"\n")
        || line[..line.len().saturating_sub(1)].contains(&b'\n')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "log record must be one complete line within the byte limit",
        ));
    }
    Ok(())
}

/// Clears managed history without unlinking live or allowing legacy reimport.
/// Writers reopen the dated path on every append, so an open writer can resume.
pub fn clear(live_path: &Path) -> io::Result<()> {
    if !files::directory(live_path)?.exists() {
        return Ok(());
    }
    let _guard = lock::acquire(live_path)?;
    let dates = retention::dates(live_path)?;
    marker::reset(live_path, || clear_files(live_path, dates)).map(drop)
}

fn clear_files(live_path: &Path, dates: Vec<(NaiveDate, PathBuf)>) -> io::Result<()> {
    files::truncate(live_path)?;
    for (_, path) in dates {
        files::remove(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
