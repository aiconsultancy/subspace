//! The navigator grouped by fleet, as drawn: a fleet's group with its
//! channels named without the fleet's prefix, a ship led by its captain's
//! icon as the Icons setting asks and ending in its one working dot, and
//! `People` for private conversations with people. Which fleet groups
//! start folded is in [`super::folding`].

mod listed;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Event;
use subspace_framework::dashboard::ListDashboard as _;
use subspace_framework::dashboard::avatar::AgentIcons;

use super::super::inject;
use super::super::remove::settle;
use super::fleet::ship;
use crate::fixtures::{channel, message, network_id, query};
use crate::{Irc, rows};

/// The group names and their rows' titles, as the list holds them.
fn outline(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Vec<(String, Vec<String>)> {
    board.read_with(cx, |board, _| {
        board
            .list
            .groups()
            .iter()
            .map(|group| {
                let rows = group.rows.iter().map(|row| row.title.to_string()).collect();
                (group.name.to_string(), rows)
            })
            .collect()
    })
}

/// `line`, marked by the runtime as mentioning us.
pub(super) fn mention(mut line: subspace_client_irc::Line) -> subspace_client_irc::Line {
    line.highlight = true;
    line
}

/// The ship channel open, the Starbase joined with a line that mentions
/// us, and private conversations with mira and with the captain.
fn populated(cx: &mut TestAppContext) -> (Entity<Irc>, &mut VisualTestContext) {
    let (board, hub, cx) = ship(cx);
    let starbase = channel(STARBASE);
    let line = mention(message(&starbase, "mira", "subspace: the review moved", 4));
    inject(
        &hub,
        vec![
            Event::Joined { buffer: starbase },
            Event::Joined {
                buffer: query("mira"),
            },
            Event::Joined {
                buffer: query("aurora-lyra"),
            },
            Event::Line(line),
        ],
        cx,
    );
    settle(cx);
    (board, cx)
}

pub(super) const STARBASE: &str = "#aurora-starbase";

#[gpui_kit::test]
fn the_navigator_draws_a_fleet_s_channels_and_people_apart(cx: &mut TestAppContext) {
    let (board, cx) = populated(cx);
    let titles = |rows: &[&str]| rows.iter().map(|row| (*row).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        outline(&board, cx),
        [
            (
                "Aurora".to_owned(),
                titles(&["starbase", "lyra", "aurora-lyra", "Server"])
            ),
            ("People".to_owned(), titles(&["mira"])),
        ]
    );
    let lyra = rows::row_key(&channel("#aurora-lyra"));
    let starbase = rows::row_key(&channel(STARBASE));
    cx.update(|window, _| {
        assert_eq!(
            window.find(format!("irc-row-avatar:{lyra}")).label(),
            Some("aurora-lyra, captain"),
            "by default a ship leads with its captain's small icon"
        );
        assert!(window.find(format!("irc-row:{starbase}")).visible());
        assert!(
            window
                .try_find(format!("irc-row-avatar:{starbase}"))
                .is_none(),
            "the Starbase leads with the #"
        );
        assert!(window.try_find(format!("irc-row-dot:{starbase}")).is_none());
    });
    cx.update(|_, cx| AgentIcons::Off.apply(cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(
            window.try_find(format!("irc-row-avatar:{lyra}")).is_none(),
            "with Icons off a ship leads with the # too"
        );
        assert!(window.find(format!("irc-row:{lyra}")).visible());
    });
    board.read_with(cx, |board, cx| {
        let mira = rows::row_key(&query("mira"));
        let row = board.list.rows().find(|row| row.key == mira).cloned();
        let label = row.map(|row| board.row_accessible_label(&row, cx).to_string());
        assert_eq!(
            label.as_deref(),
            Some("mira, here"),
            "mira shares #aurora-lyra"
        );
    });
}

#[gpui_kit::test]
fn an_unread_row_is_emphasised_with_its_mention_mark_and_count(cx: &mut TestAppContext) {
    let (board, cx) = populated(cx);
    let starbase = rows::row_key(&channel(STARBASE));
    board.read_with(cx, |board, cx| {
        let row = board
            .list
            .rows()
            .find(|row| row.key == starbase)
            .cloned()
            .expect("the Starbase row");
        assert!(board.row_emphasis(&row, cx), "unread rows are emphasised");
        let marks: Vec<&str> = row
            .trailing
            .iter()
            .map(|glyph| glyph.text.as_ref())
            .collect();
        assert_eq!(marks, ["@", "1"], "a mention's mark, then the count");
        assert_eq!(
            board.row_accessible_label(&row, cx).as_ref(),
            "starbase, 1 unread, 1 mentioning you"
        );
    });
}

/// The ship row's dot's accessible name.
fn dot(lyra: &str, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| {
        window
            .find(format!("irc-row-dot:{lyra}"))
            .label()
            .map(str::to_owned)
    })
}

#[gpui_kit::test]
fn a_ship_s_one_dot_says_working_while_its_captain_has_a_status(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let lyra = rows::row_key(&channel("#aurora-lyra"));
    assert_eq!(dot(&lyra, cx).as_deref(), Some("aurora-lyra: idle"));
    inject(
        &hub,
        vec![Event::NickStatus {
            network: network_id(),
            nick: "aurora-lyra".into(),
            status: Some("Reviewing NAV-7".into()),
        }],
        cx,
    );
    settle(cx);
    assert_eq!(dot(&lyra, cx).as_deref(), Some("aurora-lyra: working"));
    board.read_with(cx, |board, cx| {
        let row = board.list.rows().find(|row| row.key == lyra).cloned();
        let row = row.expect("the ship row");
        assert!(row.trailing.is_empty(), "no working text");
        assert_eq!(
            board.row_accessible_label(&row, cx).as_ref(),
            "lyra, aurora-lyra: working"
        );
    });
}
