//! The navigator's rows from the hub, grouped by fleet, and their marks:
//! the `#` or a ship's captain's icon as the Icons setting asks, a person's
//! presence dot, a ship's one working dot, which rows read as unread, and
//! which fleet groups start folded.

use std::collections::HashMap;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Div, Entity, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div,
};
use subspace_client_irc::Target;
use subspace_framework::dashboard::avatar::{self, AgentAvatar, AgentIcons, AgentRank};
use subspace_framework::dashboard::list::{GroupCaption, GroupIndicator};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Glyph, Group, Row, Tokens, Tone};

use super::kind_icon;
use crate::hub::Hub;
use crate::rows::{Activity, Presence};
use crate::{Irc, rows};

/// Lays the rows out again from `hub`'s state and each network's fleet
/// roles, folds fleet groups seen for the first time, and gives the list
/// and the board's lookups the result.
pub(super) fn apply(board: &mut Irc, hub: &Entity<Hub>, cx: &mut Context<Irc>) {
    let held = hub.read(cx);
    let state = &held.state;
    let rosters: rows::Rosters = state
        .networks()
        .iter()
        .map(|network| (network.id.clone(), held.roster(&network.id, cx)))
        .collect();
    let layout = rows::layout(state, &rosters, &held.fleets);
    let selected = held.initiatives.selected_fleet();
    board.search = std::rc::Rc::new(rows::search(&layout));
    fold_new_fleets(board, (&layout.fleets, &layout.groups), selected);
    board.buffers = layout.buffers;
    board.listed = layout.listed;
    board.group_networks = layout.networks;
    board.ships = layout.ships;
    board.people = layout.people;
    board.captions = layout.captions;
    board.watch.navigator = layout.fleets.values().cloned().collect();
    board.list.set_groups(layout.groups);
}

/// The quiet captions inside fleet group `group`, such as `Ships` above
/// its first ship channel.
pub(super) fn captions(board: &Irc, group: &Group) -> Vec<GroupCaption> {
    board
        .captions
        .get(&group.key)
        .into_iter()
        .flatten()
        .map(|(row, words)| GroupCaption::new(row.clone(), *words))
        .collect()
}

/// Whether row `key`'s buffer has unread messages.
pub(super) fn unread(board: &Irc, key: &str, cx: &App) -> bool {
    let held = board.buffers.get(key);
    let state = board.hub.as_ref().map(|hub| &hub.read(cx).state);
    held.zip(state)
        .and_then(|(key, state)| state.buffer(key))
        .is_some_and(|buffer| buffer.unread > 0)
}

/// What leads `row`, in a box the list's tests find by the row's key.
pub(super) fn leading(board: &Irc, row: &Row, cx: &App) -> Option<AnyElement> {
    let buffer = board
        .buffers
        .get(&row.key)
        .or_else(|| board.listed.get(&row.key))?;
    let marks = Marks {
        key: &row.key,
        ship: board.ships.get(&row.key),
        presence: board.people.get(&row.key).copied(),
        target: &buffer.target,
    };
    Some(
        div()
            .id(SharedString::from(format!("irc-row:{}", row.key)))
            .test_support()
            .flex_shrink_0()
            .child(row_mark(&marks, cx))
            .into_any_element(),
    )
}

/// What ends `row`: a mention's `@` mark, then an unread count or a
/// channel's user count, as a navigator row's quiet count, then a ship's
/// one dot at the trailing edge.
pub(super) fn trailing(board: &Irc, row: &Row, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let dot = board
        .ships
        .get(&row.key)
        .map(|ship| ship_dot(&row.key, ship, &tokens));
    h_flex()
        .flex_shrink_0()
        .gap_1()
        .children(row.trailing.iter().map(|glyph| count(glyph, &tokens)))
        .children(dot)
        .into_any_element()
}

/// One trailing mark in the caption size; a mention's `@` in bold.
fn count(glyph: &Glyph, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .text_size(tokens.caption_size())
        .text_color(tokens.glyph(glyph))
        .when(glyph.text.as_ref() == rows::MENTION, |mark| {
            mark.font_weight(FontWeight::BOLD)
        })
        .child(glyph.text.clone())
}

/// What a row's leading mark is drawn from.
pub(super) struct Marks<'a> {
    pub key: &'a SharedString,
    /// The ship's activity, for a ship channel's row.
    pub ship: Option<&'a Activity>,
    /// The person's presence, for a row in `People`.
    pub presence: Option<Presence>,
    pub target: &'a Target,
}

