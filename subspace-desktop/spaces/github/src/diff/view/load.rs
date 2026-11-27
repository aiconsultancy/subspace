//! Loading the diff and the text behind a hunk gap.

use gpui_kit::{Context, ScrollStrategy, Window};
use subspace_client_fleet::{Error, diff::DiffRead};
use subspace_framework::dashboard::{Tokens, prefs};

use super::super::model::{
    FileView, RowOptions, VIEWED_KEY, ViewedStore, build_rows, file_at, tree_order, viewed_id,
};
use super::read_executor::{LoadError, ReadExecutor, ReadResult, diff_error, read_diff};
use super::source::{Loaded, comparison, fleet_file, github_file};
use super::{DiffSource, DiffView, Text, code_row_height};

struct RefreshBaseline {
    live: u64,
    stale: u64,
}

impl DiffView {
    pub(crate) fn close(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.cancel_list();
        self.invalidate_pages();
        self.fleet.watch = None;
        self.close_binary(window, cx);
    }

    pub(super) fn refreshing_crew(&self) -> bool {
        self.loaded && self.load_task.is_some() && matches!(self.source, DiffSource::Fleet(_))
    }

    pub(super) fn crew_pages_suspended(&self) -> bool {
        self.refreshing_crew() || self.fleet.changed()
    }

