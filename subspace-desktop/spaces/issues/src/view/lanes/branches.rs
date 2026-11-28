//! Connector lines in the shared list row's indent gutter.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Div, Styled as _, div, relative};
use subspace_framework::dashboard::{Row, Tokens};

use crate::Issues;

const RAIL_OPACITY: f32 = 0.42;
const RAIL_OFFSET: f32 = 0.5;
const BRANCH_REACH: f32 = 0.32;

pub(super) fn guides(issues: &Issues, row: &Row, tokens: &Tokens) -> impl Iterator<Item = Div> {
    let rails = issues.branch_guides.get(&row.key).copied().unwrap_or(0);
    let depth = row.indent;
    let indent = tokens.indent;
    let midpoint = relative(RAIL_OFFSET);
    let colour = tokens.muted.opacity(RAIL_OPACITY);
    let branch = (depth > 0).then(|| {
        div()
            .absolute()
            .top(midpoint)
            .left(-indent * RAIL_OFFSET)
            .w(indent * BRANCH_REACH)
            .h_px()
            .bg(colour)
    });
    (1..=depth)
        .filter_map(move |level| {
            let continues = rails & (1 << (level - 1)) != 0;
            (continues || level == depth).then(|| {
                div()
                    .absolute()
                    .top_0()
                    .left(indent * (f32::from(level) - f32::from(depth) - RAIL_OFFSET))
                    .w_px()
                    .bg(colour)
                    .when(continues, |rail| rail.bottom_0())
                    .when(!continues, |rail| rail.h(midpoint))
            })
        })
        .chain(branch)
}
