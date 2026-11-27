//! Retained dashboard reconciliation after a configured-provider refresh.

use super::{ACTIVE_KEY, SubspaceApp};
use crate::registry::{BoardFactories, BoardRegistration};
use gpui_kit::{Context, Window};
use std::collections::HashMap;
use std::sync::Arc;
use subspace_dashboard::{DashboardHandle, prefs};

impl SubspaceApp {
    pub(super) fn reconcile_boards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let registrations = cx.global::<BoardFactories>().0.clone();
        let active = self.dashboards.get(self.active).map(|board| board.id);
        let changed = changed_ids(&self.board_registrations, &registrations);
        self.deactivate_replaced(&registrations, &changed, window, cx);
        self.replace_handles(registrations, &changed, window, cx);
        self.reconcile_board_context(active, &changed, window, cx);
        self.reconcile_settings(window, cx);
        cx.notify();
    }

    fn deactivate_replaced(
        &self,
        registrations: &[BoardRegistration],
        changed: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for board in &self.dashboards {
            if changed.contains(&board.id) || !registrations.iter().any(|next| next.id == board.id)
            {
                board.deactivate(window, cx);
            }
        }
    }

    fn replace_handles(
        &mut self,
        registrations: Vec<BoardRegistration>,
        changed: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.dashboards.get(self.active).map(|board| board.id);
        self.board_subscriptions.clear();
        let previous = Previous {
            handles: std::mem::take(&mut self.dashboards)
                .into_iter()
                .map(|board| (board.id, board))
                .collect(),
            registrations: std::mem::take(&mut self.board_registrations),
            parked: std::mem::take(&mut self.parked),
        };
        (self.dashboards, self.parked) =
            reconcile_handles(previous, &registrations, changed, window, cx);
        self.active = self
            .dashboards
            .iter()
            .position(|board| Some(board.id) == active)
            .unwrap_or_default();
        self.board_registrations = registrations;
        self.board_subscriptions = Self::dashboard_subscriptions(&self.dashboards, window, cx);
        for board in &self.dashboards {
            board.covered(self.cover.covered, window, cx);
        }
    }

    fn reconcile_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(settings) = &self.settings {
            settings.update(cx, |settings, cx| settings.reconcile_boards(window, cx));
        }
    }

    fn reconcile_board_context(
        &mut self,
        previous: Option<&str>,
        changed: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recent
            .retain(|id, _| self.dashboards.iter().any(|board| board.id == *id));
        for board in &self.dashboards {
            self.recent
                .entry(board.id)
                .or_insert_with(|| prefs::get_list(cx, &super::recent_key(board.id)));
        }
        self.session = None;
        let active = self.dashboards.get(self.active).map(|board| board.id);
        if active != previous || active.is_some_and(|id| changed.contains(&id)) {
            subspace_dashboard::take_selection(cx);
            self.notification_unavailable = false;
            self.search_navigation = self.search_navigation.wrapping_add(1);
            if let Some(id) = active {
                prefs::set(cx, ACTIVE_KEY, id);
            } else {
                prefs::set(cx, ACTIVE_KEY, "");
            }
            self.activate_dashboard(window, cx);
            if self.settings.is_none() {
                self.focus_active(window, cx);
            }
        }
    }
}

fn changed_ids(before: &[BoardRegistration], after: &[BoardRegistration]) -> Vec<&'static str> {
    after
        .iter()
        .filter(|next| {
            before
                .iter()
                .find(|old| old.id == next.id)
                .is_none_or(|old| !Arc::ptr_eq(&old.create, &next.create))
        })
        .map(|board| board.id)
        .collect()
}

/// The view of a disabled board that keeps running processes, such as
/// shells or tunnels. It has no tab and no background refresh; enabling the
/// same registration again shows it.
pub(super) struct Parked {
    registration: BoardRegistration,
    pub(super) handle: DashboardHandle,
}

/// The views and registrations before a reconcile.
struct Previous {
    handles: HashMap<&'static str, DashboardHandle>,
    registrations: Vec<BoardRegistration>,
    parked: Vec<Parked>,
}

