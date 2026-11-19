//! Framework Search view and revalidated internal result navigation.
use std::rc::Rc;

use crate::toolbar_labels::ToolbarControl;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Selectable as _, Sizable as _};
use gpui_kit::{AppContext as _, Context, Focusable as _, IntoElement, Window};
use subspace_dashboard::{SearchNavigation, Toast, full_text_service, prefs};
use subspace_lucide::IconName;
use subspace_search::{CachedRead, Hit, ReadPermit, Target};
use subspace_search_ui::{SearchView, SearchViewEvent};

use super::toolbar::style::shell_button;
use super::{ACTIVE_KEY, CloseSearchView, MainSurface, OpenSearchView, SubspaceApp};

#[cfg(test)]
mod tests;

impl SubspaceApp {
    pub(super) fn install_search_navigation(cx: &mut Context<Self>) {
        let app = cx.weak_entity();
        cx.set_global(SearchNavigation(Rc::new(move |target, window, cx| {
            let _ = app.update(cx, |app, cx| app.open_indexed_target(target, window, cx));
        })));
    }

    pub(super) fn search_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        shell_button(
            Button::new("open-full-text-search")
                .ghost()
                .small()
                .accessibility_label("Search Space content")
                .tooltip("Search content (Cmd/Ctrl+Shift+F)")
                .selected(self.main.search().is_some())
                .on_click(cx.listener(|app, _, window, cx| {
                    app.open_full_text(&OpenSearchView, window, cx)
                })),
            ToolbarControl::Search,
            IconName::Search,
            cx,
        )
    }

    pub(super) fn open_full_text(
        &mut self,
        _: &OpenSearchView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.main.search() {
            view.focus_handle(cx).focus(window, cx);
            return;
        }
        let Some(service) = full_text_service(cx) else {
            self.push_toast(
                Toast::info("No participating boards are available for full-text search."),
                cx,
            );
            return;
        };
        if !self.close_jig(window, cx) {
            return;
        }
        let current = self
            .dashboard()
            .map_or_else(String::new, |board| board.id.to_owned());
        let boards = self
            .dashboards
            .iter()
            .map(|board| (board.id.to_owned(), board.title.to_string()))
            .collect();
        self.session = None;
        self.help_open = false;
        let view = cx.new(|cx| SearchView::new(service, boards, current, window, cx));
        let subscription =
            cx.subscribe_in(&view, window, |app, _, event, window, cx| match event {
                SearchViewEvent::Close => app.close_full_text(&CloseSearchView, window, cx),
                SearchViewEvent::Open(hit) => {
                    app.open_indexed_target(hit.target.clone(), window, cx)
                }
            });
        let focus = view.focus_handle(cx);
        let next = MainSurface::Search {
            view,
            _subscription: subscription,
        };
        self.set_main(next, window, cx);
        focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_full_text(
        &mut self,
        _: &CloseSearchView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.main.search().is_none() {
            return;
        }
        self.search_navigation = self.search_navigation.wrapping_add(1);
        self.set_main(MainSurface::Board, window, cx);
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn open_indexed_target(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(service) = full_text_service(cx) else {
            return;
        };
        self.search_navigation = self.search_navigation.wrapping_add(1);
        let generation = self.search_navigation;
        let result = cx.background_spawn(async move {
            service
                .read_fenced(&target)
                .map(|document| (target, document))
        });
        cx.spawn_in(window, async move |app, cx| {
            let result = result.await;
            let _ = app.update_in(cx, |app, window, cx| {
                if app.search_navigation != generation {
                    return;
                }
                match navigation_hit(result) {
                    Some((hit, permit)) => app.reveal_indexed_hit(hit, permit, window, cx),
                    None => app.push_toast(
                        Toast::info(
                            "This search result is no longer available in the selected account.",
                        ),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    fn reveal_indexed_hit(
        &mut self,
        hit: Hit,
        permit: ReadPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !permit.valid() {
            self.push_toast(
                Toast::info("This cached result is no longer available."),
                cx,
            );
            return;
        }
        let Some(index) = self
            .dashboards
            .iter()
            .position(|board| board.id == hit.target.scope.board)
        else {
            return;
        };
        self.note_arrival(index, cx);
        if self.leave_main()
            && let Some(board) = self.dashboard()
        {
            board.deactivate(window, cx);
        }
        self.notification_unavailable = false;
        self.active = index;
        prefs::set(cx, ACTIVE_KEY, self.dashboards[index].id);
        // No normal activation here: revealing retained content must not fetch or mark it read.
        self.dashboards[index].open_indexed_result_fenced(&hit, permit, window, cx);
        self.focus_active(window, cx);
        cx.notify();
    }
}

fn navigation_hit(
    result: Result<(Target, Option<CachedRead>), String>,
) -> Option<(Hit, ReadPermit)> {
    let (target, cached) = result.ok()?;
    cached.filter(|cached| cached.permit.valid()).map(|cached| {
        (
            Hit {
                target,
                document: cached.document,
                excerpt: String::new(),
                score: 0.0,
            },
            cached.permit,
        )
    })
}