/// What leads a row: a ship's captain's agent icon as the Icons setting
/// asks (`avatar::agent_icon`), else the quiet glyph of the buffer's kind,
/// the `#` for every channel; a person in `People` with a presence dot.
pub(super) fn row_mark(marks: &Marks<'_>, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let icon = marks
        .ship
        .map(|ship| captain_icon(marks.key, &ship.captain, AgentIcons::current(cx)));
    match (icon.flatten(), marks.presence) {
        (Some(icon), _) => icon.into_any_element(),
        (None, Some(presence)) => presence_mark(presence, &tokens),
        (None, None) => Icon::from(kind_icon(marks.target))
            .xsmall()
            .text_color(tokens.muted)
            .into_any_element(),
    }
}

/// A ship's captain's icon at the size `icons` asks; `None` when off.
pub(super) fn captain_icon(key: &str, captain: &str, icons: AgentIcons) -> Option<AgentAvatar> {
    let avatar = avatar::agent(
        format!("irc-row-avatar:{key}"),
        AgentRank::Captain,
        captain.to_owned(),
    )
    .small();
    avatar::agent_icon(avatar, icons)
}

/// A person's presence: the shared activity dot, filled while they are
/// here, in a box as wide as the `#` so names line up.
fn presence_mark(presence: Presence, tokens: &Tokens) -> AnyElement {
    div()
        .size_3()
        .flex()
        .items_center()
        .justify_center()
        .child(avatar::activity_dot(presence == Presence::Here, tokens))
        .into_any_element()
}

/// A ship's one dot at its row's trailing edge: filled green while it is
/// working, a muted ring while not, named for its captain and showing what
/// is at work in the shared tooltip on hover.
pub(super) fn ship_dot(key: &str, ship: &Activity, tokens: &Tokens) -> AnyElement {
    let tooltip = SharedString::from(ship.tooltip());
    div()
        .id(SharedString::from(format!("irc-row-dot:{key}")))
        .test_support()
        .role(Role::Image)
        .aria_label(ship.label())
        .size_3()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .child(avatar::activity_dot(ship.is_working(), tokens))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .into_any_element()
}

/// What row `key`'s mark says, for its spoken name: a ship's dot or a
/// person's presence.
pub(super) fn spoken_mark(board: &Irc, key: &str) -> Option<String> {
    let ship = board.ships.get(key).map(Activity::label);
    ship.or_else(|| {
        board
            .people
            .get(key)
            .map(|presence| presence.spoken().to_owned())
    })
}

/// The `People` group's glyph.
pub(super) fn people_mark(cx: &App) -> AnyElement {
    Icon::from(IconName::Users)
        .xsmall()
        .text_color(Tokens::current(cx).muted)
        .into_any_element()
}

/// Folds each fleet group that appears for the first time while the
/// selected initiative (`selected`, its fleet id if any) is not of its
/// fleet. With no initiative selected, every fleet starts open. A network's
/// only fleet's group, which takes in the network's own rows, instead keeps
/// the fold of the network's group it replaces, so the network's other
/// channels and its Server row never fold away unasked. A new group that
/// holds the selected row stays open, so the list keeps the selection (a
/// channel joined on request, or one that moved into a fleet's group), and
/// a folded group the selected row has just moved into opens; one the
/// person folded around the selection stays folded.
pub(super) fn fold_new_fleets(
    board: &mut Irc,
    (fleets, groups): (&HashMap<SharedString, String>, &[Group]),
    selected: Option<Option<&str>>,
) {
    let holding = holding_group(groups, board.list.selected());
    for (key, fleet) in fleets {
        let first = board.seen_fleets.insert(key.clone());
        let other = selected.is_some_and(|selected| selected != Some(fleet.as_str()));
        let keeps = holding == Some(key);
        let network = groups
            .iter()
            .find(|group| &group.key == key)
            .and_then(rows::absorbed_network)
            .map(|network| board.list.is_folded(&network));
        let began = began_absorbing(&mut board.absorbing, key, network.is_some());
        let folded = board.list.is_folded(key);
        if fold_to(first, began, starts_folded(other, keeps, network))
            .is_some_and(|fold| fold != folded)
        {
            board.list.toggle(key);
        }
    }
    let moved = moved_into(board.selection_group.as_ref(), holding);
    if let Some(key) = moved.filter(|key| board.list.is_folded(key)) {
        board.list.toggle(key);
    }
    board.selection_group = holding.cloned();
}

