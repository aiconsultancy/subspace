//! Where the toolbar's rows and controls land, and the layout rules they keep.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Pixels, TestAppContext, WindowHandle, px};

use super::SubspaceApp;

#[derive(Debug)]
pub(super) struct ToolbarGeometry {
    window: Bounds<Pixels>,
    toolbar: Bounds<Pixels>,
    middle: Bounds<Pixels>,
    globals: Bounds<Pixels>,
    tabs: Option<Bounds<Pixels>>,
    board: Bounds<Pixels>,
    pub(super) board_row: Option<Bounds<Pixels>>,
}

pub(super) fn collect_toolbar_geometry(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
) -> ToolbarGeometry {
    cx.update_window((*window).into(), |_, window, _| ToolbarGeometry {
        window: window.bounds(),
        toolbar: window.find("app-toolbar").bounds(),
        middle: window.find("app-toolbar-row").bounds(),
        globals: window.find("global-toolbar").bounds(),
        tabs: window.try_find("dashboard-tabs").map(|tabs| tabs.bounds()),
        board: window.find("dashboard-toolbar").bounds(),
        board_row: window.try_find("board-toolbar-row").map(|row| row.bounds()),
    })
    .unwrap()
}

pub(super) fn assert_toolbar_geometry(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    width: f32,
    has_tabs: bool,
) {
    let geometry = collect_toolbar_geometry(window, cx);
    assert_board_viewport(window, cx);
    let epsilon = px(1.);
    let trailing_inset = geometry.window.right() - geometry.globals.right();
    assert!(geometry.globals.top() <= geometry.toolbar.top() + px(8.));
    assert!(
        (middle_y(geometry.globals) - middle_y(geometry.middle)).abs() <= epsilon,
        "globals are not vertically centred on the tab row: geometry={geometry:?}"
    );
    if let Some(tabs) = geometry.tabs {
        assert!((middle_y(tabs) - middle_y(geometry.middle)).abs() <= epsilon);
    }
    assert!(geometry.globals.left() >= geometry.middle.right() - epsilon);
    assert!(geometry.globals.right() <= geometry.window.right() + epsilon);
    assert!(
        trailing_inset >= px(1.) && trailing_inset <= px(16.),
        "globals are not right anchored: geometry={geometry:?}, width={width}"
    );
    assert_eq!(geometry.tabs.is_some(), has_tabs);
    if let Some(board_row) = geometry.board_row {
        assert!(geometry.board.top() >= board_row.top() - epsilon);
        assert!(
            board_row.top() >= geometry.middle.bottom().max(geometry.globals.bottom()) - epsilon
        );
    } else {
        assert!(
            geometry.board.bottom()
                <= geometry.middle.bottom().max(geometry.globals.bottom()) + epsilon
        );
        assert!(geometry.board.right() <= geometry.globals.left() + epsilon);
        assert!(
            (middle_y(geometry.board) - middle_y(geometry.middle)).abs() <= epsilon,
            "board controls beside the tabs are not vertically centred: geometry={geometry:?}"
        );
    }
    assert_global_controls_centred(window, cx, geometry.middle);
}

fn middle_y(bounds: Bounds<Pixels>) -> Pixels {
    (bounds.top() + bounds.bottom()) / 2.
}

/// The shell's own buttons, not only their container, sit on the row's centre.
fn assert_global_controls_centred(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    row: Bounds<Pixels>,
) {
    cx.update_window((*window).into(), |_, window, _| {
        let globals = window.within("global-toolbar");
        let control = globals
            .try_find("help")
            .or_else(|| globals.try_find("overflow-toolbar-button"))
            .expect("the global toolbar shows Help or its overflow button");
        let offset = (middle_y(control.bounds()) - middle_y(row)).abs();
        assert!(
            offset <= px(1.),
            "global control is {offset:?} off the row centre"
        );
    })
    .unwrap();
}

fn assert_board_viewport(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) {
    cx.update_window((*window).into(), |_, window, _| {
        let right = window.bounds().right();
        let board = window.within("dashboard-toolbar");
        let row = board.find("overflow-toolbar-row").bounds();
        assert!(row.right() <= right, "board row exceeds window: {row:?}");
        for index in 0usize..8 {
            if let Some(control) = board.try_find(format!("overflow-unit:fixture-unit-{index}")) {
                let bounds = control.bounds();
                assert!(
                    bounds.left() >= row.left() && bounds.right() <= row.right(),
                    "clipped action: {bounds:?}, {row:?}"
                );
            }
        }
        if board.try_find("overflow-unit:fixture-unit-7").is_none() {
            let trigger = board.find("overflow-toolbar-button").bounds();
            assert!(
                trigger.right() <= row.right(),
                "clipped overflow: {trigger:?}, {row:?}"
            );
        }
    })
    .unwrap();
}
