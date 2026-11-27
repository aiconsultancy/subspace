use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Entity, Pixels, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions, px, size,
};
use subspace_framework::dashboard::icon_button::IconButtonSize;
use subspace_framework::dashboard::{
    Dashboard,
    workspace::{PANE_MIN_WIDTH_PX, READING_WIDTH_PX},
};

use crate::milestone_page::Detail;
use crate::{Projects, Record, Tab};

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

#[gpui_kit::test]
fn the_toolbar_heading_the_pane_ends_level_with_the_navigators_header(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (window, _board) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Projects::new(window, cx))
        })
        .unwrap()
    });
    let view = VisualTestContext::from_window(window, cx).into_mut();
    view.update(|window, cx| {
        window.render_frame(cx);
        let header = window.find("workspace-navigator-header").bounds();
        let toolbar = window.find("projects-toolbar").bounds();
        assert!(
            (toolbar.bottom() - header.bottom()).abs() < px(0.5),
            "the hairlines under {toolbar:?} and {header:?} are one line"
        );
    });
}

/// Each leading control's id and its entry in the overflow menu.
const LEADING: [(&str, &str); 4] = [
    ("projects-initiative-button", "Initiative: Design Studio…"),
    ("Overview", "Overview"),
    ("Milestones", "Milestones"),
    ("milestone-back", "Back"),
];

const LEADING_SIDE: &str = "workspace-toolbar-leading";
const TRAILING_SIDE: &str = "workspace-toolbar-trailing";

/// What the pane shows in a narrow test.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    /// A milestone's page, so all four leading controls are there.
    Milestone,
    /// A writable project's overview, so Edit project is there too.
    Overview,
}

/// Projects in a pane at its least width showing `page`, at icon-button
/// size `buttons`.
fn narrow(
    buttons: IconButtonSize,
    page: Page,
    cx: &mut TestAppContext,
) -> (Entity<Projects>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            Default::default(),
            size(px(500.), px(600.)),
        ))),
        ..Default::default()
    };
    let (window, board) = cx.update(|cx| {
        buttons.apply(cx);
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Projects::new(window, cx))
        })
        .unwrap()
    });
    let view = VisualTestContext::from_window(window, cx).into_mut();
    board.update(view, |board, cx| {
        let initiative = serde_json::from_value(serde_json::json!({
            "id": "initiative-design", "name": "Design Studio", "channels": ""
        }))
        .unwrap();
        board.initiatives.update(cx, |catalog, cx| {
            catalog.set_memberships_for_test(Some("ada".into()), Ok(vec![initiative]), cx);
        });
    });
    // Picking up the initiative clears the selection, so select afterwards.
    view.run_until_parked();
    board.update(view, |board, cx| {
        board.selected = Some("project-sub".into());
        match page {
            Page::Milestone => {
                board.tab = Tab::Milestones;
                board.milestone_pages.detail = Some(Detail {
                    milestone: "milestone-first".into(),
                    from: Tab::Milestones,
                });
            }
            Page::Overview => {
                board.tab = Tab::Overview;
                board.record = Some(Ok(writable_record()));
            }
        }
        cx.notify();
    });
    settle(view);
    (board, view)
}

/// A project the person may edit, in the selected initiative.
fn writable_record() -> Record {
    let project = serde_json::from_value(serde_json::json!({
        "id": "project-sub", "key": "SUB", "name": "Subspace fixture",
        "leadName": null, "status": "started", "health": null, "priority": 2,
        "startDate": null, "targetDate": null, "description": null,
        "archived": false, "initiativeId": "initiative-design",
        "initiative": null, "version": 1
    }))
    .unwrap();
    Record {
        project,
        milestones: Ok(Vec::new()),
        update: Ok(None),
    }
}

fn settle(view: &mut VisualTestContext) {
    for _ in 0..6 {
        view.update(|window, cx| window.render_frame(cx));
        view.run_until_parked();
    }
}

/// The entries of the open overflow menu.
fn menu_labels(window: &mut gpui_kit::Window) -> Vec<String> {
    let menu = window.within("popup-menu");
    (0usize..16)
        .filter_map(|index| menu.try_find(index))
        .filter_map(|item| item.label().map(|label| label.to_string()))
        .collect()
}

/// Opens the leading side's overflow menu and chooses `entry` in it.
fn choose(entry: &str, view: &mut VisualTestContext) {
    view.update(|window, cx| {
        window
            .within(LEADING_SIDE)
            .click("overflow-toolbar-button", cx);
        let menu = window.within("popup-menu");
        let index = (0usize..16)
            .find(|index| {
                menu.try_find(*index)
                    .is_some_and(|item| item.label().is_some_and(|label| label == entry))
            })
            .unwrap_or_else(|| panic!("{entry} is in the overflow menu"));
        window.within("popup-menu").click(index, cx);
    });
    settle(view);
}

