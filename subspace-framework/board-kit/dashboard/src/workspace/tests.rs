use gpui_kit::px;

use super::header::capitals;
use super::{SpaceDock, SpaceDocks, fit_strip, inspector_width, navigator_width, text_edge};
use crate::tokens::Tokens;

#[test]
fn a_space_reports_no_docks_by_default() {
    assert_eq!(SpaceDocks::default(), SpaceDocks::NONE);
    for dock in SpaceDock::ALL {
        assert_eq!(SpaceDocks::NONE.get(dock), None);
    }
    let docks = SpaceDocks::both(true, false);
    assert_eq!(docks.get(SpaceDock::Navigator), Some(true));
    assert_eq!(docks.get(SpaceDock::Inspector), Some(false));
}

#[test]
fn each_dock_switch_keeps_its_id_and_says_what_it_does() {
    assert_eq!(
        SpaceDock::Navigator.switch_id(),
        "workspace-navigator-toggle"
    );
    assert_eq!(
        SpaceDock::Inspector.switch_id(),
        "workspace-inspector-toggle"
    );
    assert_eq!(SpaceDock::Navigator.switch_name(true), "Hide navigator");
    assert_eq!(SpaceDock::Navigator.switch_name(false), "Show navigator");
    assert_eq!(SpaceDock::Inspector.switch_name(true), "Hide inspector");
    assert_eq!(SpaceDock::Inspector.switch_name(false), "Show inspector");
}

#[test]
fn section_labels_show_in_capitals() {
    assert_eq!(&*capitals("Views"), "VIEWS");
    assert_eq!(&*capitals("Latest activity"), "LATEST ACTIVITY");
}

#[test]
fn the_navigator_keeps_its_width_inside_the_window() {
    assert_eq!(navigator_width(240.0, 1440.0), 240.0, "the token fits");
    assert_eq!(navigator_width(480.0, 1000.0), 300.0, "30 % at most");
    assert_eq!(navigator_width(100.0, 1440.0), 180.0, "never narrower");
    assert_eq!(navigator_width(240.0, 400.0), 180.0, "a narrow window");
    assert_eq!(navigator_width(240.4, 1440.0), 240.0, "whole pixels");
}

#[test]
fn the_inspector_keeps_its_width_inside_the_window() {
    assert_eq!(inspector_width(320.0, 1440.0), 320.0, "the token fits");
    assert_eq!(inspector_width(640.0, 1000.0), 400.0, "40 % at most");
    assert_eq!(inspector_width(100.0, 1440.0), 260.0, "never narrower");
    assert_eq!(inspector_width(320.0, 500.0), 260.0, "a narrow window");
}

#[test]
fn text_starts_at_the_content_inset_plus_the_row_inset() {
    let mut tokens = Tokens::one_dark();
    assert_eq!(text_edge(&tokens), tokens.pad_x, "no row inset by default");
    tokens.pad_x = px(12.);
    tokens.row_inset = px(8.);
    assert_eq!(text_edge(&tokens), px(20.));
}

#[test]
fn a_strip_grows_only_to_hold_its_tallest_button_with_its_focus_ring() {
    let small = [px(24.), px(22.), px(20.)];
    assert_eq!(fit_strip(px(34.), &small), px(34.), "Small fits");
    let tall_action = [px(24.), px(40.), px(20.)];
    assert_eq!(
        fit_strip(px(34.), &tall_action),
        px(47.),
        "a 40 px action button, 3 px of ring above and below, then the hairline"
    );
}
