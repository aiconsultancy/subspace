//! Data-free dashboard and private preferences for navigation interaction tests.

use std::cell::RefCell;

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, Global, InteractiveElement as _,
    IntoElement, Render, SharedString, TestAppContext, Window, WindowHandle, div,
};
use subspace_dashboard::{Dashboard, DashboardEvent, prefs};

use super::super::{COLLAPSED_KEY, NAVIGATION_KEY, SubspaceApp, WIDTH_KEY};

pub(super) struct MutableBadge(pub RefCell<Option<SharedString>>);
impl Global for MutableBadge {}

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
        div().id(Self::ID).track_focus(&self.focus)
    }
}
impl Dashboard for Fixture {
    const ID: &'static str = "interaction-fixture";
    const TITLE: &'static str = "Navigation";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    fn badge(&self, cx: &App) -> Option<SharedString> {
        cx.try_global::<MutableBadge>()
            .and_then(|badge| badge.0.borrow().clone())
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

pub(super) fn open_fixture(
    cx: &mut TestAppContext,
    root: &std::path::Path,
    badge: Option<&str>,
    width: Option<&str>,
) -> WindowHandle<SubspaceApp> {
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        crate::theme::preview_tests::nominal_controls(cx);
        cx.set_global(MutableBadge(RefCell::new(badge.map(Into::into))));
        prefs::set(cx, NAVIGATION_KEY, "vertical");
        prefs::set(cx, COLLAPSED_KEY, "0");
        if let Some(width) = width {
            prefs::set(cx, WIDTH_KEY, width);
        }
        cx.set_global(crate::registry::BoardFactories(vec![
            crate::registry::BoardRegistration::new::<Fixture>(),
        ]));
    });
    cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx))
}
