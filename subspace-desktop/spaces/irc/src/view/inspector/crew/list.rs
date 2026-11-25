//! Crew rows in the Ship tab.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{Tokens, avatar, truncated};

use crate::{
    Irc,
    live::{CrewLine, ShipLive},
};

use super::super::ship::work;
use super::CrewFocus;

/// One crew member: their dot, name, work and how long.
pub(in super::super) fn row(
    focus: &CrewFocus,
    member: &CrewLine,
    route: (&ShipLive, Option<&str>),
    since: String,
    cx: &mut Context<Irc>,
) -> AnyElement {
    let (ship, fleet) = route;
    let tokens = Tokens::current(cx);
    let id = SharedString::from(format!("irc-inspector-crew:{}", member.id));
    let row = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .text_size(tokens.secondary_size())
        .child(avatar::activity_dot(member.working, &tokens))
        .child(
            div()
                .flex_shrink_0()
                .text_color(if member.working {
                    tokens.fg_strong
                } else {
                    tokens.muted
                })
                .child(member.name.clone()),
        )
        .child(
            truncated::text(
                format!("irc-inspector-crew-work:{}", member.id),
                work(member),
            )
            .flex_1()
            .text_color(tokens.muted),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(since),
        );
    let Some(fleet) = fleet else {
        return row.id(id).test_support().into_any_element();
    };
    let target = (fleet.to_owned(), ship.id.clone(), member.id.clone());
    Button::new(id.clone())
        .ghost()
        .w_full()
        .min_w_0()
        .accessibility_id(id)
        .accessibility_label(format!("Open crew member {}", member.name))
        .when(focus.returned.as_deref() == Some(&member.id), |button| {
            button.track_focus(&focus.row)
        })
        .child(row)
        .on_click(cx.listener(move |board, _, window, cx| {
            super::open(board, &target, window, cx);
        }))
        .into_any_element()
}
