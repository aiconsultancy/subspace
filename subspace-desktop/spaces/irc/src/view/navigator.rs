//! The navigator's rows from the hub, grouped by fleet, and their marks:
//! what leads a fleet channel's row and the `People` group, which rows read
//! as unread, and which fleet groups start folded.

use std::collections::HashMap;

use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{AnyElement, App, Context, Entity, IntoElement as _, SharedString, Styled as _};
use subspace_client_irc::Target;
use subspace_framework::dashboard::avatar::{self, AgentRank};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Row, Tokens};

use super::kind_icon;
use crate::fleet::ChannelKind;
use crate::hub::Hub;
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
    let layout = rows::layout(state, &rosters);
    let selected = held.initiatives.selected_fleet();
    board.search = std::rc::Rc::new(rows::search(&layout));
    fold_new_fleets(board, &layout.fleets, selected);
    board.buffers = layout.buffers;
    board.listed = layout.listed;
    board.group_networks = layout.networks;
    board.kinds = layout.kinds;
    board.list.set_groups(layout.groups);
}

/// Whether row `key`'s buffer has unread messages.
pub(super) fn unread(board: &Irc, key: &str, cx: &App) -> bool {
    let held = board.buffers.get(key);
    let state = board.hub.as_ref().map(|hub| &hub.read(cx).state);
    held.zip(state)
        .and_then(|(key, state)| state.buffer(key))
        .is_some_and(|buffer| buffer.unread > 0)
}

/// What leads `row`: a ship's captain's small agent avatar, the Starbase's
/// or command channel's glyph in its tone, else the quiet glyph of the
/// buffer's kind.
pub(super) fn row_mark(
    row: &Row,
    kind: Option<&ChannelKind>,
    target: &Target,
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    match kind {
        Some(ChannelKind::Ship { captain }) => avatar::agent(
            format!("irc-row-avatar:{}", row.key),
            AgentRank::Captain,
            captain.clone(),
        )
        .small()
        .into_any_element(),
        Some(kind) => {
            let (icon, tone, _) = crate::conversation::kind_glyph(kind);
            Icon::from(icon)
                .xsmall()
                .text_color(tokens.tone(tone))
                .into_any_element()
        }
        None => Icon::from(kind_icon(target))
            .xsmall()
            .text_color(tokens.muted)
            .into_any_element(),
    }
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
/// fleet. With no initiative selected, every fleet starts open.
pub(super) fn fold_new_fleets(
    board: &mut Irc,
    fleets: &HashMap<SharedString, String>,
    selected: Option<Option<&str>>,
) {
    for (key, fleet) in fleets {
        let first = board.seen_fleets.insert(key.clone());
        let other = selected.is_some_and(|selected| selected != Some(fleet.as_str()));
        if first && other && !board.list.is_folded(key) {
            board.list.toggle(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;
    use subspace_client_irc::Target;
    use subspace_framework::dashboard::{Glyph, Row, Tone};

    use super::{people_mark, row_mark};
    use crate::fleet::ChannelKind;

    /// The drawn navigator is checked by `tests/initiatives/navigator.rs`;
    /// this builds each kind of mark directly.
    #[gpui_kit::test]
    fn each_fleet_row_and_people_build_their_mark(cx: &mut TestAppContext) {
        let (_, _, cx) = crate::tests::mount(cx, crate::tests::settings());
        let row = Row {
            key: "row".into(),
            icon: Glyph::new("#", Tone::Muted),
            id: None,
            title: "lyra".into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        };
        let ship = ChannelKind::Ship {
            captain: "aurora-lyra".into(),
        };
        let target = Target::Channel("#aurora-lyra".into());
        let built = cx.update(|_, cx| {
            [
                row_mark(&row, Some(&ship), &target, cx),
                row_mark(&row, Some(&ChannelKind::Starbase), &target, cx),
                row_mark(&row, None, &target, cx),
                people_mark(cx),
            ]
            .len()
        });
        assert_eq!(built, 4);
    }
}
