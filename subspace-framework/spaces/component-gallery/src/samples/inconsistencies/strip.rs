//! Plain replica boxes of measured heights, radii and padding, side by side.
//!
//! The boxes are replicas drawn from the table below, not the board code.
//! Heights without a fixed value in the source are the 16 px text line plus
//! any vertical padding.

use gpui_kit::{Context, ParentElement as _, Styled as _, Window, div, px};
use subspace_dashboard::Tokens;

use crate::ComponentGallery;
use crate::samples::{Sample, sample};

/// One measured control in px: height, corner radius and horizontal padding.
struct Replica {
    name: &'static str,
    height: f32,
    radius: f32,
    pad: f32,
}

const REPLICAS: [Replica; 2] = [
    // framework/board-kit/dashboard/src/badge.rs: PADDING_X_PX 5, RADIUS_PX 7.
    Replica {
        name: "badge::badge",
        height: 16.,
        radius: 7.,
        pad: 5.,
    },
    // spaces/github/src/detail.rs mode_switch: px_2, TAB_PAD_Y_PX 2, TAB_RADIUS_PX 4.
    Replica {
        name: "github::mode_switch",
        height: 20.,
        radius: 4.,
        pad: 8.,
    },
];

pub(in crate::samples) fn size_and_radius(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    REPLICAS
        .iter()
        .map(|replica| replica_box(replica, tokens))
        .collect()
}

/// The one operation that draws every replica from its measured values.
fn replica_box(replica: &Replica, tokens: &Tokens) -> Sample {
    let caption = format!(
        "replica: {} {}/{} px",
        replica.name, replica.height, replica.radius
    );
    let shape = div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(replica.height))
        .px(px(replica.pad))
        .rounded(px(replica.radius))
        .border_1()
        .border_color(tokens.line)
        .bg(tokens.select)
        .text_color(tokens.fg)
        .text_size(tokens.caption_size())
        .child("Aa");
    sample(caption, shape)
}
