use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyView, AppContext as _, Context, Div, DragMoveEvent, EmptyView,
    InteractiveElement as _, IntoElement, ParentElement, Stateful, StatefulInteractiveElement as _,
    Styled, TestSupportExt as _, div, px, relative,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::dock_panel::{BottomSpan, PanelLayout, Placement};

use super::super::SubspaceApp;

const RESIZE_OFFSET: f32 = -3.;
const RESIZE_SIZE: f32 = 6.;
/// The most of its parent workspace a dock may cover. The preferred size stays
/// as stored; only the rendered extent is limited, so the main workspace keeps
/// the rest on narrow or short windows and the dock regains its size on resize.
const MAX_DOCK_SHARE: f32 = 0.6;
/// Physical border already drawn by the frame's native `border_*_1` treatment.
const DOCK_BORDER: gpui_kit::Pixels = px(1.);

/// What the workspace body holds beside the terminal: the Space, the chat's
/// canvas and the chat. The Space is absent while the canvas gallery shows.
pub(super) struct WorkspaceParts {
    pub(super) space: Option<Div>,
    pub(super) canvas: Option<AnyElement>,
    pub(super) chat: Option<AnyElement>,
}

/// A horizontal workspace row that stretches its children to its height.
pub(super) fn row() -> Div {
    h_flex().flex_1().min_w_0().min_h_0().items_stretch()
}

impl SubspaceApp {
    /// Composes the workspace with the terminal at `layout`'s edge, moving
    /// the same panel view between placements without recreating it.
    /// Bottom beside the chat: `[[Space / terminal], chat]`; full width:
    /// `[[Space, chat] / terminal]`; left: `[[terminal, Space], chat]`;
    /// right: `[[Space, terminal], chat]`. The chat stays outermost and takes
    /// its share of the whole body; the terminal takes its share of the rest,
    /// so the Space keeps the remainder.
    pub(super) fn place_terminal(
        &self,
        parts: WorkspaceParts,
        terminal: AnyView,
        layout: PanelLayout,
        cx: &mut Context<Self>,
    ) -> Div {
        let minimum = self.terminal_minimum(cx);
        let WorkspaceParts {
            space,
            canvas,
            chat,
        } = parts;
        let space = space.map(|space| {
            space.when(minimum.is_some(), |space| {
                space
                    .min_w(gpui_kit::base::PANEL_MIN_SIZE)
                    .min_h(gpui_kit::base::PANEL_MIN_SIZE)
            })
        });
        match composition(layout, space.is_some()) {
            Composition::BesideChat => row()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .children(space)
                        .child(dock_frame(terminal, None, self.dock_bottom, minimum, cx)),
                )
                .children(canvas)
                .children(chat),
            Composition::FullWidth => v_flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(row().children(space).children(canvas).children(chat))
                .child(dock_frame(terminal, None, self.dock_bottom, minimum, cx)),
            Composition::Left => row()
                .child(
                    row()
                        .child(dock_frame(
                            terminal,
                            Some(SidePlacement::Left),
                            self.dock_side,
                            minimum,
                            cx,
                        ))
                        .children(space),
                )
                .children(canvas)
                .children(chat),
            Composition::Right => row()
                .child(row().children(space).child(dock_frame(
                    terminal,
                    Some(SidePlacement::Right),
                    self.dock_side,
                    minimum,
                    cx,
                )))
                .children(canvas)
                .children(chat),
        }
    }
}

/// How the workspace body is arranged around the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Composition {
    BesideChat,
    FullWidth,
    Left,
    Right,
}

/// The arrangement for `layout`. While the canvas gallery replaces the
/// Space there is no Space column to share, so a bottom terminal spans the
/// window.
pub(super) fn composition(layout: PanelLayout, has_space: bool) -> Composition {
    match (layout.placement, layout.span) {
        (Placement::Left, _) => Composition::Left,
        (Placement::Right, _) => Composition::Right,
        (Placement::Bottom, BottomSpan::BesideChat) if has_space => Composition::BesideChat,
        (Placement::Bottom, _) => Composition::FullWidth,
    }
}

struct DockResize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SidePlacement {
    Left,
    Right,
}

pub(super) fn dock_frame(
    dock: AnyView,
    side: Option<SidePlacement>,
    dimension: f32,
    minimum: Option<gpui_kit::Size<gpui_kit::Pixels>>,
    cx: &mut Context<SubspaceApp>,
) -> impl IntoElement {
    let frame = div()
        .id(if side.is_some() {
            "workspace-dock-side"
        } else {
            "workspace-dock-bottom"
        })
        .relative()
        .flex()
        .flex_col()
        .min_h_0()
        .min_w_0()
        .flex_shrink_0()
        .border_color(Tokens::current(cx).line)
        .child(dock);
    match side {
        Some(side) => side_frame(frame, dimension, side, minimum, cx),
        None => bottom_frame(frame, dimension, minimum, cx),
    }
    .test_support()
}

