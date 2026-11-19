//! One gallery card: the copyable name, the used mark, the source, the note,
//! the preferred treatment and the samples.

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, TestSupportExt as _, WeakEntity, div,
};
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::{Tokens, rich};
use subspace_lucide::IconName;

use crate::ComponentGallery;
use crate::catalog::Item;
use crate::samples::Sample;

use super::named;

/// One item: its copyable name, source, use note and a wrapping row of samples.
pub(super) fn card(
    item: &'static Item,
    samples: Vec<Sample>,
    tokens: &Tokens,
    gallery: WeakEntity<ComponentGallery>,
    cx: &App,
) -> AnyElement {
    v_flex()
        .id(named("gallery-item", item.name))
        .test_support()
        .gap_1()
        .p(tokens.pad_x)
        .rounded(tokens.surface_radius)
        .border_1()
        .border_color(tokens.line)
        .bg(tokens.pane)
        .child(title_row(item, tokens, cx))
        .child(
            div()
                .text_size(tokens.caption_size())
                .text_color(tokens.muted)
                .child(rich::plain_text(
                    named("gallery-source", item.name),
                    item.source,
                )),
        )
        .child(
            div()
                .text_size(tokens.secondary_size())
                .child(rich::plain_text(
                    named("gallery-used-in", item.name),
                    item.used_in,
                )),
        )
        .children(prefer_row(item, tokens, gallery))
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .gap_3()
                .pt_1()
                .children(samples.into_iter().map(|sample| frame(sample, tokens))),
        )
        .into_any_element()
}

/// The name in the monospace family, GPUI Kit's Clipboard to copy it, and
/// for a GPUI Kit component whether Subspace uses it.
fn title_row(item: &'static Item, tokens: &Tokens, cx: &App) -> Div {
    let usage = item.kit.as_ref().map(|kit| {
        if kit.used {
            Tag::secondary().small().child("Used in Subspace")
        } else {
            Tag::new().outline().small().child("Not used in Subspace")
        }
    });
    h_flex()
        .gap_1()
        .flex_wrap()
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(tokens.mono_font)
                .text_color(tokens.fg_strong)
                .child(rich::plain_text(
                    named("gallery-name", item.name),
                    item.name,
                )),
        )
        .child(
            Clipboard::new(named("gallery-copy", item.name))
                .value(item.name)
                .tooltip("Copy name")
                .accessibility_label(format!("Copy {}", item.name)),
        )
        .children(usage)
}

/// For a GPUI Kit component with a shared treatment: a button to its card.
fn prefer_row(
    item: &'static Item,
    tokens: &Tokens,
    gallery: WeakEntity<ComponentGallery>,
) -> Option<Div> {
    let prefer = item.kit.as_ref()?.prefer?;
    let button = reveal_button(named("gallery-prefer", item.name), prefer, gallery);
    Some(
        h_flex()
            .gap_1()
            .text_size(tokens.secondary_size())
            .text_color(tokens.muted)
            .child("In Subspace, use")
            .child(button),
    )
}

/// A quiet button that shows the card called `name`, for in-gallery links.
pub(crate) fn reveal_button(
    id: ElementId,
    name: &'static str,
    gallery: WeakEntity<ComponentGallery>,
) -> IconButton {
    action::action_button(id, Intent::Quiet, Some(IconName::ArrowRight), name)
        .tooltip(format!("Show {name}"))
        .on_click(move |_, window, cx| {
            _ = gallery.update(cx, |gallery, cx| gallery.reveal(name, window, cx));
        })
}

/// A sample above its state caption.
fn frame(sample: Sample, tokens: &Tokens) -> Div {
    v_flex().items_start().gap_1().child(sample.element).child(
        div()
            .text_size(tokens.caption_size())
            .text_color(tokens.muted)
            .child(sample.state),
    )
}