    /// First load or same-scope refresh. The source is fixed for this view;
    /// mode and connection changes clear the snapshot at their own boundary.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let source = self.source.clone();
        let mode = self.fleet.mode;
        let fleet_client = match &source {
            DiffSource::Fleet(source) => {
                self.observe_current_crew(cx);
                Some(match source.connection(cx) {
                    Ok((client, access)) => {
                        self.set_access(Some(access), cx);
                        Ok(client)
                    }
                    Err(help) => {
                        self.set_access(None, cx);
                        Err(help)
                    }
                })
            }
            DiffSource::GitHub(_) => None,
        };
        self.cancel_list();
        self.fleet.list_read = DiffRead::default();
        let generation = self.fleet.generation;
        let baseline = RefreshBaseline {
            live: self.fleet.live_revision,
            stale: self.fleet.stale_revision,
        };
        let read = self.fleet.list_read.clone();
        self.error = None;
        let worker = ReadExecutor::start(
            cx,
            Box::new(move || read_diff(source, mode, fleet_client, &read)),
        );
        self.load_task = Some(cx.spawn(async move |view, cx| {
            let result = worker.await;
            _ = view.update(cx, |view, cx| {
                if view.fleet.generation != generation || view.check_crew_access(cx) {
                    cx.notify();
                    return;
                }
                view.load_task = None;
                view.finish_load(result, baseline, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn cancel_list(&mut self) {
        self.fleet.list_read.cancel();
        self.load_task = None;
        self.fleet.generation += 1;
    }

    fn finish_load(
        &mut self,
        result: ReadResult,
        baseline: RefreshBaseline,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(diff) => self.set_diff(diff, baseline, cx),
            Err(LoadError::Message(message)) => self.error = Some(message),
            Err(LoadError::Fleet(Error::DiffReadCancelled)) => {}
            Err(LoadError::Fleet(error)) if self.reject_crew_access(error, cx) => {}
            Err(LoadError::Fleet(error)) => {
                if error == Error::StaleDiff {
                    self.mark_crew_stale();
                }
                self.error = Some(diff_error(error));
            }
        }
    }

    pub(super) fn clear_snapshot(&mut self, cx: &mut Context<Self>) {
        self.invalidate_patch_pages();
        self.fleet.fingerprint = None;
        self.fleet.comparison = None;
        self.binary_holds = Default::default();
        self.fleet.unavailable = false;
        self.fleet.stale_page = false;
        self.fleet.unstable = false;
        self.fleet.live_changed = false;
        self.close_binary(None, cx);
        self.full_line = None;
        self.loaded = false;
        self.error = None;
        self.total = 0;
        self.files.clear();
        self.rows.clear();
        self.starts.clear();
        self.texts.clear();
        self.highlights.clear();
        self.context_highlights.clear();
        self.collapsed.clear();
        self.expanded.clear();
        self.current = 0;
        self.searching = false;
        self.pending = None;
        self.last_top = 0;
        self.scroll = gpui_kit::UniformListScrollHandle::new();
        self.tree_scroll = gpui_kit::UniformListScrollHandle::new();
    }

    fn set_diff(&mut self, diff: Loaded, baseline: RefreshBaseline, cx: &mut Context<Self>) {
        match diff {
            Loaded::GitHub(diff) => {
                if self.loaded && diff.head_sha == self.head_sha && diff.base_sha == self.base_sha {
                    return;
                }
                self.base_sha = diff.base_sha;
                self.head_sha = diff.head_sha;
                self.total = diff.total;
                self.set_files(diff.files.into_iter().map(github_file).collect(), cx);
            }
            Loaded::Fleet(diff) => {
                let unchanged =
                    self.loaded && self.fleet.fingerprint.as_ref() == Some(&diff.fingerprint);
                // An unchanged list cannot settle a page conflict that arrived
                // after this refresh started.
                if !unchanged || self.fleet.stale_revision == baseline.stale {
                    self.fleet.stale_page = false;
                }
                self.fleet.unavailable = false;
                self.fleet.comparison = Some(comparison(&diff));
                self.binary_holds.pin_to(self.fleet.mode, &diff.fingerprint);
                self.fleet
                    .accept_fingerprint(diff.fingerprint, baseline.live);
                self.fleet.unstable = diff.unstable;
                if unchanged {
                    return;
                }
                self.total = diff.total;
                self.replace_crew_files(diff.files.into_iter().map(fleet_file).collect(), cx);
            }
            Loaded::Unavailable => {
                self.clear_snapshot(cx);
                self.fleet.unavailable = true;
                self.loaded = true;
            }
        }
    }

    fn replace_crew_files(&mut self, files: Vec<FileView>, cx: &mut Context<Self>) {
        let location = Location::capture(self, cx);
        self.invalidate_patch_pages();
        self.close_binary(None, cx);
        self.full_line = None;
        self.set_files(files, cx);
        location.restore(self);
    }

    fn set_files(&mut self, files: Vec<FileView>, cx: &mut Context<Self>) {
        let paths: Vec<&str> = files.iter().map(|view| view.file.path.as_str()).collect();
        let order = tree_order(&paths);
        let mut files: Vec<_> = files.into_iter().map(Some).collect();
        self.files = order
            .into_iter()
            .filter_map(|index| files[index].take())
            .collect();
        self.loaded = true;
        self.texts.clear();
        self.highlights.clear();
        self.context_highlights.clear();
        self.expanded.clear();
        self.viewed.clear();
        if let Some(id) = self.viewed_id() {
            let store = ViewedStore::parse(prefs::get(cx, VIEWED_KEY).as_deref());
            self.viewed = store.get(&id);
        }
        self.collapsed = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| self.viewed.contains(&file.file.path))
            .map(|(index, _)| index)
            .collect();
        self.current = 0;
        self.rebuild();
    }

    pub(super) fn viewed_id(&self) -> Option<String> {
        let item = self.item()?;
        Some(viewed_id(&item.repo, item.number, &self.head_sha))
    }

    pub(super) fn rebuild(&mut self) {
        let texts = &self.texts;
        let lengths = |file: usize| match texts.get(&file) {
            Some(Text::Ready(lines)) => u32::try_from(lines.len()).ok(),
            _ => None,
        };
        let (rows, starts) = build_rows(
            &self.files,
            &RowOptions {
                layout: self.layout,
                collapsed: &self.collapsed,
                expanded: &self.expanded,
                lengths: &lengths,
            },
        );
        self.rows = rows;
        self.starts = starts;
    }
}

/// Anchor the selection and top row by file, rather than by their indices in
/// a list whose ordering and row counts can change during a refresh.
struct Location {
    selected: Option<FileKey>,
    top: Option<FileKey>,
    row: usize,
    within_file: usize,
}

type FileKey = (String, Option<String>);

impl Location {
    fn capture(view: &DiffView, cx: &Context<DiffView>) -> Self {
        let row = view
            .pending
            .unwrap_or_else(|| view.top_row(code_row_height(&Tokens::current(cx))));
        let top = file_at(&view.starts, row);
        let key = |index| view.files.get(index).map(file_key);
        Self {
            selected: key(view.current),
            top: key(top),
            row,
            within_file: view
                .starts
                .get(top)
                .map_or(0, |start| row.saturating_sub(*start)),
        }
    }

    fn restore(self, view: &mut DiffView) {
        let find = |key: &Option<FileKey>| {
            key.as_ref().and_then(|(path, previous)| {
                view.files.iter().position(|file| {
                    &file.file.path == path && &file.file.previous_path == previous
                })
            })
        };
        let top = find(&self.top);
        let selected = find(&self.selected);
        view.current = selected.or(top).unwrap_or(0);
        let row = top
            .map(|index| {
                let end = view
                    .starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or(view.rows.len());
                (view.starts[index] + self.within_file).min(end - 1)
            })
            .or_else(|| view.starts.get(view.current).copied())
            .unwrap_or_default();
        if row != self.row {
            view.pending = Some(row);
            view.scroll.scroll_to_item_strict(row, ScrollStrategy::Top);
        }
    }
}

fn file_key(file: &FileView) -> FileKey {
    (file.file.path.clone(), file.file.previous_path.clone())
}