/// Checks that each of `controls`, an id and its overflow menu entry, is
/// wholly inside the toolbar's `side` or listed in that side's overflow
/// menu, and returns the entries of those in the menu.
fn reachable(
    side: &'static str,
    controls: &[(&'static str, &'static str)],
    cx: &mut VisualTestContext,
) -> Vec<&'static str> {
    cx.update(|window, cx| {
        let pane = window.find("workspace-pane").bounds();
        assert!(pane.size.width <= px(PANE_MIN_WIDTH_PX + 1.), "{pane:?}");
        let area = window.find(side).bounds();
        let inside = |part: Bounds<Pixels>| {
            part.left() >= area.left()
                && part.right() <= area.right()
                && part.top() >= area.top()
                && part.bottom() <= area.bottom()
        };
        let mut hidden = Vec::new();
        for (id, entry) in controls {
            match window.try_find(*id) {
                Some(control) => {
                    let bounds = control.bounds();
                    assert!(control.visible(), "{id}");
                    assert!(inside(bounds), "{id} at {bounds:?} in {side} {area:?}");
                }
                None => hidden.push(*entry),
            }
        }
        if !hidden.is_empty() {
            let chevron = window.within(side).find("overflow-toolbar-button").bounds();
            assert!(
                inside(chevron),
                "{side}'s menu button {chevron:?} in {area:?}"
            );
            window.within(side).click("overflow-toolbar-button", cx);
            let labels = menu_labels(window);
            for entry in &hidden {
                assert!(
                    labels.iter().any(|label| label == entry),
                    "{entry} in {side}'s {labels:?}"
                );
            }
            window.within(side).click("overflow-toolbar-button", cx);
        }
        hidden
    })
}

#[gpui_kit::test]
fn a_narrow_pane_keeps_every_leading_control_reachable(cx: &mut TestAppContext) {
    let (board, view) = narrow(IconButtonSize::Small, Page::Milestone, cx);
    let hidden = reachable(LEADING_SIDE, &LEADING, view);
    assert!(hidden.contains(&"Back"), "Back, the last, goes first");
    choose("Back", view);
    view.update(|_, cx| {
        assert!(
            board.read(cx).milestone_pages.detail.is_none(),
            "Back from the menu left the milestone"
        );
    });
}

#[gpui_kit::test]
fn the_largest_icon_buttons_keep_every_leading_control_reachable(cx: &mut TestAppContext) {
    let (board, view) = narrow(IconButtonSize::ExtraLarge, Page::Milestone, cx);
    let hidden = reachable(LEADING_SIDE, &LEADING, view);
    let initiative = LEADING[0].1;
    assert!(hidden.contains(&initiative), "{hidden:?}");
    choose(initiative, view);
    view.update(|_, cx| {
        assert!(
            board.read(cx).initiative_menu.is_some(),
            "the initiative entry opened the picker"
        );
    });
}

#[gpui_kit::test]
fn the_largest_icon_buttons_keep_edit_project_and_refresh_reachable(cx: &mut TestAppContext) {
    let (_board, view) = narrow(IconButtonSize::ExtraLarge, Page::Overview, cx);
    reachable(LEADING_SIDE, &LEADING[..3], view);
    let actions = [
        ("projects-edit", "Edit project"),
        ("projects-refresh", "Refresh"),
    ];
    let hidden = reachable(TRAILING_SIDE, &actions, view);
    assert!(!hidden.is_empty(), "the actions yield the line: {hidden:?}");
}

#[gpui_kit::test]
fn leaving_projects_closes_the_trailing_overflow_menu(cx: &mut TestAppContext) {
    let (board, view) = narrow(IconButtonSize::ExtraLarge, Page::Overview, cx);
    view.update(|window, cx| {
        window
            .within(TRAILING_SIDE)
            .click("overflow-toolbar-button", cx);
    });
    settle(view);
    view.update(|window, _| assert!(window.try_find("popup-menu").is_some(), "the menu opened"));
    view.update(|window, cx| {
        board.update(cx, |board, cx| {
            subspace_framework::dashboard::Dashboard::deactivate(board, window, cx);
        });
    });
    settle(view);
    view.update(|window, _| {
        assert!(
            window.try_find("popup-menu").is_none(),
            "no menu left open behind another Space"
        );
    });
}
