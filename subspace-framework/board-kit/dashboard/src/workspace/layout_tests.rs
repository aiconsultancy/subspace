//! The docks yield to the pane, and nothing runs past the workspace.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, TestAppContext, TestSupportExt as _, VisualTestContext, Window, div, px,
};

use super::{PANE_MIN_WIDTH_PX, Workspace, inspector_width, navigator_width};
use crate::tokens::Tokens;

/// Rounding to whole pixels may move an edge by up to one pixel.
const TOLERANCE_PX: f32 = 1.;

/// A workspace with both docks, in a box `width` wide.
struct Shell {
    width: Pixels,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("shell-box")
            .test_support()
            .w(self.width)
            .h(px(400.))
            .child(
                Workspace::new("shell-workspace")
                    .navigator(div().size_full())
                    .pane(div().size_full())
                    .inspector(div().size_full()),
            )
    }
}

struct Frame {
    container: Bounds<Pixels>,
    navigator: Bounds<Pixels>,
    pane: Bounds<Pixels>,
    inspector: Bounds<Pixels>,
}

fn frame(width: f32, cx: &mut TestAppContext) -> (Frame, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| Shell { width: px(width) });
    let frame = cx.update(|window, cx| {
        window.render_frame(cx);
        Frame {
            container: window.find("shell-box").bounds(),
            navigator: window.find("workspace-navigator-slot").bounds(),
            pane: window.find("workspace-pane").bounds(),
            inspector: window.find("workspace-inspector-slot").bounds(),
        }
    });
    (frame, cx)
}

fn width(bounds: Bounds<Pixels>) -> f32 {
    f32::from(bounds.size.width)
}

/// The three panels sit side by side, in order, inside the container.
fn assert_tiled(frame: &Frame) {
    let edge = |pixels: Pixels| f32::from(pixels);
    assert!(edge(frame.navigator.left()) + TOLERANCE_PX >= edge(frame.container.left()));
    assert!(edge(frame.navigator.right()) <= edge(frame.pane.left()) + TOLERANCE_PX);
    assert!(edge(frame.pane.right()) <= edge(frame.inspector.left()) + TOLERANCE_PX);
    assert!(
        edge(frame.inspector.right()) <= edge(frame.container.right()) + TOLERANCE_PX,
        "the inspector stays inside the workspace"
    );
}

#[gpui_kit::test]
fn the_docks_keep_their_widths_while_there_is_room(cx: &mut TestAppContext) {
    let (frame, cx) = frame(1600., cx);
    assert_tiled(&frame);
    cx.update(|window, cx| {
        let tokens = Tokens::current(cx);
        let viewport = f32::from(window.viewport_size().width);
        let navigator = navigator_width(f32::from(tokens.navigator_width), viewport);
        let inspector = inspector_width(f32::from(tokens.inspector_width), viewport);
        assert!((width(frame.navigator) - navigator).abs() <= TOLERANCE_PX);
        assert!((width(frame.inspector) - inspector).abs() <= TOLERANCE_PX);
    });
    assert!(width(frame.pane) > PANE_MIN_WIDTH_PX);
}

#[gpui_kit::test]
fn the_docks_yield_so_the_pane_keeps_its_least_width(cx: &mut TestAppContext) {
    // Narrower than the docks' least widths (180 + 260 px) plus the pane's.
    let (frame, _) = frame(560., cx);
    assert_tiled(&frame);
    assert!(width(frame.pane) + TOLERANCE_PX >= PANE_MIN_WIDTH_PX);
    assert!(width(frame.navigator) > 0. && width(frame.inspector) > 0.);
    assert!(
        width(frame.inspector) > width(frame.navigator),
        "the docks shrink in proportion to their widths"
    );
}

#[gpui_kit::test]
fn a_workspace_narrower_than_the_pane_gives_the_pane_all_of_it(cx: &mut TestAppContext) {
    let (frame, _) = frame(200., cx);
    assert_tiled(&frame);
    assert!((width(frame.pane) - 200.).abs() <= TOLERANCE_PX);
    assert!(width(frame.navigator) <= TOLERANCE_PX);
    assert!(width(frame.inspector) <= TOLERANCE_PX);
}
