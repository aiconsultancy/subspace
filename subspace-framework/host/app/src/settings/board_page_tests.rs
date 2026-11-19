//! Board page layout, the turned-off board rule and the scroll region.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, InteractiveElement as _,
    IntoElement, Render, Styled as _, TestAppContext, TestSupportExt as _, Window, WindowHandle,
    div, point, px, size,
};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;

use super::organization_tests::open_settings;
use super::{Settings, SettingsPage};

fn edit(
    settings: WindowHandle<Settings>,
    cx: &mut TestAppContext,
    change: impl FnOnce(&mut Settings),
) {
    settings
        .update(cx, |settings, _, cx| {
            change(settings);
            cx.notify();
        })
        .unwrap();
    // Elements the change adds are observed from the frame after it.
    rendered(settings, cx, |_, _| ());
}

fn rendered<T>(
    window: impl Into<AnyWindowHandle>,
    cx: &mut TestAppContext,
    check: impl FnOnce(&mut Window, &mut App) -> T,
) -> T {
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        check(window, cx)
    })
    .unwrap()
}

fn show_board(settings: WindowHandle<Settings>, width: f32, cx: &mut TestAppContext) {
    cx.simulate_window_resize(settings.into(), size(px(width), px(1500.)));
    edit(settings, cx, |settings| {
        settings.page = SettingsPage::Board("future");
    });
}

#[gpui_kit::test]
fn board_controls_share_two_equal_columns_that_stack_when_narrow(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    show_board(settings, 1400., cx);
    rendered(settings, cx, |window, _| {
        let enabled = window.within("settings-enabled-row").find(0usize).bounds();
        let background = window.within("settings-background-row");
        let toggle = background.find(0usize).bounds();
        let interval = background.find(1usize).bounds();
        assert!(interval.top() < toggle.bottom(), "one row");
        assert!(toggle.top() < interval.bottom(), "one row");
        assert_eq!(toggle.size.width, interval.size.width, "equal columns");
        assert_eq!(enabled.left(), toggle.left());
        assert_eq!(enabled.right(), toggle.right(), "switches line up");
    });

    show_board(settings, 700., cx);
    rendered(settings, cx, |window, _| {
        let background = window.within("settings-background-row");
        let toggle = background.find(0usize).bounds();
        let interval = background.find(1usize).bounds();
        assert!(interval.top() >= toggle.bottom(), "stacked");
        assert_eq!(toggle.left(), interval.left());
        assert_eq!(toggle.size.width, interval.size.width);
    });
}

fn click_background_switch(settings: WindowHandle<Settings>, cx: &mut TestAppContext) -> bool {
    rendered(settings, cx, |window, cx| {
        let cell = window
            .within("settings-background-row")
            .find(0usize)
            .bounds();
        let offset = point(cell.size.width - px(8.), cell.size.height / 2.);
        window
            .within("settings-background-row")
            .click_at(0usize, offset, cx);
    });
    cx.run_until_parked();
    settings
        .update(cx, |settings, _, _| settings.background_updates[0].enabled)
        .unwrap()
}

#[gpui_kit::test]
fn a_turned_off_board_keeps_its_values_but_locks_its_controls(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    show_board(settings, 1400., cx);
    let initial = settings
        .update(cx, |settings, _, _| settings.background_updates[0].enabled)
        .unwrap();
    let toggled = click_background_switch(settings, cx);
    assert_ne!(toggled, initial, "an enabled board's switch works");

    edit(settings, cx, |settings| {
        settings.form.set_board_enabled("future", false);
        assert!(settings.form.board_locked("future"));
    });
    assert!(rendered(settings, cx, |window, _| window
        .find("settings-board-off")
        .visible()));
    assert_eq!(
        click_background_switch(settings, cx),
        toggled,
        "a turned-off board's switch is locked and keeps its value"
    );

    edit(settings, cx, |settings| {
        settings.page = SettingsPage::BoardSection {
            board: "future",
            path: vec![0],
        };
    });
    assert!(
        rendered(settings, cx, |window, _| window
            .find("settings-board-off")
            .visible()),
        "its section pages follow the same rule"
    );

    edit(settings, cx, |settings| {
        settings.page = SettingsPage::Board("future");
        settings.form.set_board_enabled("future", true);
    });
    assert!(rendered(settings, cx, |window, _| window
        .try_find("settings-board-off")
        .is_none()));
    assert_ne!(
        click_background_switch(settings, cx),
        toggled,
        "turning the board on unlocks it before Save"
    );
}

/// The dialog with a footer, as the app shows it.
struct Dialog(Entity<Settings>);

impl Render for Dialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let footer: AnyElement = div()
            .id("test-footer")
            .test_support()
            .h(px(40.))
            .into_any_element();
        self.0.update(cx, |settings, cx| {
            settings.render_dialog(window, cx, None, Some(footer))
        })
    }
}

#[gpui_kit::test]
fn the_scrollbar_stays_between_the_breadcrumb_and_the_footer(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    open_settings(root.path(), cx);
    let file = SettingsFile::open(&root.path().join("config.json")).unwrap();
    let dialog = cx.add_window(move |window, cx| {
        Dialog(cx.new(|cx| Settings::new(file, GitHubAccounts::default(), window, cx)))
    });
    cx.simulate_window_resize(dialog.into(), size(px(900.), px(560.)));
    let settings = dialog.update(cx, |dialog, _, _| dialog.0.clone()).unwrap();
    cx.update(|cx| {
        settings.update(cx, |settings, cx| {
            settings.page = SettingsPage::Board("future");
            cx.notify();
        });
    });
    rendered(dialog, cx, |window, cx| {
        assert!(
            settings.read(cx).scroll.max_offset().y > px(0.),
            "the page overflows"
        );
        let header = window.find("settings-breadcrumb").bounds();
        let region = window.find("settings-scroll-region").bounds();
        let scrollbar = window.find("settings-scrollbar").bounds();
        let footer = window.find("test-footer").bounds();
        assert_eq!(scrollbar, region);
        assert!(scrollbar.top() >= header.bottom());
        assert!(scrollbar.bottom() <= footer.top());
        assert!(scrollbar.right() <= footer.right(), "inside the padding");
    });
}
