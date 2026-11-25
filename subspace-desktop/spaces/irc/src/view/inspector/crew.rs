//! A single crew member inside the Ship tab. The live roster controls
//! presence and asks for a fresh detail read when the member changes.

mod detail;
mod focus;
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
use focus::close;
use state::{DetailState, Roster};

pub(crate) struct CrewFocus {
    pub(super) ship_tab: FocusHandle,
    pub(super) show_all: bool,
    row: FocusHandle,
    returned: Option<String>,
}

impl CrewFocus {
    pub(crate) fn new(cx: &mut Context<Irc>) -> Self {
        Self {
            ship_tab: cx.focus_handle().tab_stop(true),
            show_all: false,
            row: cx.focus_handle().tab_stop(true),
            returned: None,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.returned = None;
        self.show_all = false;
    }
}

pub(crate) struct Selection {
    fleet: String,
    ship: String,
    member: String,
    state: DetailState,
    back: FocusHandle,
    client_state: Option<FleetState>,
    live: bool,
    request: Option<Task<()>>,
    retry_timer: Option<Task<()>>,
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
        service.connection_revision(fleet),
    );
    let live = service.state(fleet).is_live();
    let back = cx.focus_handle().tab_stop(true);
    back.focus(window, cx);
    board.inspector_crew = Some(Selection {
        fleet: fleet.clone(),
        ship: ship.clone(),
        member: member.clone(),
        state,
        back,
        client_state: None,
        live,
        request: None,
        retry_timer: None,
        // The selected member may move beyond the Ship tab's compact list.
        _updates: cx.observe(&api, sync),
    });
    sync(board, api, cx);
    cx.notify();
}

pub(crate) fn retry(board: &mut Irc, cx: &mut Context<Irc>) {
    if let Some(api) = FleetApi::current(cx)
        && let Some(selection) = &mut board.inspector_crew
    {
        selection.retry_timer = None;
        selection.request = None;
        selection.state.probe();
        sync(board, api, cx);
        cx.notify();
    }
}

pub(crate) fn visibility_changed(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.inspector_crew.is_some()
        && let Some(api) = FleetApi::current(cx)
    {
        sync(board, api, cx);
    }
}

fn roster_of(api: &FleetApi, fleet: &str, ship: &str, member: &str) -> Roster {
    Roster::find(api.ship_detail(fleet, ship), member)
}

fn sync(board: &mut Irc, api: Entity<FleetApi>, cx: &mut Context<Irc>) {
    let visible = visible(board, cx);
    let Some(selection) = &mut board.inspector_crew else {
        return;
    };
    let visibility_changed = selection.state.set_visible(visible);
    if visibility_changed {
        selection.request = None;
        selection.retry_timer = None;
    }
    let roster_changed = selection.update(api.read(cx));
    let client_changed = visible && selection.refresh(&api, cx);
    if visibility_changed || roster_changed || client_changed {
        cx.notify();
    }
}

pub(crate) fn visible(board: &Irc, cx: &gpui_kit::App) -> bool {
    let Some(selection) = &board.inspector_crew else {
        return false;
    };
    board.active
        && board.dock.members_open()
        && board.inspector_tab == super::InspectorTab::Subject
        && super::front(board, cx).is_some_and(|front| {
            crate::live::fleet_id(&front.roster, &front.channel) == Some(&selection.fleet)
                && matches!(front.subject, super::Subject::Ship { captain } if selection.ship.eq_ignore_ascii_case(&captain))
        })
}

impl Selection {
    fn update(&mut self, api: &FleetApi) -> bool {
        let connection = api.connection_revision(&self.fleet);
        let next = roster_of(api, &self.fleet, &self.ship, &self.member);
        if self.state.connection_changed(connection) || self.state.roster.member_returned(&next) {
            self.retry_timer = None;
            self.request = None;
        }
        let changed = self.state.update(next, connection);
        let live = api.state(&self.fleet).is_live();
        if live && !self.live {
            self.retry_timer = None;
            self.request = None;
            self.state.probe();
        }
        self.live = live;
        changed
    }

    fn refresh(&mut self, api: &Entity<FleetApi>, cx: &mut Context<Irc>) -> bool {
        let client = api.read(cx).client(&self.fleet);
        let state = client.as_ref().err().cloned();
        let changed = self.client_state != state;
        self.client_state = state;
        if let Ok(client) = client
            && self.retry_timer.is_none()
        {
            if let Some(generation) = self.state.begin() {
                self.request = Some(self.read(client, generation, api, cx));
            } else if let Some(delay) = self.state.retry_delay() {
                self.retry_timer = Some(self.schedule_retry(delay, api, cx));
            }
        }
        changed
    }

    fn schedule_retry(
        &self,
        mut delay: std::time::Duration,
        api: &Entity<FleetApi>,
        cx: &mut Context<Irc>,
    ) -> Task<()> {
        let selection_id = self.back.clone();
        let api = api.clone();
        cx.spawn(async move |board, cx| {
            // Retry-After is a u64 in seconds. Bound each timer's duration
            // so the platform scheduler can represent its deadline.
            const MAX_TIMER_SLICE: std::time::Duration = std::time::Duration::from_secs(86_400);
            while !delay.is_zero() {
                let slice = delay.min(MAX_TIMER_SLICE);
                cx.background_executor().timer(slice).await;
                delay -= slice;
            }
            _ = board.update(cx, |board, cx| {
                if let Some(selection) = &mut board.inspector_crew
                    && selection.back == selection_id
                {
                    selection.retry_timer = None;
                    selection.state.retry_elapsed();
                    sync(board, api, cx);
                }
            });
        })
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
                    sync(board, api, cx);
                    cx.notify();
                }
            });
        })
    }

    fn connection_note(&self, roster_shown: bool, cx: &gpui_kit::App) -> Option<AnyElement> {
        let (label, mut reason) = super::reason::reason(self.client_state.as_ref()?)?;
        if roster_shown {
            reason.push_str(if self.state.has_full_detail() {
                " Showing the last available detail and current roster tasks."
            } else {
                " Showing the ship's roster data."
            });
        }
        Some(super::reason::view(label, reason, cx))
    }
}

pub(super) fn visible_members(ship: &ShipLive, show_all: bool) -> &[crate::live::CrewLine] {
    let count = if show_all {
        ship.members.len()
    } else {
        ship.members.len().min(crate::live::ship::MAX_CREW)
    };
    &ship.members[..count]
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
    let connection = selection.connection_note(member.is_some(), cx);
    let note = note.map(|note| feedback::empty_line(&tokens, "irc-crew-note", note));
    let retry = (member.is_some() && selection.state.can_retry()).then(|| {
        action_button("irc-crew-retry", Intent::Quiet, None, "Try again")
            .on_click(cx.listener(|board, _, _, cx| self::retry(board, cx)))
    });
    let status = (connection.is_some() || note.is_some() || retry.is_some()).then(|| {
        v_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_2()
            .children(connection)
            .children(note)
            .children(retry)
    });
    let detail = member
        .zip(board.dock.focused.as_ref())
        .map(|(member, buffer)| detail::view(&member, member.current_task(), buffer, cx));
    Some(
        v_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_4()
            .child(back)
            .children(status)
            .children(detail)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests;
