//! One Framework canvas area, independent of the selected board.
mod actions;
mod content;
mod controls;
#[cfg(test)]
mod runtime_tests;
mod table;
use super::{ChatPanel, Context, Tokens, ToolCall};
use crate::action::{Intent, action_button};
use gpui_kit::component::Selectable as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Div, DragMoveEvent, EmptyView, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, Window, div, px, relative,
};
use std::sync::Arc;
use subspace_agent::canvas::{Canvas, CanvasKind};
use subspace_lucide::IconName;

struct Resize;
const MIN_BOARD: f32 = 240.;
const MIN_CANVAS: f32 = 280.;
const HANDLE_OFFSET: f32 = -3.;
const HANDLE_WIDTH: f32 = 6.;
const THUMBNAIL_WIDTH: f32 = 144.;
const THUMBNAIL_HEIGHT: f32 = 80.;

impl ChatPanel {
    pub fn canvas_gallery(&self) -> bool {
        self.tab().canvases.gallery && !self.tab().canvases.hidden && !self.sessions_open
    }

    pub fn render_canvas(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let list = &self.tab().canvases;
        let canvas = shown_canvas(list, self.sessions_open)?.clone();
        let tokens = Tokens::current(cx);
        let width = dock_width(
            self.canvas_width,
            self.width,
            f32::from(window.viewport_size().width),
        );
        Some(
            v_flex()
                .id("chat-canvas-area")
                .test_support()
                .track_focus(&self.canvas_focus)
                .relative()
                .h_full()
                .min_h_0()
                .min_w_0()
                .flex_shrink_0()
                .bg(tokens.pane)
                .border_l_1()
                .border_color(tokens.line)
                .when(list.gallery, |frame| frame.flex_1().min_w_0())
                .when(!list.gallery, |frame| {
                    frame
                        .w(px(width))
                        .max_w(relative(self.pane_width_fraction()))
                })
                .on_key_down(cx.listener(Self::canvas_key))
                .on_drag_move::<Resize>(cx.listener(
                    |chat, event: &DragMoveEvent<Resize>, _, cx| {
                        chat.canvas_width =
                            f32::from(event.bounds.right() - event.event.position.x)
                                .max(MIN_CANVAS);
                        cx.notify();
                    },
                ))
                .child(self.canvas_header(&canvas, &tokens, cx))
                .child(self.canvas_content(&canvas, width, cx))
                .child(self.canvas_caption(&canvas, &tokens, cx))
                .when(list.gallery, |frame| {
                    frame.child(self.canvas_filmstrip(&tokens, cx))
                })
                .when(!list.gallery, |frame| {
                    frame.child(
                        div()
                            .id("canvas-resize")
                            .absolute()
                            .left(px(HANDLE_OFFSET))
                            .top_0()
                            .bottom_0()
                            .w(px(HANDLE_WIDTH))
                            .cursor_col_resize()
                            .on_drag(Resize, |_, _, _, cx| cx.new(|_| EmptyView)),
                    )
                })
                .into_any_element(),
        )
    }