/// Notes whether group `key` takes in its network's rows (`absorbs`) and
/// says whether it has just begun to.
fn began_absorbing(
    absorbing: &mut std::collections::HashSet<SharedString>,
    key: &SharedString,
    absorbs: bool,
) -> bool {
    if absorbs {
        absorbing.insert(key.clone())
    } else {
        absorbing.remove(key);
        false
    }
}

/// What fold a fleet group takes now, if any: a group seen for the first
/// time folds when it starts folded (`fold`), and a group that began to
/// take in its network's rows (`began`) folds or opens as `fold` says, so
/// a folded group never hides the network's rows the network's own group
/// showed. `None` leaves the person's choice.
pub(super) fn fold_to(first: bool, began: bool, fold: bool) -> Option<bool> {
    match (first, began) {
        (_, true) => Some(fold),
        (true, false) => fold.then_some(true),
        (false, false) => None,
    }
}

/// Whether a fleet group seen for the first time starts folded: never
/// while it holds the selection (`keeps`); as the network's own group was
/// (`network`) when it took in that group's rows; else while another
/// fleet's initiative is selected (`other`).
pub(super) fn starts_folded(other: bool, keeps: bool, network: Option<bool>) -> bool {
    !keeps && network.unwrap_or(other)
}

/// The group the selection has just moved into: `holding` when it differs
/// from `previous`, the group that held it at the last layout.
pub(super) fn moved_into<'a>(
    previous: Option<&SharedString>,
    holding: Option<&'a SharedString>,
) -> Option<&'a SharedString> {
    holding.filter(|holding| previous != Some(*holding))
}

/// The key of the group in `groups` that holds row `selected`.
pub(super) fn holding_group<'a>(
    groups: &'a [Group],
    selected: Option<&SharedString>,
) -> Option<&'a SharedString> {
    let selected = selected?;
    groups
        .iter()
        .find(|group| group.rows.iter().any(|row| &row.key == selected))
        .map(|group| &group.key)
}

/// What a folded group's header says about the rows it hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hidden {
    Mention,
    Unread,
}

/// What a folded group hides, from each row's (mention, unread): a mention
/// over unread messages over nothing. An open group shows its rows' own
/// marks, so it says nothing.
pub(super) fn hidden(folded: bool, rows: impl IntoIterator<Item = (bool, bool)>) -> Option<Hidden> {
    if !folded {
        return None;
    }
    let (mut mention, mut unread) = (false, false);
    for (row_mention, row_unread) in rows {
        mention |= row_mention;
        unread |= row_unread;
    }
    match (mention, unread) {
        (true, _) => Some(Hidden::Mention),
        (false, true) => Some(Hidden::Unread),
        (false, false) => None,
    }
}

/// The marks on group `group`'s header: a folded group with a mention or
/// unread messages in it says so, as its rows would.
pub(super) fn group_indicators(board: &Irc, group: &Group, cx: &App) -> Vec<GroupIndicator> {
    let rows = group.rows.iter().map(|row| {
        let mention = row
            .trailing
            .iter()
            .any(|glyph| glyph.text.as_ref() == rows::MENTION);
        (mention, unread(board, &row.key, cx))
    });
    hidden(board.list.is_folded(&group.key), rows)
        .map(indicator)
        .into_iter()
        .collect()
}

