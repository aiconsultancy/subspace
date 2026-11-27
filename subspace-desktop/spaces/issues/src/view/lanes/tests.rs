use serde_json::json;
use subspace_framework::dashboard::Tone;
use subspace_framework::lucide::IconName;

use super::{Lanes, label_rgb, short_date, state_icon};
use crate::fixtures::issue;
use crate::model::{IssuePriority, StateKind};
use crate::view::progress::{Figure, shown_progress};

#[gpui_kit::test]
fn identifiers_use_the_code_font_role(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::Styled as _;
    use gpui_kit::component::ActiveTheme as _;

    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::component::Theme::global_mut(cx).mono_font_family = "Fixture Code".into();
        let mut text = super::identifier("SUB-1616", cx);
        assert_eq!(
            text.style().text_style().font_family.as_ref(),
            Some(&cx.theme().mono_font_family)
        );
        assert_ne!(cx.theme().mono_font_family, cx.theme().font_family);
    });
}

#[test]
fn labels_go_as_the_pane_narrows() {
    assert_eq!(
        Lanes::for_width(880.),
        Lanes { labels: true },
        "a 1440 px window with both docks"
    );
    assert_eq!(Lanes::for_width(720.), Lanes { labels: true });
    assert_eq!(Lanes::for_width(719.), Lanes { labels: false });
    assert_eq!(Lanes::for_width(0.), Lanes::for_width(880.));
}

#[test]
fn states_and_priorities_use_lucide_glyphs_and_never_colour_alone() {
    assert_eq!(state_icon(StateKind::Backlog).0, IconName::CircleDashed);
    assert_eq!(state_icon(StateKind::Unstarted).0, IconName::Circle);
    assert_eq!(
        state_icon(StateKind::Started),
        (IconName::Contrast, Tone::Yellow)
    );
    assert_eq!(state_icon(StateKind::Completed).0, IconName::CircleCheck);
    assert_eq!(state_icon(StateKind::Canceled).0, IconName::CircleX);
    assert_eq!(state_icon(StateKind::Duplicate).0, IconName::CircleSlash);
    let none = IssuePriority::None.look();
    assert_eq!((none.icon, none.label), (IconName::Ellipsis, "No priority"));
    let urgent = IssuePriority::Urgent.look();
    assert_eq!(
        (urgent.icon, urgent.tone),
        (IconName::CircleAlert, Tone::Orange)
    );
    assert_eq!(IssuePriority::Low.look().icon, IconName::SignalLow);
}

#[test]
fn trackers_show_a_strip_or_a_bar_and_parents_their_count() {
    let progress = |done: i32, total: i32| {
        json!({"type": "tracker", "progress": {
            "done": done, "total": total, "canceled": 0, "duplicate": 0,
            "sample": [], "hasMore": false,
        }})
    };
    let few = shown_progress(&issue("SUB-1", "A", ("Todo", "unstarted"), progress(3, 9))).unwrap();
    assert_eq!((few.figure, few.text.as_str()), (Figure::Strip, "3 of 9"));
    let many = shown_progress(&issue(
        "SUB-2",
        "B",
        ("Todo", "unstarted"),
        progress(31, 52),
    ))
    .unwrap();
    assert_eq!((many.figure, many.text.as_str()), (Figure::Bar, "31 of 52"));
    let parent = issue(
        "SUB-3",
        "C",
        ("Todo", "unstarted"),
        json!({"progress": {"done": 1, "total": 2, "canceled": 0, "duplicate": 0, "sample": [], "hasMore": false}}),
    );
    let count = shown_progress(&parent).unwrap();
    assert_eq!((count.figure, count.text.as_str()), (Figure::Count, "1/2"));
    assert_eq!(
        shown_progress(&issue("SUB-4", "D", ("Todo", "unstarted"), json!({}))),
        None
    );
    assert_eq!(
        shown_progress(&issue("SUB-5", "E", ("Todo", "unstarted"), progress(0, 0))),
        None,
        "an empty tracker shows nothing"
    );
}

#[test]
fn label_colours_and_due_dates_read_as_the_api_writes_them() {
    assert_eq!(label_rgb("#e5484d"), Some(0xe5484d));
    assert_eq!(label_rgb("e5484d"), None);
    assert_eq!(label_rgb("#e548"), None);
    assert_eq!(label_rgb("#gggggg"), None);
    assert_eq!(short_date("2026-12-17").as_deref(), Some("17 Dec"));
    assert_eq!(short_date("2026-01-05T00:00:00Z").as_deref(), Some("5 Jan"));
    assert_eq!(short_date("2026-13-01"), None);
    assert_eq!(short_date("soon"), None);
}

#[gpui_kit::test]
fn issue_rows_name_both_roles_and_empty_slots(cx: &mut gpui_kit::TestAppContext) {
    use crate::tests::{design_groups, label, listed, render};

    let (board, cx) = listed(cx);
    let mut groups = design_groups();
    groups[0].rows[0].fields.owner =
        Some(serde_json::from_value(crate::fixtures::person("Rosa Fixture")).unwrap());
    board.update(cx, |board, cx| board.show_groups(groups, true, cx));
    render(cx);

    for (id, expected) in [
        ("issues-person:Owner:id-SUB-955", "Owner: Rosa Fixture"),
        ("issues-person:Assignee:id-SUB-955", "Assignee: Jo Fixture"),
        ("issues-person:Owner:id-SUB-1540", "Owner: No owner"),
        ("issues-person:Assignee:id-SUB-1540", "Assignee: Unassigned"),
    ] {
        assert_eq!(label(cx, id).as_deref(), Some(expected));
    }
}
