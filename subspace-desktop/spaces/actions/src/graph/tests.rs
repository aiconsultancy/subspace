use super::layout::{
    MAX_NODES, NODE_HEIGHT, NODE_WIDTH, NodeRect, Point, bounds, layout, neighbor, route,
};
use super::viewport::{MAX_SCALE, MIN_SCALE, Viewport};

#[test]
fn longest_depth_and_response_order_are_stable() {
    let graph = layout(
        &[4, 3, 2, 1],
        &[(1, 2), (1, 3), (3, 4), (2, 4), (1, 4)],
        None,
    );
    assert_eq!(
        graph.nodes.iter().map(|n| n.layer).collect::<Vec<_>>(),
        [2, 1, 1, 0]
    );
    assert!(graph.nodes[1].rect.origin.y < graph.nodes[2].rect.origin.y);
    assert_eq!(
        graph,
        layout(
            &[4, 3, 2, 1],
            &[(1, 2), (1, 3), (3, 4), (2, 4), (1, 4)],
            None
        )
    );
}

#[test]
fn invalid_dependencies_degrade_without_inventing_edges() {
    for edges in [vec![(1, 2), (2, 1)], vec![(9, 1)]] {
        let graph = layout(&[1, 2], &edges, None);
        assert!(graph.degraded.is_some());
        assert!(graph.edges.is_empty());
        assert!(graph.nodes.iter().all(|n| n.layer == 0));
    }
    let unreadable = layout(&[1, 2], &[(1, 2)], Some("Unsupported YAML".into()));
    assert_eq!(unreadable.degraded.as_deref(), Some("Unsupported YAML"));
    assert!(unreadable.edges.is_empty());
    assert!(layout(&[1, 1], &[], None).degraded.is_some());
}

#[test]
fn large_graph_is_bounded_and_empty_graph_has_empty_bounds() {
    let ids: Vec<_> = (0..=MAX_NODES as u64).collect();
    let graph = layout(&ids, &[], None);
    assert!(graph.nodes.is_empty());
    assert!(graph.degraded.is_some());
    assert_eq!(
        bounds(layout(&[], &[], None).nodes.into_iter().map(|n| n.rect)),
        NodeRect::default()
    );
}

#[test]
fn curve_attaches_to_card_centers() {
    let graph = layout(&[1, 2], &[(1, 2)], None);
    let points = route(graph.nodes[0].rect, graph.nodes[1].rect);
    assert_eq!(
        points[0],
        Point {
            x: NODE_WIDTH,
            y: NODE_HEIGHT / 2.
        }
    );
    assert_eq!(points[3].x, graph.nodes[1].rect.origin.x);
    assert_eq!(points[1].x, points[2].x);
}

#[test]
fn zoom_preserves_cursor_world_position_and_clamps() {
    let mut viewport = Viewport::default();
    let cursor = Point { x: 420., y: 250. };
    let before = viewport.world(cursor);
    viewport.zoom(2., cursor);
    assert_eq!(viewport.world(cursor), before);
    assert_eq!(viewport.screen(before), cursor);
    viewport.zoom(1000., cursor);
    assert_eq!(viewport.scale, MAX_SCALE);
    viewport.zoom(0.00001, cursor);
    assert_eq!(viewport.scale, MIN_SCALE);
    let valid = viewport.clone();
    viewport.zoom(f32::NAN, cursor);
    assert_eq!(viewport, valid);
}

#[test]
fn dragging_one_node_keeps_layout_and_other_nodes_fixed() {
    let graph = layout(&[1, 2], &[(1, 2)], None);
    let mut viewport = Viewport {
        scale: 2.,
        ..Viewport::default()
    };
    viewport.drag(Some(1), Point { x: -100., y: 60. });
    assert_eq!(
        viewport.rect(&graph.nodes[0]).origin,
        Point { x: -50., y: 30. }
    );
    assert_eq!(viewport.rect(&graph.nodes[1]), graph.nodes[1].rect);
    assert_eq!(graph.nodes[0].rect.origin, Point::default());
    viewport.drag(None, Point { x: 20., y: -10. });
    assert_eq!(viewport.pan, Point { x: 44., y: 14. });
}

#[test]
fn fit_contains_all_nodes_including_negative_drag_offsets() {
    let graph = layout(&[1, 2, 3, 4], &[(1, 2), (1, 3), (3, 4)], None);
    let mut viewport = Viewport::default();
    viewport.drag(Some(1), Point { x: -300., y: -200. });
    viewport.fit(viewport.extent(&graph), Point { x: 800., y: 500. });
    for node in &graph.nodes {
        let rect = viewport.rect(node);
        let start = viewport.screen(rect.origin);
        let end = viewport.screen(Point {
            x: rect.origin.x + rect.width,
            y: rect.origin.y + rect.height,
        });
        assert!(start.x >= 0. && start.y >= 0. && end.x <= 800. && end.y <= 500.);
    }
}

#[test]
fn keyboard_moves_between_layers_and_matrix_siblings() {
    let graph = layout(&[1, 2, 3, 4], &[(1, 2), (1, 3), (3, 4)], None);
    assert_eq!(neighbor(&graph.nodes, None, "right"), Some(1));
    assert_eq!(neighbor(&graph.nodes, Some(1), "right"), Some(2));
    assert_eq!(neighbor(&graph.nodes, Some(2), "down"), Some(3));
    assert_eq!(neighbor(&graph.nodes, Some(3), "right"), Some(4));
    assert_eq!(neighbor(&graph.nodes, Some(3), "up"), Some(2));
    let reversed = layout(&[4, 3, 2, 1], &[(1, 2), (1, 3), (3, 4)], None);
    assert_eq!(neighbor(&reversed.nodes, Some(4), "left"), Some(3));
    assert_eq!(neighbor(&reversed.nodes, Some(1), "right"), Some(3));
}
