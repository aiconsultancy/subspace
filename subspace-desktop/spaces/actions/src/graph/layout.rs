//! Deterministic job placement; no window or service state.
use std::collections::{BTreeMap, BTreeSet};

pub const NODE_WIDTH: f32 = 220.;
pub const NODE_HEIGHT: f32 = 104.;
const LAYER_GAP: f32 = 72.;
const ROW_GAP: f32 = 32.;
pub const MAX_NODES: usize = 128;
pub const MIDPOINT_DIVISOR: f32 = 2.;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NodeRect {
    pub origin: Point,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: u64,
    pub layer: usize,
    pub rect: NodeRect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub nodes: Vec<Node>,
    pub edges: Vec<(u64, u64)>,
    pub degraded: Option<String>,
}

/// Invalid dependencies deliberately produce an edge-free start-order view.
pub fn layout(ids: &[u64], edges: &[(u64, u64)], degraded: Option<String>) -> Layout {
    if ids.len() > MAX_NODES {
        return Layout {
            degraded: Some("Graph too large; use the Runs text view".into()),
            ..Layout::default()
        };
    }
    let depths = depths(ids, edges);
    let reason = degraded.or_else(|| depths.as_ref().err().cloned());
    let layers = match reason.is_some() {
        true => vec![0; ids.len()],
        false => depths.unwrap_or_default(),
    };
    Layout {
        nodes: place(ids, &layers),
        edges: if reason.is_some() {
            Vec::new()
        } else {
            edges.to_vec()
        },
        degraded: reason,
    }
}

fn depths(ids: &[u64], edges: &[(u64, u64)]) -> Result<Vec<usize>, String> {
    let known: BTreeSet<_> = ids.iter().copied().collect();
    if known.len() != ids.len() {
        return Err("Duplicate job identifiers".into());
    }
    if edges
        .iter()
        .any(|(a, b)| !known.contains(a) || !known.contains(b))
    {
        return Err("A dependency refers to a missing job".into());
    }
    longest_depths(ids, edges)
}

fn longest_depths(ids: &[u64], edges: &[(u64, u64)]) -> Result<Vec<usize>, String> {
    let mut resolved = BTreeMap::<u64, usize>::new();
    while resolved.len() < ids.len() {
        let before = resolved.len();
        for id in ids {
            if resolved.contains_key(id) {
                continue;
            }
            let parents: Vec<_> = edges.iter().filter(|(_, to)| to == id).collect();
            if parents.iter().all(|(from, _)| resolved.contains_key(from)) {
                let depth = parents
                    .iter()
                    .filter_map(|(from, _)| resolved.get(from))
                    .max();
                resolved.insert(*id, depth.map_or(0, |depth| depth + 1));
            }
        }
        if resolved.len() == before {
            return Err("Dependencies contain a cycle".into());
        }
    }
    Ok(ids.iter().map(|id| resolved[id]).collect())
}

fn place(ids: &[u64], layers: &[usize]) -> Vec<Node> {
    let mut rows = BTreeMap::<usize, usize>::new();
    ids.iter()
        .zip(layers)
        .map(|(id, layer)| {
            let row = rows.entry(*layer).or_default();
            let node = Node {
                id: *id,
                layer: *layer,
                rect: NodeRect {
                    origin: Point {
                        x: *layer as f32 * (NODE_WIDTH + LAYER_GAP),
                        y: *row as f32 * (NODE_HEIGHT + ROW_GAP),
                    },
                    width: NODE_WIDTH,
                    height: NODE_HEIGHT,
                },
            };
            *row += 1;
            node
        })
        .collect()
}

/// Cubic Bézier points in source, control, control, destination order.
pub fn route(source: NodeRect, target: NodeRect) -> [Point; 4] {
    let start = Point {
        x: source.origin.x + source.width,
        y: source.origin.y + source.height / MIDPOINT_DIVISOR,
    };
    let end = Point {
        x: target.origin.x,
        y: target.origin.y + target.height / MIDPOINT_DIVISOR,
    };
    let middle = (start.x + end.x) / MIDPOINT_DIVISOR;
    [
        start,
        Point {
            x: middle,
            y: start.y,
        },
        Point {
            x: middle,
            y: end.y,
        },
        end,
    ]
}

pub fn bounds(rects: impl Iterator<Item = NodeRect>) -> NodeRect {
    let mut extent: Option<(f32, f32, f32, f32)> = None;
    for rect in rects {
        let right = rect.origin.x + rect.width;
        let bottom = rect.origin.y + rect.height;
        extent = Some(match extent {
            None => (rect.origin.x, rect.origin.y, right, bottom),
            Some((x, y, r, b)) => (
                x.min(rect.origin.x),
                y.min(rect.origin.y),
                r.max(right),
                b.max(bottom),
            ),
        });
    }
    extent.map_or(NodeRect::default(), |(x, y, r, b)| NodeRect {
        origin: Point { x, y },
        width: r - x,
        height: b - y,
    })
}

/// Horizontal arrows traverse layers; vertical arrows traverse response order.
pub fn neighbor(nodes: &[Node], selected: Option<u64>, key: &str) -> Option<u64> {
    let Some(current) = nodes.iter().find(|node| Some(node.id) == selected) else {
        return nodes.first().map(|node| node.id);
    };
    match key {
        "left" => nodes
            .iter()
            .find(|n| n.layer.checked_add(1) == Some(current.layer)),
        "right" => nodes
            .iter()
            .find(|n| Some(n.layer) == current.layer.checked_add(1)),
        "up" => nodes
            .iter()
            .take_while(|n| n.id != current.id)
            .filter(|n| n.layer == current.layer)
            .last(),
        "down" => nodes
            .iter()
            .skip_while(|n| n.id != current.id)
            .skip(1)
            .find(|n| n.layer == current.layer),
        _ => None,
    }
    .map(|node| node.id)
    .or(selected)
}
