//! Keeps a keyboard-focused control inside the decision panel's viewport.
//!
//! Tab moves focus between the panel's Kit buttons and links, which own
//! their focus handles, but GPUI does not scroll a newly focused control
//! into view. This layout-only element (it draws nothing) gives each control
//! a scope with its own focus handle, which is not a tab stop. When focus
//! first enters a scope, the panel body scrolls just far enough to show it.
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, FocusHandle, GlobalElementId, InspectorElementId,
    InteractiveElement as _, IntoElement, LayoutId, ParentElement as _, Pixels, ScrollHandle,
    SharedString, Styled as _, TestSupportExt as _, Window,
};
use std::cell::Cell;
use std::rc::Rc;

/// `child` in a reveal scope. Tests find the scope as `<key>:scope`.
pub(super) fn scope(
    key: impl Into<SharedString>,
    scroll: &ScrollHandle,
    child: impl IntoElement,
) -> RevealOnFocus {
    let key = key.into();
    RevealOnFocus {
        id: ElementId::Name(format!("{key}:reveal").into()),
        scope: ElementId::Name(format!("{key}:scope").into()),
        scroll: scroll.clone(),
        child: Some(child.into_any_element()),
    }
}

pub(super) struct RevealOnFocus {
    id: ElementId,
    scope: ElementId,
    scroll: ScrollHandle,
    child: Option<AnyElement>,
}

#[derive(Clone)]
pub(super) struct Tracked {
    focus: FocusHandle,
    focused: Rc<Cell<bool>>,
}

impl Tracked {
    /// Whether focus has just entered the scope.
    fn entered(&self, window: &Window, cx: &App) -> bool {
        let focused = self.focus.contains_focused(window, cx);
        !self.focused.replace(focused) && focused
    }
}

fn tracked(id: Option<&GlobalElementId>, window: &mut Window, cx: &mut App) -> Tracked {
    let fresh = |cx: &mut App| Tracked {
        focus: cx.focus_handle(),
        focused: Rc::new(Cell::new(false)),
    };
    match id {
        Some(id) => window.with_element_state(id, |state: Option<Tracked>, _| {
            let state = state.unwrap_or_else(|| fresh(cx));
            (state.clone(), state)
        }),
        None => fresh(cx),
    }
}

/// Scrolls `scroll` the least distance that shows `target`, its top first
/// when it is taller than the viewport.
fn reveal(scroll: &ScrollHandle, target: Bounds<Pixels>, window: &mut Window) {
    let viewport = scroll.bounds();
    let delta = if target.top() < viewport.top() || target.size.height > viewport.size.height {
        viewport.top() - target.top()
    } else if target.bottom() > viewport.bottom() {
        viewport.bottom() - target.bottom()
    } else {
        return;
    };
    let mut offset = scroll.offset();
    offset.y += delta;
    scroll.set_offset(offset);
    window.refresh();
}

impl IntoElement for RevealOnFocus {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for RevealOnFocus {
    type RequestLayoutState = (AnyElement, Tracked);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let tracked = tracked(id, window, cx);
        let mut inner = v_flex()
            .min_w_0()
            .id(self.scope.clone())
            .test_support()
            .track_focus(&tracked.focus)
            .children(self.child.take())
            .into_any_element();
        (inner.request_layout(window, cx), (inner, tracked))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (inner, tracked): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        inner.prepaint(window, cx);
        if tracked.entered(window, cx) {
            reveal(&self.scroll, bounds, window);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (inner, _): &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        inner.paint(window, cx);
    }
}