fn indicator(hidden: Hidden) -> GroupIndicator {
    let (icon, tone, label) = match hidden {
        Hidden::Mention => (
            IconName::AtSign,
            Tone::Red,
            "You were mentioned in this group",
        ),
        Hidden::Unread => (
            IconName::Circle,
            Tone::Accent,
            "Unread messages in this group",
        ),
    };
    GroupIndicator {
        icon,
        tone,
        label: label.into(),
        tooltip: label.into(),
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::dashboard::avatar::{AgentIcons, MARK_SIZE_PX, MEMBER_SIZE_PX};

    use gpui_kit::TestAppContext;
    use subspace_client_irc::Target;
    use subspace_framework::dashboard::Tokens;

    use super::{Marks, captain_icon, people_mark, presence_mark, row_mark, ship_dot};
    use crate::rows::{Activity, Presence};

    /// A group folds when first seen if it starts folded, and takes its
    /// network's fold when it begins to take in the network's rows.
    #[test]
    fn a_group_folds_on_first_sight_or_as_its_network_s_when_it_begins_to_take_in_its_rows() {
        use super::fold_to;
        assert_eq!(fold_to(true, false, true), Some(true));
        assert_eq!(fold_to(true, false, false), None, "a new group is open");
        assert_eq!(
            fold_to(false, true, false),
            Some(false),
            "opens as the network's"
        );
        assert_eq!(
            fold_to(false, true, true),
            Some(true),
            "folds as the network's"
        );
        assert_eq!(
            fold_to(false, false, true),
            None,
            "the person's choice stands"
        );
    }

    /// The drawn navigator is checked by `tests/initiatives/navigator.rs`;
    /// this checks the captain's icon in each mode of the Icons setting.
    #[test]
    fn a_ship_s_captain_icon_follows_the_icons_setting() {
        let side = |icons| captain_icon("row", "aurora-lyra", icons).map(|icon| icon.side());
        assert_eq!(side(AgentIcons::Large), Some(gpui_kit::px(MEMBER_SIZE_PX)));
        assert_eq!(side(AgentIcons::Small), Some(gpui_kit::px(MARK_SIZE_PX)));
        assert_eq!(side(AgentIcons::Off), None, "the # leads instead");
    }

    /// The drawn marks are checked by `tests/initiatives/navigator.rs`;
    /// this builds each kind directly: a ship's icon or `#`, a person's
    /// dot, any other channel's `#`, the ship's dot and `People`'s glyph.
    #[gpui_kit::test]
    fn each_row_mark_builds(cx: &mut TestAppContext) {
        let (_, _, cx) = crate::tests::mount(cx, crate::tests::settings());
        let ship = Activity {
            captain: "aurora-lyra".into(),
            working: vec!["Reviewing NAV-7".into()],
            crew: None,
        };
        let target = Target::Channel("#aurora-lyra".into());
        let key = "row".into();
        let marks = |ship, presence| Marks {
            key: &key,
            ship,
            presence,
            target: &target,
        };
        let built = cx.update(|_, cx| {
            let tokens = Tokens::current(cx);
            [
                row_mark(&marks(Some(&ship), None), cx),
                row_mark(&marks(None, Some(Presence::Away)), cx),
                row_mark(&marks(None, None), cx),
                presence_mark(Presence::Here, &tokens),
                ship_dot("row", &ship, &tokens),
                people_mark(cx),
            ]
            .len()
        });
        assert_eq!(built, 6);
    }

    fn group(key: &str, rows: &[&str]) -> subspace_framework::dashboard::Group {
        use subspace_framework::dashboard::{Glyph, Row, Tone};
        subspace_framework::dashboard::Group {
            key: key.to_owned().into(),
            icon: Glyph::new("", Tone::Muted),
            name: key.to_owned().into(),
            count: None,
            rows: rows
                .iter()
                .map(|row| Row {
                    key: (*row).to_owned().into(),
                    icon: Glyph::new("", Tone::Muted),
                    id: None,
                    title: (*row).to_owned().into(),
                    trailing: Vec::new(),
                    done: false,
                    indent: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn the_group_holding_the_selection_is_found() {
        let groups = [
            group("fleet-a", &["lyra", "orion"]),
            group("fleet-b", &["vega"]),
        ];
        let vega = gpui_kit::SharedString::from("vega");
        let found = super::holding_group(&groups, Some(&vega));
        assert_eq!(found.map(|key| key.as_ref()), Some("fleet-b"));
        assert_eq!(super::holding_group(&groups, None), None);
        let gone = gpui_kit::SharedString::from("castor");
        assert_eq!(super::holding_group(&groups, Some(&gone)), None);
    }

    #[test]
    fn a_folded_group_opens_only_when_the_selection_moves_into_it() {
        use super::moved_into;
        let (a, b) = (
            gpui_kit::SharedString::from("fleet-a"),
            gpui_kit::SharedString::from("fleet-b"),
        );
        assert_eq!(moved_into(Some(&a), Some(&b)), Some(&b), "moved");
        assert_eq!(moved_into(None, Some(&b)), Some(&b), "first held");
        assert_eq!(
            moved_into(Some(&b), Some(&b)),
            None,
            "folded by hand: stays"
        );
        assert_eq!(moved_into(Some(&b), None), None, "nothing selected");
    }

    #[test]
    fn a_folded_group_says_what_it_hides() {
        use super::{Hidden, hidden};
        assert_eq!(
            hidden(true, [(false, true), (true, true)]),
            Some(Hidden::Mention)
        );
        assert_eq!(
            hidden(true, [(false, false), (false, true)]),
            Some(Hidden::Unread)
        );
        assert_eq!(hidden(true, [(false, false)]), None);
        assert_eq!(
            hidden(false, [(true, true)]),
            None,
            "an open group's rows say it"
        );
    }
}
