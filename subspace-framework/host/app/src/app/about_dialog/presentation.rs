//! Product identity and attribution, separate from the retained update panel.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use subspace_dashboard::{Tokens, links, rich};
use subspace_product::{Channel, ProductContext, ValidatedProduct};

// A large product mark, on the shared spacing scale.
const LOGO_SIZE: f32 = 64.;

pub(super) fn identity(context: &ProductContext, text: &str, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    v_flex()
        .gap_2()
        .items_center()
        .pb_2()
        .child(logos(cx))
        .child(
            rich::plain_text("about-product-name", crate::product::desktop_name(cx))
                .text_size(tokens.heading_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong),
        )
        .child(
            rich::plain_text(
                "about-version",
                version_line(text, &context.product.display_name),
            )
            .font_weight(FontWeight::MEDIUM)
            .text_color(tokens.fg_strong),
        )
        .child(
            rich::plain_text(
                "about-channel",
                build_channel(context.build.kind.as_str(), Channel::current()),
            )
            .text_size(tokens.secondary_size())
            .text_color(tokens.muted),
        )
        .into_any_element()
}

fn logos(cx: &App) -> impl IntoElement {
    let size = px(LOGO_SIZE);
    h_flex()
        .gap_4()
        .justify_center()
        .child(crate::assets::product_logo(cx, size))
        .children(crate::assets::organization_logo(cx, size).map(|logo| {
            div()
                .id("about-organization-logo")
                .test_support()
                .child(logo)
        }))
}

/// The status line without the product name, which the dialog shows above it.
pub(super) fn version_line<'a>(text: &'a str, name: &str) -> &'a str {
    let line = text.lines().next().unwrap_or_default();
    line.strip_prefix(name).map_or(line, str::trim_start)
}

pub(super) fn build_channel(kind: &str, channel: Channel) -> &'static str {
    match (kind, channel) {
        (_, Channel::Nightly) => "Nightly",
        ("release", Channel::Stable) => "Release",
        ("release", Channel::Alpha) => "Alpha",
        ("release", Channel::Beta) => "Beta",
        _ => "Development",
    }
}

pub(super) fn details(context: &ProductContext, text: &str, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let repository = format!("https://github.com/{}", context.product.release_repository);
    v_flex()
        .gap_2()
        .pt_2()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(rich::plain_text("about-build-details", build_details(text)))
        .child(links::external(
            "about-repository",
            "Repository on GitHub",
            repository,
            &tokens,
        ))
        .into_any_element()
}

fn build_details(text: &str) -> &str {
    text.find("Git commit:")
        .map_or(text, |start| &text[start..])
}

pub(super) fn credits(product: &ValidatedProduct, cx: &App) -> Option<AnyElement> {
    if product.credits.is_empty() {
        return None;
    }
    Some(credit_section(&product.credits, &Tokens::current(cx)))
}

fn credit_section(lines: &[String], tokens: &Tokens) -> AnyElement {
    v_flex()
        .id("about-credits-section")
        .role(Role::Group)
        .aria_label("Product credits")
        .test_support()
        .gap_2()
        .mt_2()
        .pt_4()
        .border_t_1()
        .border_color(tokens.hair)
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(rich::plain_text("about-credits", &lines.join("\n")))
        .into_any_element()
}
