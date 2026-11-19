//! Behaviour-only adapters: scope the native text registry and its scroll owner.
use super::Session;
use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, Entity, Global, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, ScrollHandle, Window,
};

#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) session: Entity<Session>,
    pub(crate) scroll: Option<ScrollHandle>,
    pub(crate) identity: Option<gpui_kit::SharedString>,
    enabled: bool,
    order: std::rc::Rc<std::cell::Cell<usize>>,
}
#[derive(Default)]
struct Stack(Vec<Scope>);
impl Global for Stack {}

pub(crate) fn current(cx: &App) -> Option<Scope> {
    cx.try_global::<Stack>()
        .and_then(|stack| stack.0.last())
        .filter(|scope| scope.enabled)
        .cloned()
}

pub(super) fn wrap(session: Entity<Session>, child: impl IntoElement) -> Scoped {
    Scoped {
        root: true,
        scope: Scope {
            session,
            scroll: None,
            identity: None,
            enabled: true,
            order: Default::default(),
        },
        child: child.into_any_element(),
    }
}

pub(crate) fn scroll_scope(scroll: ScrollHandle, child: impl IntoElement) -> impl IntoElement {
    ScrollScope {
        scroll,
        child: child.into_any_element(),
    }
}

pub(super) struct Scoped {
    root: bool,
    scope: Scope,
    child: AnyElement,
}
struct ScrollScope {
    scroll: ScrollHandle,
    child: AnyElement,
}

fn enter<T>(scope: Scope, cx: &mut App, f: impl FnOnce(&mut App) -> T) -> T {
    cx.default_global::<Stack>().0.push(scope);
    let result = f(cx);
    cx.default_global::<Stack>().0.pop();
    result
}

impl IntoElement for Scoped {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Scoped {
    type RequestLayoutState = <AnyElement as Element>::RequestLayoutState;
    type PrepaintState = <AnyElement as Element>::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        enter(self.scope.clone(), cx, |cx| {
            Element::request_layout(&mut self.child, id, inspector, window, cx)
        })
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if self.root {
            self.scope.order.set(0);
        }
        enter(self.scope.clone(), cx, |cx| {
            Element::prepaint(&mut self.child, id, inspector, bounds, layout, window, cx)
        })
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        paint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        enter(self.scope.clone(), cx, |cx| {
            Element::paint(
                &mut self.child,
                id,
                inspector,
                bounds,
                layout,
                paint,
                window,
                cx,
            )
        });
    }
}
impl IntoElement for ScrollScope {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for ScrollScope {
    type RequestLayoutState = <AnyElement as Element>::RequestLayoutState;
    type PrepaintState = <AnyElement as Element>::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let Some(mut scope) = current(cx) else {
            return Element::request_layout(&mut self.child, id, inspector, window, cx);
        };
        scope.scroll = Some(self.scroll.clone());
        enter(scope, cx, |cx| {
            Element::request_layout(&mut self.child, id, inspector, window, cx)
        })
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let Some(mut scope) = current(cx) else {
            return Element::prepaint(&mut self.child, id, inspector, bounds, layout, window, cx);
        };
        scope.scroll = Some(self.scroll.clone());
        enter(scope, cx, |cx| {
            Element::prepaint(&mut self.child, id, inspector, bounds, layout, window, cx)
        })
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        paint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(mut scope) = current(cx) else {
            return Element::paint(
                &mut self.child,
                id,
                inspector,
                bounds,
                layout,
                paint,
                window,
                cx,
            );
        };
        scope.scroll = Some(self.scroll.clone());
        enter(scope, cx, |cx| {
            Element::paint(
                &mut self.child,
                id,
                inspector,
                bounds,
                layout,
                paint,
                window,
                cx,
            )
        });
    }
}

pub(crate) fn reveal(scroll: &ScrollHandle, bounds: Bounds<Pixels>, window: &mut Window) {
    let viewport = scroll.bounds();
    let delta = if bounds.top() < viewport.top() {
        viewport.top() - bounds.top()
    } else if bounds.bottom() > viewport.bottom() {
        viewport.bottom() - bounds.bottom()
    } else {
        return;
    };
    let mut offset = scroll.offset();
    offset.y += delta;
    scroll.set_offset(offset);
    window.refresh();
}

/// Restrict participation to the focused pane of a composite Space.
pub(crate) fn identified(
    identity: gpui_kit::SharedString,
    enabled: bool,
    child: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let Some(mut scope) = current(cx) else {
        return child.into_any_element();
    };
    scope.identity = Some(identity);
    scope.enabled = enabled;
    Scoped {
        root: false,
        scope,
        child: child.into_any_element(),
    }
    .into_any_element()
}
mod position;
pub(crate) use position::measured;
