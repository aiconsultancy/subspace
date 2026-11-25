//! Moving between files, scrolling, viewed marks, the layout and the tree
//! search.

use gpui_kit::{Context, ScrollStrategy, Window, point, px};
use subspace_framework::dashboard::{Toast, Tokens, links, prefs};

use super::super::model::{Layout, VIEWED_KEY, ViewedStore, tree_rows};
use super::{
    CloseSearch, CopyUrl, DiffEscape, DiffEvent, DiffView, FirstFile, LastFile, NextFile,
    OpenFiles, PickSearch, PreviousFile, SCROLL_LINES, SCROLL_TOP_PX, STYLE_KEY, ScrollDown,
    ScrollUp, SearchFiles, ToggleLayout, ToggleViewed, code_row_height,
};

impl DiffView {
    // ---- Navigation -----------------------------------------------------------------

    pub(super) fn go_to(
        &mut self,
        file: usize,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.files.is_empty() {
            return;
        }
        let file = file.min(self.files.len() - 1);
        self.close_binary(window, cx);
        self.full_line = None;
        self.current = file;
        if let Some(&row) = self.starts.get(file) {
            self.scroll.scroll_to_item_strict(row, ScrollStrategy::Top);
            self.pending = Some(row);
        }
        self.tree_scroll
            .scroll_to_item(self.tree_index(file), ScrollStrategy::Nearest);
        if self.item().is_some() && self.files[file].image_only() {
            self.open_image(file, cx);
        } else {
            self.image = None;
        }
        cx.notify();
    }

    pub(super) fn tree_index(&self, file: usize) -> usize {
        let paths = self.paths();
        tree_rows(&paths, "")
            .iter()
            .position(|row| row.file == Some(file))
            .unwrap_or(0)
    }

    pub(super) fn paths(&self) -> Vec<&str> {
        self.files
            .iter()
            .map(|file| file.file.path.as_str())
            .collect()
    }

    pub(super) fn next_file(&mut self, _: &NextFile, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to(self.current + 1, Some(window), cx);
    }

    pub(super) fn previous_file(
        &mut self,
        _: &PreviousFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to(self.current.saturating_sub(1), Some(window), cx);
    }

    pub(super) fn first_file(
        &mut self,
        _: &FirstFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to(0, Some(window), cx);
    }

    pub(super) fn last_file(&mut self, _: &LastFile, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to(self.files.len().saturating_sub(1), Some(window), cx);
    }

    pub(super) fn scroll_by(&mut self, lines: f32, cx: &mut Context<Self>) {
        self.pending = None;
        let handle = self.scroll.0.borrow().base_handle.clone();
        let offset = handle.offset();
        let max = handle.max_offset();
        let row_height = code_row_height(&Tokens::current(cx));
        let y = (offset.y - px(lines * row_height))
            .min(px(SCROLL_TOP_PX))
            .max(-max.y);
        handle.set_offset(point(offset.x, y));
        cx.notify();
    }

    pub(super) fn scroll_down(&mut self, _: &ScrollDown, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_by(SCROLL_LINES, cx);
    }

    pub(super) fn scroll_up(&mut self, _: &ScrollUp, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_by(-SCROLL_LINES, cx);
    }

    /// The row at the top of the code view, from its scroll offset.
    pub(super) fn top_row(&self, row_height: f32) -> usize {
        let offset = self.scroll.0.borrow().base_handle.offset();
        (f32::from(-offset.y) / row_height).max(0.0) as usize
    }

    // ---- Viewed and layout ----------------------------------------------------------

    pub(super) fn toggle_viewed_file(
        &mut self,
        file: usize,
        advance: bool,
        cx: &mut Context<Self>,
    ) {
        let (Some(id), Some(path)) = (
            self.viewed_id(),
            self.files.get(file).map(|f| f.file.path.clone()),
        ) else {
            return;
        };
        let marking = !self.viewed.contains(&path);
        if marking {
            self.viewed.insert(path);
            self.collapsed.insert(file);
        } else {
            self.viewed.remove(&path);
            self.collapsed.remove(&file);
        }
        let mut paths: Vec<String> = self.viewed.iter().cloned().collect();
        paths.sort();
        let store = ViewedStore::parse(prefs::get(cx, VIEWED_KEY).as_deref()).with(&id, paths);
        prefs::set(cx, VIEWED_KEY, store.to_json());
        self.rebuild();
        if marking && advance {
            let next = (file + 1..self.files.len())
                .find(|&index| !self.viewed.contains(&self.files[index].file.path));
            match next {
                Some(next) => self.go_to(next, None, cx),
                None => self.go_to(file, None, cx),
            }
        } else {
            self.go_to(file, None, cx);
        }
    }

    pub(super) fn toggle_viewed(
        &mut self,
        _: &ToggleViewed,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_viewed_file(self.current, true, cx);
    }

    pub(super) fn toggle_layout(
        &mut self,
        _: &ToggleLayout,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_layout(
            match self.layout {
                Layout::Unified => Layout::Split,
                Layout::Split => Layout::Unified,
            },
            cx,
        );
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub(super) fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        self.layout = layout;
        prefs::set(
            cx,
            STYLE_KEY,
            match layout {
                Layout::Unified => "unified",
                Layout::Split => "split",
            },
        );
        self.rebuild();
        let current = self.current;
        if let Some(&row) = self.starts.get(current) {
            self.scroll.scroll_to_item_strict(row, ScrollStrategy::Top);
            self.pending = Some(row);
        }
        cx.notify();
    }

    pub(super) fn toggle_collapsed(&mut self, file: usize, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&file) {
            self.collapsed.insert(file);
        }
        self.rebuild();
        self.current = file;
        cx.notify();
    }

    pub(super) fn open_files(&mut self, _: &OpenFiles, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(url) = self.files_url() {
            links::open(&url, cx);
        }
    }

    pub(super) fn copy_url(&mut self, _: &CopyUrl, _: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = self.files_url() else {
            return;
        };
        links::copy(url.clone(), cx);
        cx.emit(DiffEvent::Toast(Toast::info(format!("Copied {url}"))));
    }

    pub(super) fn escape(&mut self, _: &DiffEscape, window: &mut Window, cx: &mut Context<Self>) {
        if self.full_line.take().is_some()
            || self.close_binary(Some(window), cx)
            || self.image.take().is_some()
        {
            cx.notify();
            return;
        }
        cx.emit(DiffEvent::Close);
    }

    pub(super) fn open_search(
        &mut self,
        _: &SearchFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.searching = true;
        self.search.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_search(
        &mut self,
        _: &CloseSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.searching = false;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn pick_search(
        &mut self,
        _: &PickSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.search.read(cx).value().to_string();
        let first = tree_rows(&self.paths(), &query)
            .into_iter()
            .find_map(|row| row.file);
        self.close_search(&CloseSearch, window, cx);
        if let Some(file) = first {
            self.go_to(file, Some(window), cx);
        }
    }
}
