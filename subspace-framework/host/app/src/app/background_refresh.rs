//! Shell-owned automatic reads share the same policy as board-owned pollers.

use std::time::Duration;

use gpui_kit::App;
use subspace_dashboard::{
    DashboardHandle,
    refresh::{self, Schedule},
};

pub(super) fn start(cx: &mut gpui_kit::Context<super::SubspaceApp>) {
    let app = cx.weak_entity();
    cx.spawn(async move |_, cx| run(app, cx).await).detach();
}

async fn run(app: gpui_kit::WeakEntity<super::SubspaceApp>, cx: &mut gpui_kit::AsyncApp) {
    let started = std::time::Instant::now();
    let mut schedules = std::collections::HashMap::new();
    loop {
        cx.background_executor().timer(Duration::from_secs(1)).await;
        let result = app.update(cx, |app, cx| {
            tick_all(&app.dashboards, &mut schedules, started.elapsed(), cx)
        });
        if result.is_err() {
            return;
        }
    }
}

pub(super) fn tick_all(
    dashboards: &[DashboardHandle],
    schedules: &mut std::collections::HashMap<&'static str, Schedule>,
    now: Duration,
    cx: &mut App,
) {
    schedules.retain(|id, _| dashboards.iter().any(|board| board.id == *id));
    for (index, dashboard) in dashboards.iter().enumerate() {
        let schedule = schedules.entry(dashboard.id).or_insert_with(|| {
            Schedule::new(now + Duration::from_secs(super::refresh_jitter(index)))
        });
        tick(dashboard, schedule, now, cx);
    }
}

pub(super) fn tick(
    dashboard: &DashboardHandle,
    schedule: &mut Schedule,
    now: Duration,
    cx: &mut App,
) {
    let enabled = refresh::enabled(dashboard.id, cx);
    let interval = refresh::interval(
        dashboard.id,
        Duration::from_secs(super::REFRESH_INTERVAL_SECS),
        cx,
    );
    if schedule.ready(now, enabled, interval) {
        dashboard.background_refresh(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::tick;
    use gpui_kit::{
        App, Context, EventEmitter, FocusHandle, Focusable, Global, IntoElement, Render,
        TestAppContext, Window, div,
    };
    use std::time::Duration;
    use subspace_dashboard::{Dashboard, DashboardEvent, DashboardHandle, prefs, refresh};

    #[derive(Default)]
    struct Calls {
        automatic: usize,
        manual: usize,
    }
    impl Global for Calls {}

    struct Probe {
        focus: FocusHandle,
    }
    impl EventEmitter<DashboardEvent> for Probe {}
    impl Focusable for Probe {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    impl Dashboard for Probe {
        const ID: &'static str = "refresh-probe";
        const TITLE: &'static str = "Refresh probe";
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                focus: cx.focus_handle(),
            }
        }
        fn background_refresh(&mut self, cx: &mut Context<Self>) {
            cx.global_mut::<Calls>().automatic += 1;
        }
        fn reload(&mut self, _: &mut Window, cx: &mut Context<Self>) {
            cx.global_mut::<Calls>().manual += 1;
        }
    }

    #[gpui_kit::test]
    fn disabled_shell_updates_leave_manual_refresh_available(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
            cx.set_global(Calls::default());
        });
        let window = cx.add_window(|_, cx| Probe {
            focus: cx.focus_handle(),
        });
        window
            .update(cx, |_, window, cx| {
                let board = DashboardHandle::new::<Probe>(window, cx);
                let mut schedule = refresh::Schedule::new(Duration::ZERO);
                refresh::Preferences {
                    enabled: false,
                    interval_seconds: None,
                }
                .save(Probe::ID, cx);
                tick(&board, &mut schedule, Duration::from_secs(600), cx);
                board.reload(window, cx);
                assert_eq!(cx.global::<Calls>().automatic, 0);
                assert_eq!(cx.global::<Calls>().manual, 1);
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn shell_uses_the_saved_interval_and_applies_changes_without_restart(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
            cx.set_global(Calls::default());
        });
        let window = cx.add_window(|_, cx| Probe {
            focus: cx.focus_handle(),
        });
        window
            .update(cx, |_, window, cx| {
                let board = DashboardHandle::new::<Probe>(window, cx);
                let mut schedule = refresh::Schedule::new(Duration::ZERO);
                refresh::Preferences {
                    enabled: true,
                    interval_seconds: Some(300),
                }
                .save(Probe::ID, cx);
                tick(&board, &mut schedule, Duration::ZERO, cx);
                tick(&board, &mut schedule, Duration::from_secs(60), cx);
                assert_eq!(cx.global::<Calls>().automatic, 1);
                refresh::Preferences {
                    enabled: true,
                    interval_seconds: Some(30),
                }
                .save(Probe::ID, cx);
                tick(&board, &mut schedule, Duration::from_secs(60), cx);
                assert_eq!(cx.global::<Calls>().automatic, 2);
            })
            .unwrap();
    }
}
