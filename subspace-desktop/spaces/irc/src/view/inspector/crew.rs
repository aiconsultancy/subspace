//! A single crew member inside the Ship tab. The one-shot read is scoped
//! to this selection; live ship updates take over as soon as the roster
//! changes, including when the member leaves.

mod detail;
pub(super) mod list;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, IntoElement as _, ParentElement as _,
    Styled as _, Subscription, Task,
};
use subspace_client_fleet::model::{CrewMember, ShipDetail};
use subspace_client_fleet::{Error, FleetApi};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, feedback};

use crate::{Irc, live::ShipLive};

pub(crate) struct Selection {
    fleet: String,
    ship: String,
    member: String,
    directory_revision: u64,
    roster: Option<ShipDetail>,
    follow_live: bool,
    read: Option<Result<CrewMember, Error>>,
    request: Option<Task<()>>,
    _updates: Subscription,
}

pub(super) fn open(board: &mut Irc, fleet: &str, ship: &str, member: &str, cx: &mut Context<Irc>) {
    let Some(api) = FleetApi::current(cx) else {
        return;
    };
    let service = api.read(cx);
    let directory_revision = service.directory_revision();
    let roster = detail_of(service, fleet, ship);
    let client = service.client(fleet).ok();
    let request = client.map(|client| {
        let (ship, member) = (ship.to_owned(), member.to_owned());
        cx.spawn(async move |board, cx| {
            let result = cx
                .background_spawn(
                    async move { client.crew(&ship, &member).map(|detail| detail.member) },
                )
                .await;
            _ = board.update(cx, |board, cx| {
                if let Some(selection) = &mut board.inspector_crew {
                    selection.read = Some(result);
                    cx.notify();
                }
            });
        })
    });
    board.inspector_crew = Some(Selection {
        fleet: fleet.to_owned(),
        ship: ship.to_owned(),
        member: member.to_owned(),
        directory_revision,
        roster,
        follow_live: false,
        read: None,
        request,
        // The selected member may move beyond the Ship tab's bounded list.
        _updates: cx.observe(&api, sync),
    });
    cx.notify();
}

fn detail_of(api: &FleetApi, fleet: &str, ship: &str) -> Option<ShipDetail> {
    api.snapshot(fleet)?.ships.remove(ship)
}

fn sync(board: &mut Irc, api: Entity<FleetApi>, cx: &mut Context<Irc>) {
    if let Some(selection) = board.inspector_crew.as_mut()
        && selection.sync(api.read(cx))
    {
        cx.notify();
    }
}

impl Selection {
    fn sync(&mut self, api: &FleetApi) -> bool {
        let next = detail_of(api, &self.fleet, &self.ship);
        let previous = Roster::find(self.roster.as_ref(), &self.member);
        let changed = previous.changed(Roster::find(next.as_ref(), &self.member));
        self.update(next, api.directory_revision(), changed)
    }

    fn update(&mut self, next: Option<ShipDetail>, revision: u64, changed: bool) -> bool {
        let directory_changed = revision != self.directory_revision;
        if directory_changed {
            self.directory_revision = revision;
            self.request = None;
            self.read = None;
        }
        self.follow_live |= directory_changed || changed;
        self.roster = next;
        directory_changed || changed
    }
}

#[derive(PartialEq, Eq)]
enum Roster<'a> {
    Present(&'a CrewMember),
    Gone,
    Unavailable,
}

impl<'a> Roster<'a> {
    fn changed(self, next: Self) -> bool {
        self != next
    }

    fn find(detail: Option<&'a ShipDetail>, member: &str) -> Self {
        let Some(detail) = detail.filter(|detail| detail.ship.available && !detail.detail_stale)
        else {
            return Self::Unavailable;
        };
        detail
            .crew_members
            .iter()
            .find(|entry| entry.id == member)
            .map_or(Self::Gone, Self::Present)
    }
}

fn shown<'a>(
    roster: Roster<'a>,
    follow_live: bool,
    read: Option<&'a Result<CrewMember, Error>>,
) -> (Option<&'a CrewMember>, Option<&'static str>) {
    let member = match roster {
        Roster::Present(member) => member,
        Roster::Gone => {
            return (None, Some("This member is no longer on the ship's roster."));
        }
        Roster::Unavailable => {
            return (None, Some("The ship's current crew roster is unavailable."));
        }
    };
    match read {
        Some(Ok(detail)) if !follow_live => (Some(detail), None),
        Some(Ok(_)) => (Some(member), None),
        Some(Err(Error::NotFound)) => (
            Some(member),
            Some("Crew detail is not available on this gateway. Showing the ship's roster data."),
        ),
        Some(Err(_)) => (
            Some(member),
            Some("Could not load crew detail. Showing the ship's roster data."),
        ),
        None => (Some(member), None),
    }
}

pub(super) fn body(
    board: &Irc,
    fleet: Option<&str>,
    captain: &str,
    ship: Option<&ShipLive>,
    cx: &mut Context<Irc>,
) -> Option<AnyElement> {
    let selection = board.inspector_crew.as_ref().filter(|selection| {
        fleet == Some(&selection.fleet) && selection.ship.eq_ignore_ascii_case(captain)
    })?;
    let roster = ship
        .filter(|ship| ship.unavailable.is_none())
        .and(selection.roster.as_ref());
    let (member, note) = shown(
        Roster::find(roster, &selection.member),
        selection.follow_live,
        selection.read.as_ref(),
    );
    let tokens = Tokens::current(cx);
    let back = action_button(
        "irc-crew-back",
        Intent::Quiet,
        Some(IconName::ArrowLeft),
        "Back to Ship",
    )
    .on_click(cx.listener(|board, _, _, cx| {
        board.inspector_crew = None;
        cx.notify();
    }));
    let note = note.map(|note| feedback::empty_line(&tokens, "irc-crew-note", note));
    let detail = member
        .zip(board.dock.focused.as_ref())
        .map(|(member, buffer)| detail::view(member, buffer, cx));
    Some(
        v_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_4()
            .child(back)
            .children(note)
            .children(detail)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests;
