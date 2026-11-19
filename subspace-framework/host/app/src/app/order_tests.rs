use super::toolbar::tests::status::rooted_fixture;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use std::sync::Arc;
use subspace_agent_tools::Catalog;
use subspace_core::{config::Config, credentials::Credentials};
use subspace_dashboard::{Services, prefs};

#[gpui_kit::test]
fn saved_order_reuses_boards_and_keeps_the_active_board_then_rebinds_function_keys(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 2200., false);
    let retained = cx
        .update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.dashboards.truncate(3);
                for (board, id) in app.dashboards.iter_mut().zip(["first", "second", "third"]) {
                    board.id = id;
                }
                app.active = 1;
                app.focus_active(window, cx);
                app.dashboards
                    .iter()
                    .map(|board| (board.id, board.focus_handle(cx)))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap();
    cx.update_window(window, |_, window, cx| {
        let initial = Config::default();
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&initial)),
            config: Arc::new(initial),
            tools: Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file: root.path().join("config.json"),
        });
        let mut config = (*cx.global::<Services>().config).clone();
        config.board_order = ["third", "first", "second"].map(str::to_owned).into();
        cx.global_mut::<Services>().config = Arc::new(config);
        app.update(cx, |app, cx| app.sync_navigation_preferences(window, cx));
    })
    .unwrap();
    app.read_with(cx, |app, cx| {
        assert_eq!(
            app.dashboards
                .iter()
                .map(|board| board.id)
                .collect::<Vec<_>>(),
            ["third", "first", "second"]
        );
        assert_eq!(app.dashboards[app.active].id, "second");
        for board in &app.dashboards {
            let original = retained.iter().find(|(id, _)| *id == board.id).unwrap();
            assert_eq!(board.focus_handle(cx), original.1);
        }
    });
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.press("f1", cx))
        .unwrap();
    cx.run_until_parked();
    app.read_with(cx, |app, cx| {
        assert_eq!(app.dashboards[app.active].id, "third");
        assert_eq!(prefs::get(cx, super::ACTIVE_KEY).as_deref(), Some("third"));
    });
}

#[gpui_kit::test]
fn dragging_visible_space_tabs_updates_the_saved_order_and_retains_focus(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("config.json");
    let (window, app) = rooted_fixture(cx, root.path(), 5000., false);
    cx.update(|cx| {
        let initial = Config::default();
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&initial)),
            config: Arc::new(initial),
            tools: Catalog::new(Vec::new()).unwrap(),
            search: None,
            config_file: config_path.clone(),
        });
    });
    let selected = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let selected = app.read_with(cx, |app, _| app.dashboards[app.active].id);
            window.drag_to("space-tab-fixture-three", "space-tab-toolbar-fixture", cx);
            selected
        })
        .unwrap();
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(app.dashboards[0].id, "fixture-three");
        assert_eq!(app.dashboards[app.active].id, selected);
    });
    let saved = subspace_core::settings::SettingsFile::open(&config_path).unwrap();
    assert_eq!(
        saved.values().board_order[..3],
        ["fixture-three", "toolbar-fixture", "fixture-two"]
    );
}
