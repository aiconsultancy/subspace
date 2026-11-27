//! Owns the pending or mounted diff and the navigation it can return to.

use gpui_kit::{AppContext as _, Context, Entity, FocusHandle, Subscription, Window};
use subspace_client_fleet::{FleetApi, navigation::OpenCrewDiff};
use subspace_framework::dashboard::ReturnPoint;

use crate::diff::view::{DiffSource, FleetSource};
use crate::diff::{DiffEvent, DiffView, OpenDiff};
use crate::{Github, emit_toast};

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

    pub fn queue_crew(&mut self, target: OpenCrewDiff, cx: &mut Context<Github>) -> bool {
        if !FleetApi::current(cx).is_some_and(|api| api.read(cx).fleet(&target.fleet).is_some()) {
            return false;
        }
        let origin = self.arrival.take();
        let _ = self.replace(Some(Current::Pending(target, origin)), None, cx);
        true
    }

    pub fn mount_pending(
        &mut self,
        focus: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<Github>,
    ) -> bool {
        if !matches!(self.current, Some(Current::Pending(..))) {
            return false;
        }
        let Some(Current::Pending(target, origin)) = self.current.take() else {
            return false;
        };
        let open = OpenDiff {
            source: DiffSource::Fleet(FleetSource { target }),
            focus: focus.clone(),
        };
        self.mount(open, Route::Crew(origin), window, cx);
        true
    }

    pub fn mount_pull_request(
        &mut self,
        open: OpenDiff,
        window: &mut Window,
        cx: &mut Context<Github>,
    ) {
        self.mount(open, Route::PullRequest, window, cx);
    }

    fn mount(
        &mut self,
        open: OpenDiff,
        route: Route,
        window: &mut Window,
        cx: &mut Context<Github>,
    ) {
        let focus = open.focus.clone();
        let view = cx.new(|cx| DiffView::new(open, window, cx));
        let events = cx.subscribe_in(&view, window, |board, _, event, window, cx| match event {
            DiffEvent::Close => board.close_diff(window, cx),
            DiffEvent::Toast(toast) => emit_toast(toast.clone(), cx),
        });
        let _ = self.replace(
            Some(Current::Mounted {
                view,
                _events: events,
                route,
            }),
            Some(window),
            cx,
        );
        focus.focus(window, cx);
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Github>) -> Option<ReturnPoint> {
        self.replace(None, Some(window), cx)
    }
}
