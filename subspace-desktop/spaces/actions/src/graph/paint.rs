//! The edge canvas uses the same GPUI path primitive as native plot shapes.
use super::{
    FlowGraph,
    layout::{self, Point},
    viewport::Viewport,
};
use gpui_kit::{
    Bounds, Context, IntoElement, PathBuilder, Pixels, Styled, Window, canvas, point, px,
};
use subspace_framework::dashboard::Tokens;

const EDGE_WIDTH: f32 = 1.5;
const ARROW_LENGTH: f32 = 6.;
const ARROW_HALF_HEIGHT: f32 = 3.;

pub(super) fn edges(
    graph: &FlowGraph,
    tokens: Tokens,
    cx: &Context<FlowGraph>,
) -> impl IntoElement {
    let entity = cx.entity().downgrade();
    let routes = routes(graph);
    let viewport = graph.viewport.clone();
    canvas(
        move |bounds, _, cx| {
            let entity = entity.clone();
            cx.defer(move |cx| {
                let _ = entity.update(cx, |graph, cx| graph.measured(bounds, cx));
            });
        },
        move |bounds, (), window, _| paint_routes(&routes, &viewport, (bounds, tokens), window),
    )
    .absolute()
    .size_full()
}

fn routes(graph: &FlowGraph) -> Vec<[Point; 4]> {
    graph
        .layout
        .edges
        .iter()
        .filter_map(|(from, to)| {
            let source = graph.layout.nodes.iter().find(|node| node.id == *from)?;
            let target = graph.layout.nodes.iter().find(|node| node.id == *to)?;
            Some(layout::route(
                graph.viewport.rect(source),
                graph.viewport.rect(target),
            ))
        })
        .collect()
}

fn paint_routes(
    routes: &[[Point; 4]],
    viewport: &Viewport,
    frame: (Bounds<Pixels>, Tokens),
    window: &mut Window,
) {
    for route in routes {
        let points = route.map(|world| {
            let screen = viewport.screen(world);
            frame.0.origin + point(px(screen.x), px(screen.y))
        });
        let mut path = PathBuilder::stroke(px(EDGE_WIDTH));
        path.move_to(points[0]);
        path.cubic_bezier_to(points[3], points[1], points[2]);
        if let Ok(path) = path.build() {
            window.paint_path(path, frame.1.muted);
        }
        arrow(points[3], frame.1.muted, window);
    }
}

fn arrow(end: gpui_kit::Point<Pixels>, color: gpui_kit::Hsla, window: &mut Window) {
    let mut path = PathBuilder::fill();
    path.move_to(end);
    path.line_to(end + point(px(-ARROW_LENGTH), px(-ARROW_HALF_HEIGHT)));
    path.line_to(end + point(px(-ARROW_LENGTH), px(ARROW_HALF_HEIGHT)));
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

impl FlowGraph {
    fn measured(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        self.origin = Point {
            x: bounds.origin.x.into(),
            y: bounds.origin.y.into(),
        };
        self.size = Point {
            x: bounds.size.width.into(),
            y: bounds.size.height.into(),
        };
        if self.fit_pending
            && !self.layout.nodes.is_empty()
            && self.size.x > super::viewport::EMPTY_DIMENSION
            && self.size.y > super::viewport::EMPTY_DIMENSION
        {
            self.fit_pending = false;
            self.fit(cx);
        }
    }
}
