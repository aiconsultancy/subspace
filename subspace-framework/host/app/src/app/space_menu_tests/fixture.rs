//! The three plain Spaces the menu tests open, and the product that lists
//! them.

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    Render, Styled as _, Window, div,
};
use subspace_dashboard::{Dashboard, DashboardEvent};
use subspace_product::{BoardDescriptor, ValidatedProduct, validate_product};

use super::{IDS, TITLES};

/// One of three plain Spaces that can take focus.
pub(super) struct Plain<const N: usize> {
    focus: FocusHandle,
}
impl<const N: usize> EventEmitter<DashboardEvent> for Plain<N> {}
impl<const N: usize> Focusable for Plain<N> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl<const N: usize> Render for Plain<N> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).size_full()
    }
}
impl<const N: usize> Dashboard for Plain<N> {
    const ID: &'static str = IDS[N];
    const TITLE: &'static str = TITLES[N];
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

/// The product: Gamma is off by default, as some real Spaces are.
pub(super) fn product() -> ValidatedProduct {
    let mut descriptor_set = subspace_product::fixture::descriptor();
    descriptor_set.boards = IDS
        .iter()
        .map(|id| BoardDescriptor {
            id: (*id).into(),
            aliases: Vec::new(),
            former_ids: Vec::new(),
            default_enabled: *id != "gamma",
        })
        .collect();
    validate_product(&descriptor_set).unwrap()
}