fn bottom_frame(
    frame: Stateful<Div>,
    dimension: f32,
    minimum: Option<gpui_kit::Size<gpui_kit::Pixels>>,
    cx: &mut Context<SubspaceApp>,
) -> Stateful<Div> {
    frame
        .h(px(dimension))
        .max_h(relative(MAX_DOCK_SHARE))
        .when_some(minimum, |frame, minimum| {
            frame
                .min_h(minimum.height + DOCK_BORDER)
                .min_w(minimum.width)
        })
        .border_t_1()
        .on_drag_move::<DockResize>(
            cx.listener(|app, event: &DragMoveEvent<DockResize>, _, cx| {
                app.set_dock_dimension(
                    true,
                    f32::from(event.bounds.bottom() - event.event.position.y),
                    cx,
                );
            }),
        )
        .child(
            div()
                .id("workspace-dock-bottom-resize")
                .absolute()
                .top(px(RESIZE_OFFSET))
                .h(px(RESIZE_SIZE))
                .left_0()
                .right_0()
                .cursor_row_resize()
                .on_drag(DockResize, |_, _, _, cx| cx.new(|_| EmptyView)),
        )
}

fn side_frame(
    frame: Stateful<Div>,
    dimension: f32,
    placement: SidePlacement,
    minimum: Option<gpui_kit::Size<gpui_kit::Pixels>>,
    cx: &mut Context<SubspaceApp>,
) -> Stateful<Div> {
    let handle = div()
        .id("workspace-dock-side-resize")
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(RESIZE_SIZE))
        .cursor_col_resize()
        .on_drag(DockResize, |_, _, _, cx| cx.new(|_| EmptyView));
    frame
        .w(px(dimension))
        .max_w(relative(MAX_DOCK_SHARE))
        .h_full()
        .when_some(minimum, |frame, minimum| {
            frame
                .min_w(minimum.width + DOCK_BORDER)
                .min_h(minimum.height)
        })
        .when(placement == SidePlacement::Left, |frame| frame.border_r_1())
        .when(placement == SidePlacement::Right, |frame| {
            frame.border_l_1()
        })
        .on_drag_move::<DockResize>(cx.listener(
            move |app, event: &DragMoveEvent<DockResize>, _, cx| {
                app.set_dock_dimension(
                    false,
                    side_dimension(
                        placement,
                        f32::from(event.bounds.left()),
                        f32::from(event.bounds.right()),
                        f32::from(event.event.position.x),
                    ),
                    cx,
                );
            },
        ))
        .child(match placement {
            SidePlacement::Left => handle.right(px(RESIZE_OFFSET)),
            SidePlacement::Right => handle.left(px(RESIZE_OFFSET)),
        })
}

fn side_dimension(placement: SidePlacement, left: f32, right: f32, cursor: f32) -> f32 {
    match placement {
        SidePlacement::Left => cursor - left,
        SidePlacement::Right => right - cursor,
    }
}

#[cfg(test)]
mod extent_tests;

#[cfg(test)]
mod tests {
    use subspace_dashboard::dock_panel::{BottomSpan, PanelLayout, Placement};

    use super::{Composition, SidePlacement, composition, side_dimension};

    #[test]
    fn each_placement_and_bottom_span_picks_its_composition() {
        let layout = |placement, span| PanelLayout {
            placement,
            span,
            ..PanelLayout::default()
        };
        let cases = [
            (
                Placement::Bottom,
                BottomSpan::BesideChat,
                true,
                Composition::BesideChat,
            ),
            (
                Placement::Bottom,
                BottomSpan::FullWidth,
                true,
                Composition::FullWidth,
            ),
            (
                Placement::Bottom,
                BottomSpan::BesideChat,
                false,
                Composition::FullWidth,
            ),
            (
                Placement::Left,
                BottomSpan::FullWidth,
                true,
                Composition::Left,
            ),
            (
                Placement::Right,
                BottomSpan::BesideChat,
                true,
                Composition::Right,
            ),
        ];
        for (placement, span, space, expected) in cases {
            assert_eq!(composition(layout(placement, span), space), expected);
        }
    }

    #[test]
    fn left_and_right_docks_resize_from_their_inner_edges() {
        assert_eq!(side_dimension(SidePlacement::Left, 0., 300., 220.), 220.);
        assert_eq!(
            side_dimension(SidePlacement::Right, 700., 1_000., 780.),
            220.
        );
    }
}
