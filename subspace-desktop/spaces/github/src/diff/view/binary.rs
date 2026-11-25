//! A viewport for one decoded Fleet page. Page history holds positions only;
//! switching sides or closing the viewer drops its bytes and pending read.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, IntoElement, ParentElement as _, Styled as _, Task,
    UniformListScrollHandle, Window, div,
};
use subspace_client_fleet::diff::{DiffContent, DiffRead, DiffSide};
use subspace_framework::client_github::model::GhFileStatus;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};
use subspace_framework::dashboard::{Tokens, truncated};

use super::DiffView;

mod hex;
mod preview;
mod read;
#[cfg(test)]
mod tests;

use preview::Preview;

#[derive(Clone, Default)]
struct Position {
    cursor: Option<String>,
    offset: u64,
}

fn side_control(
    binary: &BinaryView,
    status: GhFileStatus,
    changed: bool,
    tokens: &Tokens,
    cx: &Context<DiffView>,
) -> AnyElement {
    side_label(binary.side, status, changed).map_or_else(
        || side_selector(binary, tokens, cx),
        IntoElement::into_any_element,
    )
}

fn side_label(side: DiffSide, status: GhFileStatus, changed: bool) -> Option<&'static str> {
    (changed || matches!(status, GhFileStatus::Added | GhFileStatus::Removed)).then_some(match side
    {
        DiffSide::Old => "Old bytes",
        DiffSide::New => "New bytes",
    })
}

fn side_selector(binary: &BinaryView, tokens: &Tokens, cx: &Context<DiffView>) -> AnyElement {
    let file = binary.file;
    let weak = cx.entity().downgrade();
    segmented::control(
        "crew-binary-side",
        [Segment::new("Old"), Segment::new("New")],
        Some(binary.side_index()),
        tokens,
        move |index, _, window, cx| {
            _ = weak.update(cx, |view, cx| {
                view.select_binary_side(file, index, window, cx)
            });
        },
    )
    .into_any_element()
}

#[derive(Clone, Copy)]
enum Move {
    Start,
    Next,
    Previous,
}

pub(super) struct BinaryView {
    file: usize,
    side: DiffSide,
    position: Position,
    history: Vec<Position>,
    page: Option<DiffContent>,
    scroll: UniformListScrollHandle,
    read: Option<Task<()>>,
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

    fn new(file: usize, side: DiffSide) -> Self {
        Self {
            file,
            side,
            position: Position::default(),
            history: Vec::new(),
            page: None,
            scroll: UniformListScrollHandle::new(),
            read: None,
            preview: None,
            closed: DiffRead::default(),
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

    fn show(&mut self, page: DiffContent, position: Position, direction: Move) {
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
        (binary.file == file && binary.side != side && !self.fleet.changed()).then_some(side)
    }

    pub(super) fn open_binary(&mut self, file: usize, window: &mut Window, cx: &mut Context<Self>) {
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
        self.binary = Some(BinaryView::new(file, side));
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

    fn binary_controls(&self, binary: &BinaryView, tokens: &Tokens, cx: &Context<Self>) -> Div {
        let changed = self.fleet.changed();
        let status = self.files[binary.file].file.status;
        h_flex()
            .px_3()
            .gap_2()
            .flex_wrap()
            .child(side_control(binary, status, changed, tokens, cx))
            .child(paging_controls(
                binary,
                changed,
                self.binary_path(binary),
                tokens,
                cx,
            ))
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

fn paging_controls(
    binary: &BinaryView,
    changed: bool,
    path: &str,
    tokens: &Tokens,
    cx: &Context<DiffView>,
) -> Div {
    let row = h_flex().gap_2().flex_wrap();
    match (changed, binary.read.is_some(), binary.preview.is_some()) {
        (true, _, preview) => row
            .child(
                action_button(
                    "crew-binary-refresh",
                    Intent::Quiet,
                    Some(IconName::RefreshCw),
                    "Refresh diff",
                )
                .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
            )
            .when(preview, |row| row.child(preview::bytes_button(cx))),
        (false, true, _) => row.child(div().text_color(tokens.muted).child("Loading…")),
        (false, false, true) => row.child(preview::bytes_button(cx)),
        (false, false, false) => row
            .children(binary_actions(binary, cx))
            .when_some(preview::format(path), |row, format| {
                row.child(preview::open_button(format, cx))
            }),
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

fn binary_actions(binary: &BinaryView, cx: &Context<DiffView>) -> Vec<AnyElement> {
    [
        (
            Move::Start,
            binary.page.is_none(),
            "crew-binary-load",
            IconName::RefreshCw,
            "Load bytes",
        ),
        (
            Move::Previous,
            !binary.history.is_empty(),
            "crew-binary-previous",
            IconName::ChevronUp,
            "Previous page",
        ),
        (
            Move::Next,
            binary
                .page
                .as_ref()
                .is_some_and(|page| page.next_page.is_some()),
            "crew-binary-next",
            IconName::ChevronDown,
            "Next page",
        ),
    ]
    .into_iter()
    .filter(|(_, show, _, _, _)| *show)
    .map(|(direction, _, id, icon, label)| {
        action_button(id, Intent::Quiet, Some(icon), label)
            .on_click(cx.listener(move |view, _, _, cx| {
                view.read_binary(direction, cx);
            }))
            .into_any_element()
    })
    .collect()
}