/// The views of `registrations`, in order, and the views to keep parked.
/// A view is kept, restored from the parked views, or created.
fn reconcile_handles(
    mut previous: Previous,
    registrations: &[BoardRegistration],
    changed: &[&str],
    window: &mut Window,
    cx: &mut gpui_kit::App,
) -> (Vec<DashboardHandle>, Vec<Parked>) {
    let shown = registrations
        .iter()
        .map(|registration| {
            take_retained(&mut previous, registration, changed)
                .unwrap_or_else(|| create_handle(registration, window, cx))
        })
        .collect();
    (shown, park_removed(previous, registrations))
}

fn take_retained(
    previous: &mut Previous,
    registration: &BoardRegistration,
    changed: &[&str],
) -> Option<DashboardHandle> {
    if !changed.contains(&registration.id)
        && let Some(board) = previous.handles.remove(registration.id)
    {
        return Some(board);
    }
    let index = previous.parked.iter().position(|parked| {
        parked.registration.id == registration.id
            && Arc::ptr_eq(&parked.registration.create, &registration.create)
    })?;
    Some(previous.parked.swap_remove(index).handle)
}

fn create_handle(
    registration: &BoardRegistration,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) -> DashboardHandle {
    let mut board = (registration.create)(window, cx);
    board.id = registration.id;
    board.title = registration.title.clone().into();
    board.description = registration.description.clone().into();
    board
}

/// Parks the remaining views whose boards left the active set and retain
/// their views; drops the others and any parked view a new registration
/// with the same id replaces.
fn park_removed(previous: Previous, registrations: &[BoardRegistration]) -> Vec<Parked> {
    let Previous {
        mut handles,
        registrations: removed,
        parked,
    } = previous;
    let active: Vec<_> = registrations.iter().map(|board| board.id).collect();
    let kept = parked
        .into_iter()
        .filter(|parked| !active.contains(&parked.registration.id));
    let newly_parked = removed.into_iter().filter_map(|registration| {
        let handle = handles.remove(registration.id)?;
        (removal(&registration, &active) == Removal::Park).then_some(Parked {
            registration,
            handle,
        })
    });
    kept.chain(newly_parked).collect()
}

/// What happens to a retained view whose registration is not reused.
#[derive(Debug, PartialEq, Eq)]
enum Removal {
    /// Hide the view and keep its processes until the board is enabled again.
    Park,
    /// Close the view, which ends its work.
    Close,
}

/// A disabled board that declares `retain_when_disabled` is parked. A
/// board still active under a new registration was replaced and is closed.
fn removal(registration: &BoardRegistration, active: &[&str]) -> Removal {
    if registration.retain_when_disabled && !active.contains(&registration.id) {
        Removal::Park
    } else {
        Removal::Close
    }
}

#[cfg(test)]
mod hot_tests;

#[cfg(test)]
mod tests {
    use crate::{BoardRegistration, registry};
    use gpui_kit::{
        App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, TestAppContext,
        Window, div,
    };
    use std::sync::Arc;
    use subspace_core::{config::Config, settings::SettingsFile};
    use subspace_dashboard::{
        CurrentProduct, Dashboard, DashboardEvent, DashboardHandle, Services, prefs,
    };

