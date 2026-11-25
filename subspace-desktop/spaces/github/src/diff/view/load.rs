//! Loading the diff and the text behind a hunk gap.

use gpui_kit::{AppContext as _, Context, Window};
use subspace_client_fleet::{
    Error,
    diff::{DiffMode, DiffRead},
};
use subspace_framework::dashboard::prefs;

use super::super::model::{
    FileView, RowOptions, VIEWED_KEY, ViewedStore, build_rows, tree_order, viewed_id,
};
use super::source::{Loaded, fleet_file};
use super::{DiffSource, DiffView, Text};

enum LoadError {
    Message(String),
    Fleet(Error),
}

impl DiffView {
    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fleet.list_read.cancel();
        self.fleet.page_read.cancel();
        self._load = None;
        self.close_binary(Some(window), cx);
    }

    pub(crate) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_binary(Some(window), cx);
        self.load(cx);
    }

    /// Load the diff again (reload).
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let source = self.source.clone();
        let mode = self.fleet.mode;
        let fleet_client = match &source {
            DiffSource::Fleet(source) => {
                self.observe_current_crew(cx);
                self.reset_fleet_load(cx);
                Some(source.client(cx))
            }
            DiffSource::GitHub(_) => None,
        };
        let generation = self.fleet.generation;
        let baseline = self.fleet.live_revision;
        let read = self.fleet.list_read.clone();
        self.error = None;
        self._load = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { read_diff(source, mode, fleet_client, &read) })
                .await;
            _ = view.update(cx, |view, cx| {
                if view.fleet.generation != generation {
                    return;
                }
                view.finish_load(result, baseline, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn finish_load(
        &mut self,
        result: Result<Loaded, LoadError>,
        baseline: u64,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(diff) => self.set_diff(diff, baseline, cx),
            Err(LoadError::Message(message)) => self.error = Some(message),
            Err(LoadError::Fleet(Error::DiffReadCancelled)) => {}
            Err(LoadError::Fleet(error)) => {
                self.fleet.stale_page = error == Error::StaleDiff;
                self.error = Some(diff_error(error));
            }
        }
    }

    fn reset_fleet_load(&mut self, cx: &mut Context<Self>) {
        self.fleet.list_read.cancel();
        self.fleet.list_read = DiffRead::default();
        self.invalidate_pages();
        self.fleet.generation += 1;
        self.fleet.fingerprint = None;
        self.fleet.unavailable = false;
        self.fleet.stale_page = false;
        self.fleet.unstable = false;
        self.fleet.live_changed = false;
        self.close_binary(None, cx);
        self.full_line = None;
        self.loaded = false;
        self.total = 0;
        self.files.clear();
        self.rows.clear();
        self.starts.clear();
        self.highlights.clear();
        self.searching = false;
        self.pending = None;
        self.last_top = 0;
        self.scroll = gpui_kit::UniformListScrollHandle::new();
        self.tree_scroll = gpui_kit::UniformListScrollHandle::new();
    }

    fn set_diff(&mut self, diff: Loaded, baseline: u64, cx: &mut Context<Self>) {
        let files = match diff {
            Loaded::GitHub(diff) => {
                if self.loaded && diff.head_sha == self.head_sha && diff.base_sha == self.base_sha {
                    return;
                }
                self.base_sha = diff.base_sha;
                self.head_sha = diff.head_sha;
                self.total = diff.total;
                diff.files.into_iter().map(FileView::new).collect()
            }
            Loaded::Fleet(diff) => {
                self.fleet.accept_fingerprint(diff.fingerprint, baseline);
                self.fleet.unstable = diff.unstable;
                self.total = diff.total;
                diff.files.into_iter().map(fleet_file).collect()
            }
            Loaded::Unavailable => {
                self.fleet.unavailable = true;
                Vec::new()
            }
        };
        self.set_files(files, cx);
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

fn read_diff(
    source: DiffSource,
    mode: DiffMode,
    fleet_client: Option<Result<subspace_client_fleet::FleetClient, String>>,
    read: &DiffRead,
) -> Result<Loaded, LoadError> {
    match source {
        DiffSource::GitHub(source) => source
            .client
            .ok_or_else(|| LoadError::Message("GitHub is not configured.".to_owned()))?
            .diff(&source.login, &source.item.key)
            .map(Loaded::GitHub)
            .map_err(|error| LoadError::Message(error.to_string())),
        DiffSource::Fleet(source) => source
            .load(
                &fleet_client
                    .ok_or_else(|| {
                        LoadError::Message("Fleet connections are unavailable.".to_owned())
                    })?
                    .map_err(LoadError::Message)?,
                mode,
                read,
            )
            .map_err(LoadError::Fleet),
    }
}

fn diff_error(error: Error) -> String {
    match error {
        Error::Busy(Some(1)) => "Stargate is busy. Wait 1 second, then select Refresh.".to_owned(),
        Error::Busy(Some(seconds)) => {
            format!("Stargate is busy. Wait {seconds} seconds, then select Refresh.")
        }
        Error::Busy(None) => "Stargate is busy. Select Refresh to try again shortly.".to_owned(),
        other => other.to_string(),
    }
}
