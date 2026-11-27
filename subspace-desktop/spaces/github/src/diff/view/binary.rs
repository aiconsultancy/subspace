//! A viewport for one decoded Fleet page. Page history holds positions only;
//! switching sides or closing the viewer drops its bytes and pending read.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Task,
    UniformListScrollHandle, Window, div,
};
use subspace_client_fleet::diff::{DiffContent, DiffRead, DiffSide};
use subspace_framework::client_github::model::GhFileStatus;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, truncated};

use super::DiffView;

mod controls;
mod hex;
mod hold;
mod preview;
mod read;
#[cfg(test)]
mod tests;

pub(super) use hold::BinaryHolds;
use preview::{Preview, PreviewRead};

#[derive(Clone, Default)]
struct Position {
    cursor: Option<String>,
    offset: u64,
}

#[derive(Clone, Copy)]
enum Move {
    Start,
    Next,
    Previous,
}

enum Retry {
    Bytes(Move),
    Preview(image::ImageFormat, PreviewRead),
}

pub(super) struct BinaryView {
    file: usize,
    side: DiffSide,
    position: Position,
    history: Vec<Position>,
    page: Option<DiffContent>,
    scroll: UniformListScrollHandle,
    read: Option<Task<()>>,
    retry: Option<Retry>,
    clock: Option<Task<()>>,
    hold_changes: Option<Task<()>>,
    preview: Option<Preview>,
    closed: DiffRead,
}

impl BinaryView {
    fn side_index(&self) -> usize {
        match self.side {
            DiffSide::Old => 0,
            DiffSide::New => 1,
        }
    }

    fn new(file: usize, side: DiffSide, closed: DiffRead) -> Self {
        Self {
            file,
            side,
            position: Position::default(),
            history: Vec::new(),
            page: None,
            scroll: UniformListScrollHandle::new(),
            read: None,
            retry: None,
            clock: None,
            hold_changes: None,
            preview: None,
            closed,
        }
    }

    fn destination(&self, direction: &Move) -> Option<Position> {
        match direction {
            Move::Start => Some(Position::default()),
            Move::Next => {
                let page = self.page.as_ref()?;
                Some(Position {
                    cursor: Some(page.next_page.clone()?),
                    offset: page.offset.checked_add(page.bytes.len() as u64)?,
                })
            }
            Move::Previous => self.history.last().cloned(),
        }
    }

    fn show(&mut self, mut page: DiffContent, position: Position, direction: Move) {
        // An older gateway can send an empty terminator at EOF. Leave the
        // last real bytes visible and remove their now-consumed cursor.
        if matches!(direction, Move::Next)
            && page.bytes.is_empty()
            && let Some(previous) = self.page.as_mut()
        {
            previous.next_page = None;
            return;
        }
        if page.bytes.is_empty() {
            page.next_page = None;
        }
        match direction {
            Move::Start => self.history.clear(),
            Move::Next => self.history.push(self.position.clone()),
            Move::Previous => {
                self.history.pop();
            }
        }
        self.position = position;
        self.page = Some(page);
        self.scroll = UniformListScrollHandle::new();
    }

    fn replace_preview(
        &mut self,
        preview: Option<Preview>,
        window: Option<&mut Window>,
        cx: &mut gpui_kit::App,
    ) {
        if let Some(previous) = std::mem::replace(&mut self.preview, preview) {
            previous.evict(window, cx);
        }
    }
}

impl Drop for BinaryView {
    fn drop(&mut self) {
        self.closed.cancel();
    }
}

impl DiffView {
    pub(super) fn cancel_binary_read(&mut self) {
        if let Some(binary) = self.binary.as_mut() {
            binary.closed.cancel_and_reset();
            binary.read = None;
            binary.retry = None;
            binary.clock = None;
        }
    }

    fn select_binary_side(
        &mut self,
        file: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(side) = self.other_binary_side(file, index) {
            self.open_binary_side(file, side, window, cx);
        }
    }

    fn other_binary_side(&self, file: usize, index: usize) -> Option<DiffSide> {
        let side = [DiffSide::Old, DiffSide::New].get(index).copied()?;
        let binary = self.binary.as_ref()?;
        (binary.file == file && binary.side != side && !self.crew_pages_suspended()).then_some(side)
    }

    pub(super) fn open_binary(&mut self, file: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.refreshing_crew() {
            return;
        }
        let Some(view) = self.files.get(file).filter(|view| view.binary) else {
            return;
        };
        let side = match view.file.status {
            GhFileStatus::Removed => DiffSide::Old,
            _ => DiffSide::New,
        };
        self.current = file;
        self.open_binary_side(file, side, window, cx);
    }

    fn open_binary_side(
        &mut self,
        file: usize,
        side: DiffSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_binary(Some(window), cx);
        let path = self.files[file].file.path.clone();
        let read = self.binary_holds.reader(path.clone(), side, cx);
        let mut binary = BinaryView::new(file, side, read);
        binary.hold_changes = Some(self.binary_holds.watch(path, side, cx));
        self.binary = Some(binary);
        self.read_binary(Move::Start, cx);
        cx.notify();
    }

    pub(super) fn close_binary(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut gpui_kit::App,
    ) -> bool {
        let Some(mut binary) = self.binary.take() else {
            return false;
        };
        binary.replace_preview(None, window, cx);
        true
    }

    pub(super) fn binary_view(
        &self,
        binary: &BinaryView,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let file = &self.files[binary.file].file;
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_2()
            .child(
                h_flex()
                    .px_3()
                    .gap_2()
                    .child(
                        truncated::text("crew-binary-path", file.path.clone())
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        action_button(
                            "crew-binary-close",
                            Intent::Quiet,
                            Some(IconName::X),
                            "Close bytes",
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.close_binary(Some(window), cx);
                            cx.notify();
                        })),
                    ),
            )
            .child(self.binary_controls(binary, tokens, cx))
            .children(binary_body(binary, tokens, cx))
            .into_any_element()
    }

    fn binary_path<'a>(&'a self, binary: &BinaryView) -> &'a str {
        let file = &self.files[binary.file].file;
        if binary.side == DiffSide::Old {
            file.previous_path.as_deref().unwrap_or(&file.path)
        } else {
            &file.path
        }
    }
}

impl DiffView {
    fn retry_binary(&mut self, cx: &mut Context<Self>) {
        if self.crew_pages_suspended() || self.binary_retry_remaining(cx) > 0 {
            return;
        }
        let Some(retry) = self.binary.as_mut().and_then(|binary| binary.retry.take()) else {
            return;
        };
        match retry {
            Retry::Bytes(direction) => self.read_binary(direction, cx),
            Retry::Preview(format, progress) => self.preview_binary(format, progress, cx),
        }
        cx.notify();
    }
}

fn binary_body(
    binary: &BinaryView,
    tokens: &Tokens,
    cx: &mut Context<DiffView>,
) -> Option<AnyElement> {
    if let Some(preview) = &binary.preview {
        return Some(preview.body(tokens));
    }
    let page = binary.page.as_ref()?;
    Some(if page.size == 0 {
        div().px_3().child("Empty file").into_any_element()
    } else {
        hex::list(binary, page, tokens, cx).into_any_element()
    })
}
