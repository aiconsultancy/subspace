//! The Space's layout: the pane toolbar with the page controls, and the page
//! area below it. No page opens here; a page is created when the app first
//! shows the board.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::core::config::web_boards::{WebBoard as WebBoardConfig, WebBoardSource};
use subspace_framework::dashboard::Dashboard as _;

use crate::WebBoard;

const NAVIGATION: [&str; 4] = ["web-back", "web-forward", "web-reload", "web-home"];

fn config(source: WebBoardSource) -> WebBoardConfig {
    WebBoardConfig {
        id: "wiki".to_owned(),
        title: "Team wiki".to_owned(),
        source,
        settings: Default::default(),
        secrets: Default::default(),
        permissions: Default::default(),
    }
}

#[gpui_kit::test]
fn the_page_controls_sit_in_the_pane_toolbar_above_the_page_area(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let source = WebBoardSource::Url("https://wiki.example.com/".to_owned());
    let (board, cx) = cx.add_window_view(|_, cx| WebBoard::with_config(config(source), cx));
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            board
                .update(cx, |board, cx| board.render_toolbar(window, cx))
                .is_none(),
            "the title bar repeats no page control"
        );
        let toolbar = window.find("web-toolbar").bounds();
        let page_area = window.find("workspace-content").bounds();
        assert!(
            toolbar.bottom() <= page_area.top(),
            "the page starts below the toolbar"
        );
        let bar = window.within("web-toolbar");
        for id in NAVIGATION.into_iter().chain(["web-browser"]) {
            assert_eq!(bar.find(id).disabled(), Some(true), "{id} needs a page");
        }
        assert_eq!(bar.find("web-title").label(), Some("Team wiki"));
        assert_eq!(
            bar.find("web-place").label(),
            Some("Page on wiki.example.com")
        );
    });
}

#[gpui_kit::test]
fn a_local_app_names_itself_and_has_no_open_in_browser(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let folder = tempfile::tempdir().unwrap();
    let source = WebBoardSource::Folder(folder.path().to_path_buf());
    let (_, cx) = cx.add_window_view(|_, cx| WebBoard::with_config(config(source), cx));
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bar = window.within("web-toolbar");
        for id in NAVIGATION {
            assert!(bar.find(id).visible(), "missing {id}");
        }
        assert!(bar.try_find("web-browser").is_none());
        assert_eq!(bar.find("web-place").label(), Some("Local web app"));
    });
}
