//! Loading the diff and the text behind a hunk gap.

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::{
    Error,
    diff::{DiffFilePage, DiffMode, V1DiffFilePage},
};
use subspace_framework::dashboard::{Toast, prefs};

use super::super::model::{
    FileView, RowOptions, VIEWED_KEY, ViewedStore, build_rows, parse_patch, tree_order, viewed_id,
};
use super::source::{Loaded, fleet_file};
use super::{DiffEvent, DiffSource, DiffView, Text};

enum LoadError {
    Message(String),
    Fleet(Error),
}

impl DiffView {
    /// Load the diff again (reload).
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let source = self.source.clone();
        let mode = self.fleet.mode;
        let fleet_client = match &source {
            DiffSource::Fleet(source) => {
                self.reset_fleet_load();
                Some(source.client(cx))
            }
            DiffSource::GitHub(_) => None,
        };
        let generation = self.fleet.generation;
        self.error = None;
        self._load = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { read_diff(source, mode, fleet_client) })
                .await;
            _ = view.update(cx, |view, cx| {
                if view.fleet.generation != generation {
                    return;
                }
                view.finish_load(result, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn finish_load(&mut self, result: Result<Loaded, LoadError>, cx: &mut Context<Self>) {
        match result {
            Ok(diff) => self.set_diff(diff, cx),
            Err(LoadError::Message(message)) => self.error = Some(message),
            Err(LoadError::Fleet(error)) => {
                self.fleet.stale_page = error == Error::StaleDiff;
                self.error = Some(diff_error(error));
            }
        }
    }

    fn reset_fleet_load(&mut self) {
        self.fleet.generation += 1;
        self.fleet.fingerprint = None;
        self.fleet.unavailable = false;
        self.fleet.stale_page = false;
        self.fleet.unstable = false;
        self.fleet.live_changed = false;
        self.fleet.busy = None;
        self.fleet.loading.clear();
        self.binary = None;
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

    fn set_diff(&mut self, diff: Loaded, cx: &mut Context<Self>) {
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
                self.seed_crew_fingerprint(cx);
                self.fleet.fingerprint = Some(diff.fingerprint);
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

    pub(super) fn load_more(&mut self, index: usize, cx: &mut Context<Self>) {
        let (DiffSource::Fleet(source), Some(file), Some(at)) = (
            self.source.clone(),
            self.files.get(index),
            self.fleet.fingerprint.clone(),
        ) else {
            return;
        };
        let Some(page) = file.next_page.clone() else {
            return;
        };
        if self.fleet.changed() || self.fleet.loading.contains(&index) {
            return;
        }
        let client = match source.client(cx) {
            Ok(client) => client,
            Err(help) => {
                cx.emit(DiffEvent::Toast(Toast::error(help)));
                return;
            }
        };
        self.fleet.loading.insert(index);
        let (mode, generation) = (self.fleet.mode, self.fleet.generation);
        let path = file.file.path.clone();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let request = DiffFilePage {
                        path: &path,
                        page: Some(&page),
                        at: &at,
                        side: None,
                    };
                    let result = client.diff_file(source.request(mode), request)?;
                    validate_page(result, &request)
                })
                .await;
            _ = view.update(cx, |view, cx| {
                view.finish_page(generation, index, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_page(
        &mut self,
        generation: u64,
        index: usize,
        result: Result<V1DiffFilePage, Error>,
        cx: &mut Context<Self>,
    ) {
        if self.fleet.generation != generation {
            return;
        }
        self.fleet.loading.remove(&index);
        match result {
            Ok(page) => {
                self.fleet.busy = None;
                self.fleet.unstable |= page.unstable;
                apply_page(&mut self.files[index], page);
                self.highlights.remove(&index);
                self.rebuild();
            }
            Err(error) => self.fleet_page_error(error, cx),
        }
        cx.notify();
    }

    pub(super) fn fleet_page_error(&mut self, error: Error, cx: &mut Context<Self>) {
        match error {
            Error::StaleDiff => self.fleet.stale_page = true,
            Error::Busy(_) => self.fleet.busy = Some(diff_error(error)),
            Error::NotFound => cx.emit(DiffEvent::Toast(Toast::error(
                "This file is no longer available. Refresh the diff.",
            ))),
            other => cx.emit(DiffEvent::Toast(Toast::error(other.to_string()))),
        }
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

fn apply_page(file: &mut FileView, page: V1DiffFilePage) {
    if let Some(patch) = page.file.patch {
        let text = file.file.patch.get_or_insert_with(String::new);
        text.push_str(&patch);
        file.patch = parse_patch(text);
        if !file.binary {
            file.reason = None;
        }
    }
    file.next_page = page.file.patch_next_page;
}

fn read_diff(
    source: DiffSource,
    mode: DiffMode,
    fleet_client: Option<Result<subspace_client_fleet::FleetClient, String>>,
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

pub(super) fn validate_page(
    page: V1DiffFilePage,
    request: &DiffFilePage<'_>,
) -> Result<V1DiffFilePage, Error> {
    if page.fingerprint != request.at {
        return Err(Error::StaleDiff);
    }
    if page.file.path != request.path
        || (request.page.is_some() && page.file.patch_next_page.as_deref() == request.page)
    {
        return Err(Error::InvalidResponse);
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DiffFilePage, Error, V1DiffFilePage, apply_page, fleet_file, validate_page};

    fn page(fingerprint: &str, patch: &str, next: Option<&str>) -> V1DiffFilePage {
        serde_json::from_value(json!({ "fingerprint": fingerprint, "file": {
            "path": "notes.txt", "status": "added", "additions": 1, "deletions": 0,
            "patch": patch, "patchNextPage": next,
        } }))
        .unwrap()
    }

    #[test]
    fn joins_patch_pages_before_parsing_and_rejects_a_changed_checkout() {
        let request = DiffFilePage {
            path: "notes.txt",
            page: Some("next"),
            at: "snapshot",
            side: None,
        };
        let mut file = fleet_file(page("snapshot", "@@ -0,0 +1 @@\n+exa", Some("next")).file);
        let mismatch = validate_page(page("new", "mple.com\n", None), &request);
        assert_eq!(mismatch.err(), Some(Error::StaleDiff));
        let page = validate_page(page("snapshot", "mple.com\n", None), &request).unwrap();
        apply_page(&mut file, page);
        assert_eq!(file.patch.lines[0].text, "example.com");
        assert!(file.next_page.is_none());
    }
}