    struct Fixture {
        focus: FocusHandle,
    }
    impl EventEmitter<DashboardEvent> for Fixture {}
    impl Focusable for Fixture {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    impl Dashboard for Fixture {
        const ID: &'static str = "fixture-board";
        const TITLE: &'static str = "Fixture";
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                focus: cx.focus_handle(),
            }
        }
        fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    }
    fn provider(config: &Config) -> Vec<BoardRegistration> {
        config
            .web_boards
            .iter()
            .map(|board| {
                BoardRegistration::configured::<Fixture>(
                    &board.id,
                    &board.title,
                    "Configured fixture",
                    DashboardHandle::new::<Fixture>,
                )
                .with_configuration(board.clone())
            })
            .collect()
    }

    fn open(
        root: &std::path::Path,
        cx: &mut TestAppContext,
    ) -> gpui_kit::WindowHandle<super::SubspaceApp> {
        let path = root.join("config.json");
        std::fs::write(&path, "{}").unwrap();
        cx.update(|cx| {
            crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
            crate::theme::initialize(cx);
            prefs::set(cx, crate::settings::CHECK_UPDATES_KEY, "0");
            let mut descriptor = subspace_product::fixture::descriptor();
            descriptor.boards = vec![subspace_product::BoardDescriptor {
                id: Fixture::ID.into(),
                aliases: Vec::new(),
                former_ids: Vec::new(),
                default_enabled: true,
            }];
            let mut context = (*cx.global::<CurrentProduct>().0).clone();
            context.product = subspace_product::validate_product(&descriptor).unwrap();
            cx.set_global(CurrentProduct(Arc::new(context)));
            let builtin = BoardRegistration::new::<Fixture>();
            registry::retain_provider(vec![builtin.clone()], Some(provider), cx);
            cx.set_global(registry::IncludedBoards(vec![builtin.clone()]));
            cx.set_global(registry::BoardFactories(vec![builtin]));
            let config = Config::default();
            cx.set_global(Services {
                config: Arc::new(config.clone()),
                credentials: Arc::new(subspace_core::credentials::Credentials::from_system(
                    &config,
                )),
                tools: subspace_agent_tools::Catalog::new(Vec::new()).unwrap(),
                search: None,
                config_file: path.clone(),
            });
        });
        cx.add_window(|window, cx| super::super::SubspaceApp::new(Vec::new(), window, cx))
    }

    fn refresh(window: gpui_kit::WindowHandle<super::SubspaceApp>, cx: &mut TestAppContext) {
        window
            .update(cx, |_, window, cx| {
                crate::reload_configured_boards(window, cx).unwrap()
            })
            .unwrap();
        cx.run_until_parked();
    }

    fn website(file: &mut SettingsFile, url: &str) {
        file.update_web_board(
            "notes",
            Some(serde_json::json!({"url":url}).as_object().unwrap().clone()),
        )
        .unwrap();
    }

    #[gpui_kit::test]
    fn disabled_configured_boards_keep_order_and_join_settings_without_a_view(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let window = open(directory.path(), cx);
        let mut file = SettingsFile::open(&directory.path().join("config.json")).unwrap();
        let mut values = file.values().clone();
        values.boards.insert("notes".into(), false);
        values.board_order = vec!["notes".into(), "fixture-board".into()];
        file.save(&values).unwrap();
        website(&mut file, "https://notes.example");
        refresh(window, cx);
        window
            .update(cx, |app, window, cx| {
                assert_eq!(app.dashboards.len(), 1);
                assert_eq!(app.dashboards[0].id, "fixture-board");
                let included = registry::included_registrations(cx);
                assert_eq!(
                    included.iter().map(|board| board.id).collect::<Vec<_>>(),
                    ["notes", "fixture-board"]
                );
                assert!(!cx.global::<Services>().config.board_enabled("notes"));
                window.remove_window();
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn live_add_replace_remove_retain_unchanged_views_and_select_a_valid_board(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let window = open(directory.path(), cx);
        let builtin_focus = window
            .update(cx, |app, _, cx| app.dashboards[0].focus_handle(cx))
            .unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        website(&mut file, "https://notes.example");
        refresh(window, cx);
        let added_focus = window
            .update(cx, |app, window, cx| {
                assert_eq!(app.dashboards.len(), 2);
                assert_eq!(app.dashboards[0].focus_handle(cx), builtin_focus);
                app.switch_to(1, window, cx);
                app.dashboards[1].focus_handle(cx)
            })
            .unwrap();
        refresh(window, cx);
        window
            .update(cx, |app, _, cx| {
                assert_eq!(app.dashboards[1].focus_handle(cx), added_focus)
            })
            .unwrap();
        website(&mut file, "https://replacement.example");
        refresh(window, cx);
        window
            .update(cx, |app, _, cx| {
                assert_ne!(app.dashboards[1].focus_handle(cx), added_focus);
                assert_eq!(app.dashboards[app.active].id, "notes");
            })
            .unwrap();
        file.update_web_board("notes", None).unwrap();
        refresh(window, cx);
        window
            .update(cx, |app, window, cx| {
                assert_eq!(app.dashboards.len(), 1);
                assert_eq!(app.dashboards[app.active].id, "fixture-board");
                assert_eq!(app.dashboards[0].focus_handle(cx), builtin_focus);
                assert!(!app.recent.contains_key("notes"));
                assert_eq!(
                    prefs::get(cx, super::super::ACTIVE_KEY).as_deref(),
                    Some("fixture-board")
                );
                window.remove_window();
            })
            .unwrap();
    }
}
