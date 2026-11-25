//! Owns the pending or mounted diff and the navigation it can return to.

use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};
use subspace_client_fleet::{FleetApi, navigation::OpenCrewDiff};
use subspace_framework::dashboard::{DashboardEvent, ReturnPoint};

use crate::diff::view::{DiffSource, FleetSource, GitHubSource};
use crate::diff::{DiffEvent, DiffView, OpenDiff};
use crate::{GhItem, Github, ItemKind, Toast, emit_toast};

#[derive(Default)]
pub(super) struct OpenDiffState {
    current: Option<Current>,
    // The dashboard hands over the next arrival before calling open_link.
    arrival: Option<ReturnPoint>,
}

enum Current {
    Pending(OpenCrewDiff, Option<ReturnPoint>),
    Mounted {
        view: Entity<DiffView>,
        _events: Subscription,
        route: Route,
    },
}

enum Route {
    PullRequest,
    Crew(Option<ReturnPoint>),
}

impl OpenDiffState {
    pub fn is_some(&self) -> bool {
        self.current.is_some()
    }

    pub fn is_none(&self) -> bool {
        self.current.is_none()
    }

    pub fn view(&self) -> Option<&Entity<DiffView>> {
        match &self.current {
            Some(Current::Mounted { view, .. }) => Some(view),
            _ => None,
        }
    }

    pub fn is_crew(&self) -> bool {
        matches!(
            self.current,
            Some(
                Current::Pending(..)
                    | Current::Mounted {
                        route: Route::Crew(_),
                        ..
                    }
            )
        )
    }

    pub fn arrived_from(&mut self, from: Option<ReturnPoint>) {
        self.arrival = from;
    }

    fn replace(
        &mut self,
        next: Option<Current>,
        window: Option<&mut Window>,
        cx: &mut Context<Github>,
    ) -> Option<ReturnPoint> {
        self.arrival = None;
        match std::mem::replace(&mut self.current, next) {
            Some(Current::Mounted { view, route, .. }) => {
                view.update(cx, |diff, cx| diff.close(window, cx));
                match route {
                    Route::PullRequest => None,
                    Route::Crew(origin) => origin,
                }
            }
            Some(Current::Pending(_, origin)) => origin,
            None => None,
        }
    }

    pub fn discard(&mut self, window: Option<&mut Window>, cx: &mut Context<Github>) {
        let _ = self.replace(None, window, cx);
    }
}

impl Github {
    pub(super) fn open_crew_diff(&mut self, target: OpenCrewDiff, cx: &mut Context<Self>) -> bool {
        if !FleetApi::current(cx).is_some_and(|api| api.read(cx).fleet(&target.fleet).is_some()) {
            return false;
        }
        let origin = self.diff.arrival.take();
        let _ = self
            .diff
            .replace(Some(Current::Pending(target, origin)), None, cx);
        self.close_confirmation = None;
        self.composer = None;
        cx.notify();
        true
    }

    pub(super) fn mount_pending_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.diff.current, Some(Current::Pending(..)))
            && let Some(Current::Pending(target, origin)) = self.diff.current.take()
        {
            self.mount_diff(
                DiffSource::Fleet(FleetSource { target }),
                Route::Crew(origin),
                window,
                cx,
            );
        }
    }

    pub(super) fn open_diff(&mut self, item: GhItem, window: &mut Window, cx: &mut Context<Self>) {
        if item.kind != ItemKind::Pr {
            emit_toast(Toast::error("Issues have no files"), cx);
            return;
        }
        let Some(login) = self.account.clone() else {
            return;
        };
        let source = DiffSource::GitHub(GitHubSource {
            item: Box::new(item),
            login,
            client: self.client.clone(),
        });
        self.mount_diff(source, Route::PullRequest, window, cx);
    }

    fn mount_diff(
        &mut self,
        source: DiffSource,
        route: Route,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = OpenDiff {
            source,
            focus: self.focus.clone(),
        };
        let view = cx.new(|cx| DiffView::new(open, window, cx));
        let events = cx.subscribe_in(&view, window, |board, _, event, window, cx| match event {
            DiffEvent::Close => board.close_diff(window, cx),
            DiffEvent::Toast(toast) => emit_toast(toast.clone(), cx),
        });
        self.composer = None;
        let _ = self.diff.replace(
            Some(Current::Mounted {
                view,
                _events: events,
                route,
            }),
            Some(window),
            cx,
        );
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer.take().is_some() {
            cx.notify();
            return;
        }
        if let Some(origin) = self.diff.replace(None, Some(window), cx) {
            cx.emit(DashboardEvent::Return(origin));
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}
