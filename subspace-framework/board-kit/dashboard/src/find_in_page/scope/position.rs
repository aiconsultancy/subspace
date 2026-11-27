//! Record structural reading order without drawing or changing hit testing.
use super::Scope;
use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};
pub(crate) fn measured(scope: Option<Scope>, id: ElementId, child: AnyElement) -> AnyElement {
    match scope {
        Some(scope) => Measured { scope, id, child }.into_any_element(),
        None => child,
    }
}
struct Measured {
    scope: Scope,
    id: ElementId,
    child: AnyElement,
}
impl Element for Measured {
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
        Element::request_layout(&mut self.child, id, inspector, window, cx)
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
        let rank = self.scope.order.get();
        self.scope.order.set(rank + 1);
        let changed =
            self.scope
                .session
                .read(cx)
                .order_changed(&self.id, self.scope.identity.as_ref(), rank);
        if changed {
            self.scope.session.update(cx, |session, cx| {
                session.order(&self.id, self.scope.identity.as_ref(), rank, cx)
            });
        }
        Element::prepaint(&mut self.child, id, inspector, bounds, layout, window, cx);
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
        Element::paint(
            &mut self.child,
            id,
            inspector,
            bounds,
            layout,
            paint,
            window,
            cx,
        );
    }
}