    fn canvas_caption(&self, canvas: &Canvas, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        h_flex()
            .whitespace_nowrap()
            .p_1()
            .gap_1()
            .border_t_1()
            .border_color(tokens.line)
            .text_color(tokens.muted)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(canvas.caption.clone()),
            )
            .child(format!("Copilot · {}", canvas.opened_at))
            .child(
                action_button(
                    "canvas-data",
                    Intent::Neutral,
                    Some(IconName::Table),
                    if canvas.kind == CanvasKind::Chart {
                        "Data"
                    } else {
                        "Source"
                    },
                )
                .selected(self.tab().canvases.data)
                .on_click(cx.listener(|chat, _, _, cx| {
                    let list = &mut chat.tab_mut().canvases;
                    list.data = !list.data;
                    cx.notify();
                })),
            )
    }

    fn canvas_filmstrip(&self, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::Stateful<Div> {
        h_flex()
            .id("canvas-filmstrip")
            .gap_1()
            .p_1()
            .overflow_x_scroll()
            .border_t_1()
            .border_color(tokens.line)
            .children(
                self.tab()
                    .canvases
                    .items
                    .iter()
                    .enumerate()
                    .map(|(index, canvas)| {
                        v_flex()
                            .id(("canvas-film", index))
                            .role(gpui_kit::accesskit::Role::Button)
                            .aria_label(format!("Show {}: {}", canvas.kind.label(), canvas.title))
                            .w(px(THUMBNAIL_WIDTH))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .p_1()
                            .border_1()
                            .border_color(tokens.line)
                            .when(index == self.tab().canvases.selected, |preview| {
                                preview.border_color(tokens.accent)
                            })
                            .child(
                                div()
                                    .w_full()
                                    .h(px(THUMBNAIL_HEIGHT))
                                    .overflow_hidden()
                                    .child(Self::canvas_thumbnail(canvas, cx)),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(kind_icon(canvas.kind))
                                    .child(div().text_ellipsis().child(canvas.title.clone())),
                            )
                            .on_click(cx.listener(move |chat, _, _, cx| {
                                chat.tab_mut().canvases.selected = index;
                                chat.tab_mut().canvases.data = false;
                                cx.notify();
                            }))
                    }),
            )
    }

    pub(super) fn toggle_canvas_gallery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab().canvases.current().is_none() {
            return;
        }
        let list = &mut self.tab_mut().canvases;
        list.gallery = !list.gallery;
        list.hidden = false;
        if list.gallery {
            self.canvas_focus.focus(window, cx);
        } else {
            self.input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(super) fn canvas_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        if (key.modifiers.platform || key.modifiers.control) && key.modifiers.alt && key.key == "g"
        {
            self.toggle_canvas_gallery(window, cx);
            cx.stop_propagation();
            return;
        }
        if !self.tab().canvases.gallery {
            return;
        }
        if key.key == "escape" {
            self.toggle_canvas_gallery(window, cx);
            cx.stop_propagation();
        } else if self.canvas_focus.is_focused(window)
            && matches!(key.key.as_str(), "left" | "right")
        {
            self.tab_mut()
                .canvases
                .step(if key.key == "left" { -1 } else { 1 });
            cx.notify();
            cx.stop_propagation();
        }
    }

    pub(super) fn canvas_tool_card(
        &self,
        call: &ToolCall,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let id = call.detail.strip_prefix("canvasInstanceId=")?;
        let canvas = self.tab().canvas_history.get(id)?.clone();
        Some(self.canvas_card(canvas, tokens, cx))
    }

    fn canvas_card(&self, canvas: Arc<Canvas>, tokens: &Tokens, cx: &mut Context<Self>) -> Div {
        let selected = self
            .tab()
            .canvases
            .current()
            .is_some_and(|item| item.id == canvas.id);
        h_flex()
            .p_2()
            .gap_2()
            .border_1()
            .border_color(if selected { tokens.accent } else { tokens.line })
            .child(kind_icon(canvas.kind))
            .child(div().flex_1().min_w_0().text_ellipsis().child(format!(
                "{} · {}",
                canvas.kind.label(),
                canvas.title
            )))
            .child(
                action_button(
                    SharedString::from(format!("show-canvas:{}", canvas.id)),
                    Intent::Neutral,
                    Some(IconName::ExternalLink),
                    "Show in canvas",
                )
                .on_click(cx.listener(move |chat, _, _, cx| {
                    chat.send(
                        subspace_agent::Command::Chat {
                            chat: chat.tab().id,
                            command: subspace_agent::ChatCommand::OpenCanvas(canvas.clone()),
                        },
                        cx,
                    );
                    cx.notify();
                })),
            )
    }
}

fn kind_icon(kind: CanvasKind) -> subspace_lucide::SvgIcon {
    match kind {
        CanvasKind::Chart => IconName::ChartColumn,
        CanvasKind::Diagram => IconName::Network,
        CanvasKind::Drawing => IconName::Pencil,
    }
}

fn dock_width(wanted: f32, chat: f32, viewport: f32) -> f32 {
    wanted
        .min((viewport - chat - MIN_BOARD).max(MIN_CANVAS))
        .max(MIN_CANVAS)
}

pub(super) fn shown_canvas(
    list: &subspace_agent::canvas::CanvasList,
    sessions: bool,
) -> Option<&Arc<Canvas>> {
    if list.hidden || sessions {
        return None;
    }
    list.items.get(list.selected)
}
