use super::{CopySelection, GridExport, NativeGrid};
use crate::native_grid::{GridFormat, GridScope, model::Sort, selection::Cell};
use gpui_kit::{ClipboardItem, Context, KeyDownEvent, ScrollStrategy, Window};

impl NativeGrid {
    pub(super) fn select(
        &mut self,
        cell: Cell,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.options[6] {
            return;
        }
        self.model.selection.select(cell, extend, &self.model.shown);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.options[6] {
            return;
        }
        if let Some(cell) = self.model.selection.destination(
            &event.keystroke.key,
            &self.model.shown,
            self.model.source.columns.len(),
        ) {
            self.select(cell, event.keystroke.modifiers.shift, window, cx);
            if let Some(row) = self.model.shown.iter().position(|row| *row == cell.row) {
                self.scroll.scroll_to_item(row, ScrollStrategy::Top);
            }
            self.reveal_column(cell.column);
            cx.stop_propagation();
        }
    }

    fn reveal_column(&self, column: usize) {
        let Some(content) = self.horizontal.bounds_for_item(0) else {
            return;
        };
        let viewport = self.horizontal.bounds();
        let width = content.size.width / self.model.source.columns.len() as f32;
        let mut offset = self.horizontal.offset();
        let left = width * column as f32;
        let right = left + width;
        if left < -offset.x {
            offset.x = -left;
        }
        if right > -offset.x + viewport.size.width {
            offset.x = viewport.size.width - right;
        }
        self.horizontal.set_offset(offset);
    }

    pub(super) fn copy_action(
        &mut self,
        _: &CopySelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy(GridScope::Selected, cx);
    }

    pub(super) fn copy(&mut self, scope: GridScope, cx: &mut Context<Self>) {
        if !self.options[7]
            || (scope == GridScope::Selected && self.model.selection.cells.is_empty())
        {
            return;
        }
        let source = self.model.capture(scope);
        cx.write_to_clipboard(ClipboardItem::new_string(
            source.tsv(scope != GridScope::Selected),
        ));
        self.status = "Copied".into();
        cx.notify();
    }

    pub(super) fn export(&self, scope: GridScope, format: GridFormat, cx: &mut Context<Self>) {
        cx.emit(GridExport {
            snapshot: self.model.capture(scope),
            scope,
            format,
        });
    }

    pub(super) fn sort(&mut self, column: usize, cx: &mut Context<Self>) {
        if self.options[4] {
            self.model.cycle_sort(column);
            cx.notify();
        }
    }

    pub(super) fn clear_sort(&mut self, cx: &mut Context<Self>) {
        self.model.sort = None;
        self.model.rebuild();
        cx.notify();
    }

    pub(super) fn clear_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model.filters.fill(String::new());
        for input in &self.filters {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.model.rebuild();
        cx.notify();
    }

    pub(super) fn set_option(
        &mut self,
        option: usize,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options[option] = enabled;
        match (option, enabled) {
            (4, false) => self.clear_sort(cx),
            (5, false) => self.clear_filters(window, cx),
            (6, false) => self.model.selection = Default::default(),
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn sort_label(&self, column: usize) -> String {
        let direction = match self.model.sort {
            Some(Sort::Ascending(ix)) if ix == column => " · ascending",
            Some(Sort::Descending(ix)) if ix == column => " · descending",
            _ => "",
        };
        format!("{}{direction}", self.model.source.columns[column])
    }
}
