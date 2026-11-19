//! Loading the diff and the text behind a hunk gap.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::model::{DiffResponse, GhDiffFile, GhFileStatus};
use subspace_framework::dashboard::{Toast, prefs};

use super::super::model::{
    FileView, RowOptions, VIEWED_KEY, ViewedStore, build_rows, tree_order, viewed_id,
};
use super::{DiffEvent, DiffView, Text};

impl DiffView {
    /// Load the diff again (reload).
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            self.error = Some("GitHub is not configured.".into());
            return;
        };
        self.error = None;
        let (login, key) = (self.login.clone(), self.item.key.clone());
        self._load = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { client.diff(&login, &key) })
                .await;
            _ = view.update(cx, |view, cx| {
                match result {
                    Ok(diff) => view.set_diff(diff, cx),
                    Err(error) => view.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn set_diff(&mut self, diff: DiffResponse, cx: &mut Context<Self>) {
        let same = self.loaded && diff.head_sha == self.head_sha && diff.base_sha == self.base_sha;
        if same {
            return;
        }
        let paths: Vec<&str> = diff.files.iter().map(|file| file.path.as_str()).collect();
        let order = tree_order(&paths);
        let mut files: Vec<Option<GhDiffFile>> = diff.files.into_iter().map(Some).collect();
        self.files = order
            .into_iter()
            .filter_map(|index| files.get_mut(index).and_then(Option::take))
            .map(FileView::new)
            .collect();
        self.base_sha = diff.base_sha;
        self.head_sha = diff.head_sha;
        self.total = diff.total;
        self.loaded = true;
        self.texts.clear();
        self.highlights.clear();
        self.context_highlights.clear();
        self.expanded.clear();
        let store = ViewedStore::parse(prefs::get(cx, VIEWED_KEY).as_deref());
        self.viewed = store.get(&self.viewed_id());
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

    pub(super) fn viewed_id(&self) -> String {
        viewed_id(&self.item.repo, self.item.number, &self.head_sha)
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

    // ---- Hunk expansion and images --------------------------------------------------

    pub(super) fn expand(&mut self, file: usize, gap: usize, cx: &mut Context<Self>) {
        match self.texts.get(&file) {
            Some(Text::Ready(_)) => {
                self.expanded.insert((file, gap));
                self.rebuild();
                cx.notify();
            }
            Some(Text::Loading) => {}
            None => self.load_text(file, gap, cx),
        }
    }

    pub(super) fn load_text(&mut self, file: usize, gap: usize, cx: &mut Context<Self>) {
        let (Some(client), Some(view)) = (self.client.clone(), self.files.get(file)) else {
            return;
        };
        let (sha, path) = if view.file.status == GhFileStatus::Removed {
            (self.base_sha.clone(), view.file.path.clone())
        } else {
            (self.head_sha.clone(), view.file.path.clone())
        };
        self.texts.insert(file, Text::Loading);
        let (login, repo) = (self.login.clone(), self.item.repo.clone());
        cx.spawn(async move |view, cx| {
            let shown = path.clone();
            let result = cx
                .background_spawn(async move { client.file(&login, &repo, &sha, &path) })
                .await;
            _ = view.update(cx, |view, cx| {
                match result {
                    Ok(text) => {
                        let lines: Vec<String> = text.text.lines().map(str::to_string).collect();
                        view.texts.insert(file, Text::Ready(Arc::new(lines)));
                        view.expanded.insert((file, gap));
                        view.rebuild();
                    }
                    Err(error) => {
                        view.texts.remove(&file);
                        cx.emit(DiffEvent::Toast(Toast::error(format!(
                            "Cannot load {shown}: {error}"
                        ))));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
