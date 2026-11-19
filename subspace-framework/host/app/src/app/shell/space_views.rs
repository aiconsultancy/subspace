//! Each native Space keeps an independent find host and native focus tree.
use super::{SubspaceApp, empty_state};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, Div, ParentElement as _, Styled as _, Window, div};
use subspace_dashboard::Tokens;
impl SubspaceApp {
    pub(super) fn dashboard_views(
        &self,
        window: &mut Window,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        if let Some(view) = self.main.view() {
            return vec![div().size_full().child(view)];
        }
        if self.notification_unavailable {
            return vec![
                div()
                    .p_6()
                    .child("This notification is no longer available. Select a Space to continue."),
            ];
        }
        if self.dashboards.is_empty() {
            return vec![empty_state(tokens, cx)];
        }
        self.dashboards
            .iter()
            .enumerate()
            .map(|(index, dashboard)| {
                div()
                    .size_full()
                    .when(index != self.active, |view| view.hidden())
                    .child(dashboard.find_view(window, cx))
            })
            .collect()
    }
}
