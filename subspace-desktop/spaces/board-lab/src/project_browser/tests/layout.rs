use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{ProjectBrowser, mount_open, project_service};
use crate::project::{EXAMPLE_DEFINITION, ProjectDefinition};
use crate::shell::{Shell, ShellRequest, ViewTab};

/// The empty state's column is at most this wide, whatever the pane's width.
const WELCOME_MAX_WIDTH: f32 = 560.;
/// Layout rounds to whole pixels on each side of a centred column.
const CENTRE_TOLERANCE: f32 = 1.;

fn mount_empty<'a>(
    root: &std::path::Path,
    cx: &'a mut TestAppContext,
) -> (Entity<ProjectBrowser>, &'a mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let path = root.to_owned();
    cx.add_window_view(move |window, cx| ProjectBrowser::new(path, window, cx))
}

#[gpui_kit::test]
fn starter_dashboard_is_visible_below_the_toolbar(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let service = project_service::shared(root.path()).unwrap();
    let definition = ProjectDefinition::parse(EXAMPLE_DEFINITION.as_bytes()).unwrap();
    service
        .create(
            &definition,
            std::collections::BTreeMap::from([(
                "main.py".into(),
                b"# test-only source\n".to_vec(),
            )]),
        )
        .unwrap();
    let initial = service.open("native-demo").unwrap();
    let (_, cx) = mount_open(root.path(), initial, cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let toolbar = window.find("board-lab-toolbar");
        let close = window.find("project-close");
        let summary = window.find("project-native-demo-summary");
        let rate = window.find("project-native-demo-rate");
        assert!(summary.visible(), "starter summary is clipped");
        assert!(rate.visible(), "starter slider is clipped");
        assert!(close.bounds().bottom() <= toolbar.bounds().bottom());
        assert!(summary.bounds().top() >= toolbar.bounds().bottom());
        assert!(rate.bounds().top() >= summary.bounds().bottom());
    });
}

#[gpui_kit::test]
fn an_open_project_moves_its_actions_to_the_toolbar(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    super::project(root.path());
    let service = project_service::shared(root.path()).unwrap();
    let (_, cx) = mount_open(root.path(), service.open("example").unwrap(), cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let toolbar = window.find("board-lab-toolbar").bounds();
        for id in [
            "project-start",
            "project-stop",
            "project-menu",
            "project-close",
        ] {
            let action = window.find(id);
            assert!(action.visible(), "{id} is clipped");
            assert!(action.bounds().top() >= toolbar.top(), "{id} above");
            assert!(action.bounds().bottom() <= toolbar.bottom(), "{id} below");
        }
        assert!(
            window.try_find("project-apply").is_none(),
            "nothing to apply while the worker is stopped and unchanged"
        );
        assert!(
            window.try_find("project-new-toolbar").is_none(),
            "the toolbar holds the open project's actions"
        );
        assert!(window.find("project-new").visible(), "navigator header");
        assert!(window.try_find("board-lab-welcome").is_none());
    });
}

#[gpui_kit::test]
fn nothing_open_shows_a_centred_welcome_with_both_ways_on(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (_, cx) = mount_empty(root.path(), cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let pane = window.find("board-lab-pane").bounds();
        let welcome = window.find("board-lab-welcome").bounds();
        assert!(f32::from(welcome.size.width) <= WELCOME_MAX_WIDTH);
        let leading = f32::from(welcome.left()) - f32::from(pane.left());
        let trailing = f32::from(pane.right()) - f32::from(welcome.right());
        assert!((leading - trailing).abs() <= CENTRE_TOLERANCE, "off centre");
        for id in [
            "project-new",
            "project-new-toolbar",
            "project-new-welcome",
            "board-lab-open-environments",
            "projects-refresh",
        ] {
            assert!(window.find(id).visible(), "{id} is missing or clipped");
        }
        assert!(window.try_find("project-start").is_none());
        window.click("project-new-welcome", cx);
        window.render_frame(cx);
        assert!(window.find("project-create-id").visible());
        assert!(
            window.try_find("project-new-toolbar").is_none(),
            "the form has its own Create and Cancel"
        );
    });
}

#[gpui_kit::test]
fn a_hidden_navigator_leaves_its_choices_in_a_toolbar_menu(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (browser, cx) = mount_empty(root.path(), cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("workspace-navigator").visible());
        assert!(window.try_find("board-lab-view-menu").is_none());
        let hidden = Shell {
            navigator: false,
            ..Shell::default()
        };
        browser.update(cx, |browser, cx| browser.set_shell(hidden, cx));
        window.render_frame(cx);
        assert!(window.try_find("workspace-navigator").is_none());
        assert!(window.try_find("project-new").is_none());
        let menu = window.find("board-lab-view-menu");
        assert!(menu.visible());
        assert_eq!(menu.label(), Some("Projects"));
    });
}

#[gpui_kit::test]
fn the_browser_asks_its_owner_to_change_the_shell(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (browser, cx) = mount_empty(root.path(), cx);
    let requests = Rc::new(RefCell::new(Vec::new()));
    let seen = requests.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&browser, move |_, request: &ShellRequest, _| {
            seen.borrow_mut().push(*request);
        })
    });
    cx.update(|window, cx| {
        window.click("board-lab-open-environments", cx);
        assert!(
            window.try_find("workspace-navigator-toggle").is_none(),
            "the navigator's switch is the status bar's"
        );
    });
    cx.run_until_parked();
    assert_eq!(
        *requests.borrow(),
        [ShellRequest::Show(ViewTab::Environments)]
    );
}
