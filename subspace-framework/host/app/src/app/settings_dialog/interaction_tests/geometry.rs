//! Production dialog boundaries, not an isolated footer approximation.

use gpui_kit::base::test_support::snapshots;
use gpui_kit::component::Theme;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, Window, px, size};
use subspace_dashboard::prefs;

use super::Fixture;
use crate::theme::{self, THEME_PREF};

#[gpui_kit::test]
fn sidebar_fills_the_dialog_and_contains_updates_beside_right_hand_actions(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let fixture = Fixture::open(root.path(), cx);
    for theme_name in ["Solarized Light", "Solarized Dark"] {
        cx.update(|cx| {
            prefs::set(cx, THEME_PREF, theme_name);
            theme::initialize(cx);
        });
        for font_size in [16., 20.] {
            cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
            for width in [320., 640., 1000., 2200.] {
                cx.simulate_window_resize(fixture.window, size(px(width), px(1100.)));
                cx.update_window(fixture.window, |_, window, cx| {
                    window.render_frame(cx);
                    assert_eq!(window.rem_size(), px(font_size));
                    assert_pane_geometry(window);
                    let sidebar = window.find("settings-navigation").bounds();
                    let body = window.find("settings-body").bounds();
                    assert!(sidebar.size.width <= body.size.width / 2.);
                    if width == 2200. {
                        assert_eq!(body.size.width, px(1078.));
                        assert_eq!(sidebar.size.width, px(font_size * 16.));
                    }
                })
                .unwrap();
            }
        }
    }
}

fn assert_pane_geometry(window: &mut Window) {
    let dialog = snapshots(window)
        .into_iter()
        .filter(|element| element.path().last() == Some(&"surface".into()))
        .min_by_key(|element| element.path().len())
        .unwrap()
        .bounds();
    let sidebar = window.find("settings-navigation").bounds();
    // The SDK surface owns its one-pixel border; the pane fills its interior.
    assert_eq!(sidebar.left(), dialog.left() + px(1.));
    assert_eq!(sidebar.top(), dialog.top() + px(1.));
    assert_eq!(sidebar.bottom(), dialog.bottom() - px(1.));
    for id in [
        "settings-updates",
        "settings-update-action",
        "settings-app-heading",
        "settings-boards-heading",
    ] {
        let element = window.find(id);
        assert!(element.visible(), "{id}");
        let bounds = element.bounds();
        assert!(
            bounds.left() >= sidebar.left(),
            "{id}: {bounds:?} {sidebar:?}"
        );
        assert!(
            bounds.right() <= sidebar.right(),
            "{id}: {bounds:?} {sidebar:?}"
        );
        assert!(
            bounds.bottom() <= sidebar.bottom(),
            "{id}: {bounds:?} {sidebar:?}"
        );
    }
    let app = window.find("settings-app-heading");
    let boards = window.find("settings-boards-heading");
    assert_eq!(boards.label(), Some("Spaces"));
    assert_eq!(app.bounds().left(), boards.bounds().left());
    assert_eq!(app.bounds().size.height, boards.bounds().size.height);
    for id in ["settings-cancel", "settings-save"] {
        let action = window.find(id);
        assert!(action.visible());
        assert!(
            action.bounds().left() >= sidebar.right(),
            "{id}: {:?} {sidebar:?}",
            action.bounds()
        );
        assert!(action.bounds().right() <= dialog.right());
    }
}
