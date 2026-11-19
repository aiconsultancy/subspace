//! Live samples and contrast checks for the draft's foreground/background pairs.

use gpui_kit::component::{Theme, h_flex, v_flex};
use gpui_kit::{App, Div, ParentElement, Styled, div};
use subspace_dashboard::{Tokens, typography};

use crate::theme::ThemeDocument;
use subspace_dashboard::contrast;

const TEXT_CONTRAST: f64 = 4.5;
const FOCUS_CONTRAST: f64 = 3.;
const HEADING_SAMPLE_SIZE: f32 = 18.;
const CODE_SAMPLE_SIZE: f32 = 14.;

pub(super) fn samples(cx: &App) -> Div {
    let theme = Theme::global(cx);
    let tokens = Tokens::current(cx);
    v_flex()
        .gap_2()
        .child("Preview samples")
        .children(font_samples(cx))
        .child(
            div()
                .p_2()
                .bg(theme.accent)
                .text_color(theme.accent_foreground)
                .border_1()
                .border_color(theme.list_active_border)
                .child("Selected control"),
        )
        .child(
            div()
                .p_2()
                .bg(theme.primary)
                .text_color(theme.primary_foreground)
                .child("Primary action"),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap_3()
                .child(div().text_color(tokens.green).child("Success"))
                .child(div().text_color(tokens.yellow).child("Warning"))
                .child(div().text_color(tokens.red).child("Error")),
        )
}

fn font_samples(cx: &App) -> [Div; 5] {
    let theme = Theme::global(cx);
    let tokens = Tokens::current(cx);
    let heading = typography::heading_font(cx);
    [
        div()
            .font_family(heading.clone())
            .text_size(tokens.heading_size(HEADING_SAMPLE_SIZE))
            .child("Heading / active tab"),
        div()
            .font_family(theme.font_family.clone())
            .child("Body text: The quick brown fox, 0123456789."),
        div()
            .font_family(theme.mono_font_family.clone())
            .text_size(tokens.mono_size(CODE_SAMPLE_SIZE))
            .child("let theme = appearance();"),
        typography::group_heading(div().child("▾ Group heading"), cx),
        div().text_sm().child(format!(
            "Resolved fonts: heading {heading}, body {}, code {}",
            theme.font_family, theme.mono_font_family
        )),
    ]
}

pub(super) fn contrast_warnings(document: Result<ThemeDocument, String>, cx: &App) -> Div {
    let messages = document
        .map(|document| contrast_messages(&document, cx))
        .unwrap_or_else(|error| vec![error]);
    v_flex().gap_1().child("Contrast checks").children(
        messages
            .into_iter()
            .map(|message| div().text_sm().child(message)),
    )
}

/// Foreground and background colour keys checked against a minimum ratio,
/// each composited over the page.
const PAIRS: [(&str, &str, &str, f64); 9] = [
    ("Body / page", "foreground", "background", TEXT_CONTRAST),
    (
        "Muted / panel",
        "muted.foreground",
        "muted.background",
        TEXT_CONTRAST,
    ),
    (
        "Primary text",
        "primary.foreground",
        "primary.background",
        TEXT_CONTRAST,
    ),
    (
        "Selected text",
        "accent.foreground",
        "accent.background",
        TEXT_CONTRAST,
    ),
    (
        "Body / title bar",
        "foreground",
        "title_bar.background",
        TEXT_CONTRAST,
    ),
    (
        "Body / sidebar",
        "foreground",
        "sidebar.background",
        TEXT_CONTRAST,
    ),
    (
        "Body / dialogs and menus",
        "foreground",
        "popover.background",
        TEXT_CONTRAST,
    ),
    ("Focus / page", "ring", "background", FOCUS_CONTRAST),
    ("Focus / panel", "ring", "muted.background", FOCUS_CONTRAST),
];

fn contrast_messages(document: &ThemeDocument, cx: &App) -> Vec<String> {
    let color = |key: &str| {
        document
            .color(key)
            .unwrap_or_else(|| super::super::colors::current(key, cx))
    };
    let page = color("background");
    PAIRS
        .iter()
        .map(|(label, foreground, background, minimum)| {
            let measured = contrast::ratio_on(&color(foreground), &color(background), &page);
            contrast_message(label, measured, *minimum)
        })
        .collect()
}

fn contrast_message(label: &str, measured: Result<f64, String>, minimum: f64) -> String {
    match measured {
        Ok(value) if value < minimum => {
            format!("Warning: {label} {value:.2}:1; needs {minimum}:1.")
        }
        Ok(value) => format!("{label}: {value:.2}:1"),
        Err(error) => format!("{label}: {error}"),
    }
}
