use super::{fixture, mount_state};
use crate::project::{InputValue, LivePatch, LiveState, ResultUpdate, ResultValue, TableData};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{ScrollDelta, TestAppContext, point, px};

fn large_fixture() -> LiveState {
    let mut state = fixture();
    let mut patch = LivePatch::default();
    patch.results.insert(
        "result".into(),
        ResultUpdate::new(
            "large",
            2,
            ResultValue::Table {
                table: TableData::new(
                    (0..16).map(|column| format!("Column {column}")).collect(),
                    (0..500)
                        .map(|row| {
                            (0..16)
                                .map(|column| InputValue::Number(f64::from(row * 16 + column)))
                                .collect()
                        })
                        .collect(),
                ),
            },
        ),
    );
    state.apply("session-one", 0, patch).unwrap();
    state
}

#[gpui_kit::test]
fn bounded_large_result_virtualizes_and_arrow_navigation_reveals_the_active_cell(
    cx: &mut TestAppContext,
) {
    let (_, events, cx) = mount_state(cx, large_fixture());
    cx.update(|window, cx| {
        assert!(
            window
                .try_find("project-grid-example-result-row-499")
                .is_none()
        );
        window.click("project-grid-example-result-row-0-cell-0", cx);
        for _ in 0..20 {
            window.press("down", cx);
        }
        assert_eq!(
            window
                .find("project-grid-example-result-row-20-cell-0")
                .selected(),
            Some(true)
        );
        window.press("end", cx);
        assert!(
            window
                .find("project-grid-example-result-row-20-cell-15")
                .visible()
        );
        window.press("secondary-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("335")
        );
        window.scroll(
            "project-grid-example-result-viewport",
            ScrollDelta::Pixels(point(px(0.), px(-20_000.))),
            cx,
        );
        assert!(
            window
                .try_find("project-grid-example-result-row-499")
                .is_some()
        );
    });
    cx.run_until_parked();
    assert!(events.borrow().is_empty());
}

#[gpui_kit::test]
fn no_matching_rows_recovers_through_clear_filters(cx: &mut TestAppContext) {
    let (_, _, cx) = super::mount(cx);
    cx.update(|window, cx| {
        window.click("project-grid-example-result-filter-0", cx);
        window.input("not present", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find("project-grid-example-result-row-0")
                .is_none()
        );
        window.click("project-grid-example-result-clear-filters", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("project-grid-example-result-row-0").visible());
    });
}

#[gpui_kit::test]
fn theme_and_rem_changes_keep_headers_aligned_and_the_keyboard_target_visible(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::{Theme, ThemeMode};
    let (_, _, cx) = super::mount(cx);
    for (mode, rem) in [(ThemeMode::Light, 16.), (ThemeMode::Dark, 20.)] {
        cx.update(|window, cx| {
            Theme::change(mode, Some(window), cx);
            window.set_rem_size(px(rem));
            window.render_frame(cx);
            let header = window.find("project-grid-example-result-header-0").bounds();
            let cell = window
                .find("project-grid-example-result-row-0-cell-0")
                .bounds();
            assert_eq!(header.left(), cell.left());
            assert_eq!(header.size.width, cell.size.width);
            assert_eq!(cell.size.height, px(rem * 2.));
            window.click("project-grid-example-result-row-0-cell-0", cx);
            window.press("right", cx);
            assert_eq!(
                window
                    .find("project-grid-example-result-row-0-cell-1")
                    .selected(),
                Some(true)
            );
            assert_eq!(
                window
                    .find("project-grid-example-result-viewport")
                    .focused(),
                Some(true)
            );
        });
    }
}
