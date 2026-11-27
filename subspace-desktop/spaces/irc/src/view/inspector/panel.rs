//! The focused Ship tab keeps the last complete read during brief interruptions.

use std::time::Duration;

use gpui_kit::{Context, Task};
use subspace_client_fleet::{FleetState, Link};
use subspace_client_irc::BufferKey;

use super::{Front, Subject, state_of};
use crate::{Irc, live::ShipLive};

const SETTLE: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(crate) struct ShipPanel {
    key: Option<(BufferKey, Option<String>, Subject)>,
    last: Option<ShipLive>,
    refresh: Option<Refresh>,
}

struct Refresh {
    _timeout: Task<()>,
    delayed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Read {
    Starting,
    Complete,
    Refreshing,
    Unavailable,
}

impl Read {
    fn of(front: &Front, ship: Option<&ShipLive>) -> Self {
        match state_of(front.fleet.as_ref()) {
            FleetState::Checking | FleetState::Ready | FleetState::Followed(Link::Connecting) => {
                Self::Starting
            }
            FleetState::Followed(Link::Reconnecting | Link::Unreachable) => Self::Refreshing,
            state if state.is_live() => match ship {
                Some(ship) if ship.detail_pending && state == FleetState::Followed(Link::Live) => {
                    Self::Starting
                }
                Some(ship)
                    if ship.detail_pending || ship.crew_kept || ship.unavailable.is_some() =>
                {
                    Self::Refreshing
                }
                Some(_) => Self::Complete,
                None if front.fleet.as_ref().is_some_and(|fleet| !fleet.listed()) => {
                    Self::Refreshing
                }
                None => Self::Unavailable,
            },
            _ => Self::Unavailable,
        }
    }
}

impl ShipPanel {
    pub(super) fn sync(front: Option<Front>, board: &mut Irc, cx: &mut Context<Irc>) {
        let Some(front) = front else {
            board.ship_panel = Self::default();
            return;
        };
        let key = front.key();
        let ship = front.ship();
        let read = Read::of(&front, ship.as_ref());
        let panel = &mut board.ship_panel;
        if panel.key.as_ref() != Some(&key) {
            *panel = Self {
                key: Some(key),
                ..Self::default()
            };
        }
        if read == Read::Refreshing {
            panel.wait(cx);
        } else {
            panel.refresh = None;
            panel.last = ship.filter(|_| read == Read::Complete);
        }
    }

    fn wait(&mut self, cx: &mut Context<Irc>) {
        if self.refresh.is_some() {
            return;
        }
        let timeout = cx.spawn(async |board, cx| {
            cx.background_executor().timer(SETTLE).await;
            _ = board.update(cx, |board, cx| {
                if let Some(refresh) = &mut board.ship_panel.refresh {
                    refresh.delayed = true;
                    cx.notify();
                }
            });
        });
        self.refresh = Some(Refresh {
            _timeout: timeout,
            delayed: false,
        });
    }

    pub(super) fn content<'a>(
        &'a self,
        front: &Front,
        current: Option<&'a ShipLive>,
    ) -> Content<'a> {
        let read = Read::of(front, current);
        let same = self.key.as_ref() == Some(&front.key());
        let last = self
            .last
            .as_ref()
            .filter(|_| same && read == Read::Refreshing);
        let delayed = same && self.refresh.as_ref().is_some_and(|refresh| refresh.delayed);
        Content {
            ship: last.or(current),
            loading: read == Read::Starting
                || (read == Read::Refreshing && last.is_none() && !delayed),
            last_known: last.is_some() && delayed,
        }
    }
}

pub(super) struct Content<'a> {
    pub ship: Option<&'a ShipLive>,
    pub loading: bool,
    pub last_known: bool,
}

impl Front {
    fn key(&self) -> (BufferKey, Option<String>, Subject) {
        (
            self.buffer.clone(),
            self.fleet_id.clone(),
            self.subject.clone(),
        )
    }

    pub(super) fn ship(&self) -> Option<ShipLive> {
        let Subject::Ship { captain } = &self.subject else {
            return None;
        };
        self.fleet.as_ref()?.ship(captain)
    }
}
