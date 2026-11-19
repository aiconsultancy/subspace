//! The title bar: clearance after the traffic lights, one centre line, and
//! board tabs drawn as inset glass capsules.

use gpui_kit::component::Theme;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Bounds, Pixels, TestAppContext, WindowHandle, px};

use super::{SubspaceApp, open_fixture, settle};
use crate::app::OpenLogView;
#[cfg(target_os = "macos")]
use crate::app::titlebar::{CONTENT_START, TRAFFIC_LIGHTS_END, traffic_light_origin};

#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_HALF: f32 = 7.;

fn centre_y(bounds: Bounds<Pixels>) -> Pixels {
    (bounds.top() + bounds.bottom()) / 2.
}

#[gpui_kit::test]
fn title_bar_content_clears_the_traffic_lights_on_their_centre_line(cx: &mut TestAppContext) {
    for mode in ["horizontal", "vertical"] {
        for font_size in [16., 20.] {
            let root = tempfile::tempdir().unwrap();
            let window = open_fixture(cx, root.path(), mode, 1280., false);
            cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
            cx.update_window(*window, |_, window, _| window.set_rem_size(px(font_size)))
                .unwrap();
            settle(&window, cx);
            cx.update_window(*window, |_, window, _| {
                let row = window.find("app-toolbar-first-row").bounds();
                let logo = window.find("toolbar-logo").bounds();
                let help = window.within("global-toolbar").find("help").bounds();
                let case = format!("{mode} at {font_size}: logo {logo:?}, row {row:?}");
                #[cfg(target_os = "macos")]
                {
                    let edge = window.bounds().left();
                    let lights = traffic_light_origin(row.size.height).y + px(TRAFFIC_LIGHT_HALF);
                    assert!(logo.left() - edge >= px(CONTENT_START) - px(0.5), "{case}");
                    assert!(
                        logo.left() - edge - px(TRAFFIC_LIGHTS_END) >= px(14.),
                        "too close to the zoom button: {case}"
                    );
                    assert!(
                        (lights - (centre_y(row) - row.top())).abs() <= px(1.),
                        "traffic lights off the row centre: {case}"
                    );
                }
                assert!((centre_y(logo) - centre_y(row)).abs() <= px(1.), "{case}");
                assert!((centre_y(help) - centre_y(row)).abs() <= px(1.), "{case}");
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn tabs_are_inset_capsules_with_gaps_between_them(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), "horizontal", 5000., false);
    settle(&window, cx);
    let ids = window
        .read_with(cx, |app, _| {
            app.dashboards
                .iter()
                .map(|board| board.id)
                .collect::<Vec<_>>()
        })
        .unwrap();
    cx.update_window(*window, |_, window, _| {
        let top = window.bounds().top();
        let row = window.find("app-toolbar-first-row").bounds();
        let tabs: Vec<_> = ids
            .iter()
            .filter_map(|id| window.try_find(format!("space-tab-{id}")))
            .map(|tab| tab.bounds())
            .collect();
        assert!(tabs.len() >= 2, "the wide window shows several tabs");
        for tab in &tabs {
            let above = tab.top() - row.top();
            let below = row.bottom() - tab.bottom();
            assert!(
                tab.top() - top >= px(2.),
                "tab touches the window top: {tab:?}"
            );
            assert!(
                (above - below).abs() <= px(1.),
                "tab is not vertically centred: {above:?} above, {below:?} below"
            );
        }
        for pair in tabs.windows(2) {
            let gap = pair[1].left() - pair[0].right();
            assert!(gap >= px(2.), "tabs touch: {pair:?}");
        }
    })
    .unwrap();
}

fn selected_tabs(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    id: &'static str,
) -> Vec<usize> {
    let ids = window
        .read_with(cx, |app, _| {
            app.dashboards
                .iter()
                .map(|board| board.id)
                .collect::<Vec<_>>()
        })
        .unwrap();
    cx.update_window(**window, |_, window, _| {
        ids.iter()
            .enumerate()
            .filter_map(|(index, space)| {
                let tab = if id == "space-tab" {
                    window.try_find(format!("space-tab-{space}"))
                } else {
                    window.try_find((id, index))
                };
                tab.is_some_and(|tab| tab.checked() == Some(true))
                    .then_some(index)
            })
            .collect()
    })
    .unwrap()
}

#[gpui_kit::test]
fn only_the_current_board_is_glass_and_none_while_logs_are_open(cx: &mut TestAppContext) {
    for (mode, id) in [
        ("horizontal", "space-tab"),
        ("vertical", "vertical-dashboard-tab"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let window = open_fixture(cx, root.path(), mode, 5000., false);
        window
            .update(cx, |app, window, cx| app.switch_to(2, window, cx))
            .unwrap();
        settle(&window, cx);
        assert_eq!(selected_tabs(&window, cx, id), vec![2], "{mode}");
        window
            .update(cx, |app, window, cx| {
                app.open_log_view(&OpenLogView, window, cx)
            })
            .unwrap();
        settle(&window, cx);
        assert!(
            selected_tabs(&window, cx, id).is_empty(),
            "{mode} with logs open"
        );
    }
}
