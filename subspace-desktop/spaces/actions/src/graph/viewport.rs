//! Screen/world mapping and independent user offsets.
use super::layout::{Layout, MIDPOINT_DIVISOR, NodeRect, Point, bounds};
use std::collections::BTreeMap;

pub const MIN_SCALE: f32 = 0.01;
pub const MAX_SCALE: f32 = 2.5;
const FIT_PADDING: f32 = 24.;
pub const IDENTITY_SCALE: f32 = 1.;
pub const EMPTY_DIMENSION: f32 = 0.;
const MIN_FIT_DIMENSION: f32 = 1.;

#[derive(Clone, Debug, PartialEq)]
pub struct Viewport {
    pub scale: f32,
    pub pan: Point,
    pub offsets: BTreeMap<u64, Point>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scale: 1.,
            pan: Point {
                x: FIT_PADDING,
                y: FIT_PADDING,
            },
            offsets: BTreeMap::new(),
        }
    }
}

impl Viewport {
    pub fn screen(&self, world: Point) -> Point {
        Point {
            x: world.x * self.scale + self.pan.x,
            y: world.y * self.scale + self.pan.y,
        }
    }

    pub fn world(&self, screen: Point) -> Point {
        Point {
            x: (screen.x - self.pan.x) / self.scale,
            y: (screen.y - self.pan.y) / self.scale,
        }
    }

    pub fn zoom(&mut self, factor: f32, anchor: Point) {
        if !factor.is_finite() || factor <= EMPTY_DIMENSION {
            return;
        }
        let world = self.world(anchor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        self.pan = Point {
            x: anchor.x - world.x * self.scale,
            y: anchor.y - world.y * self.scale,
        };
    }

    pub fn drag(&mut self, id: Option<u64>, delta: Point) {
        match id {
            Some(id) => {
                let offset = self.offsets.entry(id).or_default();
                offset.x += delta.x / self.scale;
                offset.y += delta.y / self.scale;
            }
            None => {
                self.pan.x += delta.x;
                self.pan.y += delta.y;
            }
        }
    }

    pub fn rect(&self, node: &super::layout::Node) -> NodeRect {
        let offset = self.offsets.get(&node.id).copied().unwrap_or_default();
        NodeRect {
            origin: Point {
                x: node.rect.origin.x + offset.x,
                y: node.rect.origin.y + offset.y,
            },
            ..node.rect
        }
    }

    pub fn extent(&self, layout: &Layout) -> NodeRect {
        bounds(layout.nodes.iter().map(|node| self.rect(node)))
    }

    pub fn fit(&mut self, extent: NodeRect, size: Point) {
        if extent.width <= EMPTY_DIMENSION || extent.height <= EMPTY_DIMENSION {
            return;
        }
        let width = (size.x - FIT_PADDING * MIDPOINT_DIVISOR).max(MIN_FIT_DIMENSION);
        let height = (size.y - FIT_PADDING * MIDPOINT_DIVISOR).max(MIN_FIT_DIMENSION);
        self.scale = (width / extent.width)
            .min(height / extent.height)
            .clamp(MIN_SCALE, MAX_SCALE);
        self.pan = Point {
            x: (size.x - extent.width * self.scale) / MIDPOINT_DIVISOR
                - extent.origin.x * self.scale,
            y: (size.y - extent.height * self.scale) / MIDPOINT_DIVISOR
                - extent.origin.y * self.scale,
        };
    }
}
