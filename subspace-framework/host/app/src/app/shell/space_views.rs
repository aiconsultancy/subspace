//! Each native Space keeps an independent find host and native focus tree.
//!
//! Inactive Spaces are cached views under a hidden parent. GPUI renders a
//! cached view only when it prepaints it, and skips prepaint under
//! `display: none`, so a hidden Space costs nothing per frame. An inactive
//! Space that is not cached renders on every frame of the window.
use super::{SubspaceApp, empty_state};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, IntoElement as _, ParentElement as _, StyleRefinement, Styled as _, Window, div,
};
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
                let view = dashboard.find_view(window, cx);
                let active = index == self.active;
                div().size_full().map(|slot| {
                    if active {
                        slot.child(view)
                    } else {
                        let cached = view.cached(StyleRefinement::default().size_full());
                        slot.hidden().child(cached.into_any_element())
                    }
                })
            })
            .collect()
    }
}
