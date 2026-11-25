//! A single crew member inside the Ship tab. The live roster controls
//! presence and asks for a fresh detail read when the member changes.

mod detail;
pub(super) mod list;
mod state;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, IntoElement as _,
    ParentElement as _, Styled as _, Subscription, Task, Window,
};
use subspace_client_fleet::{FleetApi, FleetState};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, feedback};

use crate::{Irc, live::ShipLive};
use state::{DetailState, Roster};

pub(crate) struct CrewFocus {
    pub(super) ship_tab: FocusHandle,
    row: FocusHandle,
    returned: Option<String>,
}

impl CrewFocus {
    pub(crate) fn new(cx: &mut Context<Irc>) -> Self {
        Self {
            ship_tab: cx.focus_handle().tab_stop(true),
            row: cx.focus_handle().tab_stop(true),
            returned: None,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.returned = None;
    }
}

pub(crate) struct Selection {
    fleet: String,
    ship: String,
    member: String,
    state: DetailState,
    back: FocusHandle,
    client_state: Option<FleetState>,
    request: Option<Task<()>>,
    _updates: Subscription,
}

pub(super) fn open(
    board: &mut Irc,
    target: &(String, String, String),
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let Some(api) = FleetApi::current(cx) else {
        return;
    };
    let (fleet, ship, member) = target;
    let service = api.read(cx);
    let state = DetailState::new(
        roster_of(service, fleet, ship, member),
        service.directory_revision(),
    );
    let back = cx.focus_handle().tab_stop(true);
    back.focus(window, cx);
    board.inspector_crew = Some(Selection {
        fleet: fleet.clone(),
        ship: ship.clone(),
        member: member.clone(),
        state,
        back,
        client_state: None,
        request: None,
        // The selected member may move beyond the Ship tab's bounded list.
        _updates: cx.observe(&api, sync),
    });
    sync(board, api, cx);
    cx.notify();
}

fn roster_of(api: &FleetApi, fleet: &str, ship: &str, member: &str) -> Roster {
    Roster::find(api.ship_detail(fleet, ship), member)
}

fn sync(board: &mut Irc, api: Entity<FleetApi>, cx: &mut Context<Irc>) {
    let Some(selection) = &mut board.inspector_crew else {
        return;
    };
    let roster_changed = selection.update(api.read(cx));
    let client_changed = selection.refresh(&api, cx);
    if roster_changed || client_changed {
        cx.notify();
    }
}

impl Selection {
    fn update(&mut self, api: &FleetApi) -> bool {
        let next = roster_of(api, &self.fleet, &self.ship, &self.member);
        self.state.update(next, api.directory_revision())
    }

    fn refresh(&mut self, api: &Entity<FleetApi>, cx: &mut Context<Irc>) -> bool {
        let client = api.read(cx).client(&self.fleet);
        let state = client.as_ref().err().cloned();
        let changed = self.client_state != state;
        self.client_state = state;
        if let Ok(client) = client
            && let Some(generation) = self.state.begin()
        {
            self.request = Some(self.read(client, generation, api, cx));
        }
        changed
    }

    fn read(
        &self,
        client: subspace_client_fleet::FleetClient,
        generation: u64,
        api: &Entity<FleetApi>,
        cx: &mut Context<Irc>,
    ) -> Task<()> {
        let (ship, member) = (self.ship.clone(), self.member.clone());
        let api = api.clone();
        // The Back control belongs to this selection, even if the same row
        // is closed and opened again before the background read returns.
        let selection_id = self.back.clone();
        cx.spawn(async move |board, cx| {
            let result = cx
                .background_spawn(
                    async move { client.crew(&ship, &member).map(|detail| detail.member) },
                )
                .await;
            _ = board.update(cx, |board, cx| {
                if let Some(selection) = &mut board.inspector_crew
                    && selection.back == selection_id
                {
                    selection.update(api.read(cx));
                    selection.state.finish(generation, result);
                    selection.refresh(&api, cx);
                    cx.notify();
                }
            });
        })
    }

    fn connection_note(&self) -> Option<String> {
        let (label, reason) = super::reason::reason(self.client_state.as_ref()?)?;
        let fallback = if self.state.has_full_detail() {
            "Showing the last available detail and current roster tasks."
        } else {
            "Showing the ship's roster data."
        };
        Some(format!("{label}: {reason} {fallback}"))
    }
}

fn close(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    let Some(selection) = board.inspector_crew.take() else {
        return;
    };
    let ship = super::front(board, cx).and_then(|front| front.fleet?.ship(&selection.ship));
    let row = ship.as_ref().and_then(|ship| selection.return_row(ship));
    let focus = &mut board.inspector_crew_focus;
    focus.returned = row.map(str::to_owned);
    if row.is_some() {
        focus.row.focus(window, cx);
    } else {
        focus.ship_tab.focus(window, cx);
    }
    cx.notify();
}

impl Selection {
    fn return_row<'a>(&self, ship: &'a ShipLive) -> Option<&'a str> {
        let mut rows = ship.members.iter().filter(|member| {
            !matches!(self.state.roster, Roster::Gone) || member.id != self.member
        });
        let first = rows.next()?;
        if first.id == self.member {
            return Some(&first.id);
        }
        rows.find(|member| member.id == self.member)
            .map_or(Some(first.id.as_str()), |member| Some(member.id.as_str()))
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
    let (member, note) = selection
        .state
        .shown(ship.is_some_and(|ship| ship.unavailable.is_none()));
    let tokens = Tokens::current(cx);
    let back = action_button(
        "irc-crew-back",
        Intent::Quiet,
        Some(IconName::ArrowLeft),
        "Back to Ship",
    )
    .track_focus(&selection.back)
    .on_click(cx.listener(|board, _, window, cx| close(board, window, cx)));
    let connection = member.as_ref().and_then(|_| selection.connection_note());
    let note = note
        .map(str::to_owned)
        .or(connection)
        .map(|note| feedback::empty_line(&tokens, "irc-crew-note", note));
    let detail = member
        .zip(board.dock.focused.as_ref())
        .map(|(member, buffer)| {
            detail::view(&member, selection.state.current_task(&member), buffer, cx)
        });
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
