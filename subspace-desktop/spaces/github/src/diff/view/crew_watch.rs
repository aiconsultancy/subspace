//! Watch the selected checkout through the shared fleet feed while visible.

use gpui_kit::Context;
use subspace_client_fleet::{FleetApi, navigation::OpenCrewDiff};

use super::{DiffSource, DiffView};

impl DiffView {
    pub(crate) fn follow_crew(&mut self, cx: &mut Context<Self>) {
        let DiffSource::Fleet(source) = &self.source else {
            return;
        };
        if self.fleet.watch.is_some() {
            return;
        }
        let Some(api) = FleetApi::current(cx) else {
            return;
        };
        let target = source.target.clone();
        let follow = api.update(cx, |api, cx| api.follow(&target.fleet, cx));
        self.fleet.live_fingerprint = fingerprint(api.read(cx), &target);
        let observe = cx.observe(&api, move |view, api, cx| {
            let changed = view.fleet.changed();
            view.fleet
                .observe_fingerprint(fingerprint(api.read(cx), &target));
            if changed != view.fleet.changed() {
                cx.notify();
            }
        });
        self.fleet.watch = Some((follow, observe));
        cx.notify();
    }

    pub(crate) fn unfollow_crew(&mut self) {
        self.fleet.watch = None;
    }

    pub(super) fn seed_crew_fingerprint(&mut self, cx: &gpui_kit::App) {
        let Some(api) = FleetApi::current(cx) else {
            return;
        };
        if let DiffSource::Fleet(source) = &self.source {
            self.fleet.live_fingerprint = fingerprint(api.read(cx), &source.target);
        }
    }
}

fn fingerprint(api: &FleetApi, target: &OpenCrewDiff) -> Option<String> {
    api.crew_fingerprint(
        &target.fleet,
        &target.ship,
        &target.member,
        target.project.as_deref(),
    )
}
