use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, TestAppContext, VisualTestContext, WindowBounds, WindowOptions, px,
    size,
};
use subspace_framework::dashboard::{Dashboard, workspace::READING_WIDTH_PX};

use crate::{Projects, Tab};

#[gpui_kit::test]
fn overview_and_milestones_center_the_issues_reading_width(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for width in [1440., 880.] {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                Default::default(),
                size(px(width), px(800.)),
            ))),
            ..Default::default()
        };
        let (window, board) = cx.update(|cx| {
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Projects::new(window, cx))
            })
            .unwrap()
        });
        let view = VisualTestContext::from_window(window, cx).into_mut();
        for tab in [Tab::Overview, Tab::Milestones] {
            board.update(view, |board, cx| {
                board.tab = tab;
                cx.notify();
            });
            view.update(|window, cx| {
                window.render_frame(cx);
                let pane = window.find("workspace-pane").bounds();
                let column = window.find("projects-record-column").bounds();
                let expected = pane.size.width.min(px(READING_WIDTH_PX));
                assert!((column.size.width - expected).abs() <= px(1.));
                assert!(
                    ((column.left() - pane.left()) - (pane.right() - column.right())).abs()
                        <= px(1.)
                );
            });
        }
    }
}
