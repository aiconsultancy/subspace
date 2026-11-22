//! Literal references retain their existing native presentation inside the
//! selectable text view. Kit includes each inline object's text in selection.

use gpui_kit::TestSupportExt as _;
use gpui_kit::base::TestTextExt as _;
use gpui_kit::base::TextSelection;
use gpui_kit::component::text::{
    InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
    markdown_ast,
};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ClickEvent, FontWeight, HighlightStyle, Hsla, InteractiveElement as _, InteractiveText,
    MouseButton, MouseDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledText, UnderlineStyle, Window, div, px,
};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, rich};

use super::{Opens, RefHandlers, look, targets};
use crate::markup::{self, Ref};
use crate::style::RunStyle;

const REFERENCE_TINT: f32 = 0.16;
const FOCUS_TINT: f32 = 0.4;
const DECORATION_PX: f32 = 1.;
/// Room either side of a reference's tint, so it doesn't stop at the glyphs.
const REFERENCE_PAD_X_PX: f32 = 3.;
const REFERENCE_RADIUS_PX: f32 = 3.;

pub(super) struct References {
    handlers: RefHandlers,
    context: Option<rich::RichTextContext>,
}

impl References {
    pub fn new(handlers: RefHandlers) -> Self {
        Self {
            handlers,
            context: None,
        }
    }

    pub fn context(mut self, context: rich::RichTextContext) -> Self {
        self.context = Some(context);
        self
    }

    fn style(&self, found: &Opens, occurrence: usize) -> RunStyle {
        let Some(styled) = &self.handlers.styled else {
            return RunStyle::default();
        };
        targets(styled, true)
            .into_iter()
            .filter(|(_, target)| same_target(target, found))
            .nth(occurrence)
            .and_then(|(range, _)| {
                styled
                    .runs
                    .iter()
                    .find(|(run, _)| run.start <= range.start && range.start < run.end)
            })
            .map(|(_, style)| *style)
            .unwrap_or_default()
    }
}

impl MarkdownPlugin for References {
    fn name(&self) -> &str {
        "irc-reference"
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        context: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Link(link) = node else {
            return None;
        };
        let found = markup::parse_ref_url(&link.url)
            .map(Opens::Ref)
            .unwrap_or_else(|| Opens::Url(link.url.clone()));
        if self.handlers.styled.is_none() && !chip(&found) {
            return None;
        }
        // Literal links have one escaped text child. Leave formatted labels
        // to Kit rather than flattening their markup.
        let [markdown_ast::Node::Text(label)] = link.children.as_slice() else {
            return None;
        };
        let start = node.position()?.start.offset;
        let prefix = context.source().get(..start)?;
        let occurrence = link_occurrence(prefix, &link.url);
        let shown = shown_label(&found, context.source());
        Some(
            MarkdownNode::new(self.name(), (found, occurrence, link.url.clone(), shown))
                .text(label.value.clone()),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        _: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let (found, occurrence, url, shown) = node
            .data::<(Opens, usize, String, Option<String>)>()?
            .clone();
        let tokens = Tokens::current(cx);
        let run = self.style(&found, occurrence);
        let look = match &found {
            Opens::Ref(found) => (self.handlers.look)(found, cx),
            _ => None,
        };
        // Copy keeps the text as written; a pull request shows `repo#n`.
        let text = node.as_text().to_owned();
        let (label, styled) = self.label(node, shown, reference_style(run, &tokens), cx);
        let id: SharedString = format!("irc-reference:{:?}", node.source_range()).into();
        let state_id: SharedString =
            format!("irc-reference-state:{:?}", node.source_range()).into();
        let trigger = div()
            .id(id)
            .test_support()
            .cursor_pointer()
            .px(px(REFERENCE_PAD_X_PX))
            .rounded(px(REFERENCE_RADIUS_PX))
            .when_some(reference_fill(run, &tokens), |trigger, fill| {
                trigger.bg(fill)
            })
            .when_some(pull_request_glyph(&found), |trigger, glyph| {
                let tint = look::tint(look).map(|tone| tokens.tone(tone));
                let glyph = div()
                    .flex()
                    .when_some(tint, |glyph, tint| glyph.text_color(tint))
                    .child(glyph);
                trigger.flex().items_center().gap_0p5().child(glyph)
            })
            .when_some(
                look.and_then(|look| look::dot(look, &tokens)),
                |trigger, dot| {
                    let dot = div().id(state_id).test_support().flex().child(dot);
                    trigger.flex().items_center().gap_1().child(dot)
                },
            )
            .child(div().child(self.hover(&found, styled)).test_text(label))
            .on_mouse_down(MouseButton::Right, self.mark_link(copied_link(&found, url)))
            .on_click(self.activation(found.clone()));
        Some(self.inline(trigger, text, chip(&found)))
    }
}

/// Issue and pull request references are chips with the shared text menu,
/// in literal and formatted rows alike.
fn chip(found: &Opens) -> bool {
    matches!(found, Opens::Ref(Ref::Issue(_) | Ref::PullRequest(_)))
}

/// What a pull request chip shows: `repo#n`, with the owner only when the
/// message also names another owner's repository of that name.
fn shown_label(found: &Opens, source: &str) -> Option<String> {
    match found {
        Opens::Ref(Ref::PullRequest(found)) => {
            Some(found.label_among(&markup::linked_pull_requests(source)))
        }
        _ => None,
    }
}

/// What Copy link copies: a pull request's URL as written, never the
/// internal reference link.
fn copied_link(found: &Opens, url: String) -> String {
    match found {
        Opens::Ref(Ref::PullRequest(found)) => found.url.clone(),
        _ => url,
    }
}

/// A pull request chip leads with the pull request glyph.
fn pull_request_glyph(found: &Opens) -> Option<Icon> {
    matches!(found, Opens::Ref(Ref::PullRequest(_)))
        .then(|| Icon::from(IconName::GitPullRequest).xsmall())
}

impl References {
    /// The text a reference shows, with its reference styling: a pull
    /// request's `shown` label, or the text as written with any find matches.
    fn label(
        &self,
        node: &MarkdownNode,
        shown: Option<String>,
        style: HighlightStyle,
        cx: &App,
    ) -> (String, StyledText) {
        let (label, matches) = match shown {
            Some(shown) => (shown, Vec::new()),
            None => (node.as_text().to_owned(), self.highlights(node, cx)),
        };
        let runs = reference_highlights(style, label.len(), matches);
        let styled = StyledText::new(label.clone()).with_highlights(runs);
        (label, styled)
    }

    /// The reference's text, with its tooltip.
    fn hover(&self, found: &Opens, styled: StyledText) -> InteractiveText {
        let tooltip = self.handlers.tooltip.clone();
        let hovered = found.clone();
        InteractiveText::new("tooltip", styled).tooltip(move |_, window, cx| match &hovered {
            Opens::Ref(found) => tooltip(found, window, cx),
            Opens::Url(_) | Opens::Upload(_) => None,
        })
    }

    /// A right press marks the link Copy link copies.
    // qual:allow(test_quality, untested) reason: "The pull request interaction tests right-click a chip and verify Copy link writes both the original URL and a marker's canonical URL. GPUI dispatches the handler indirectly."
    fn mark_link(&self, url: String) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let context = self.context.clone();
        move |_, _, _| {
            if let Some(context) = &context {
                context.mark_link(url.clone());
            }
        }
    }

    /// A left click with nothing selected opens the reference or link.
    fn activation(&self, found: Opens) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let claim = self.handlers.open.clone();
        move |event, window, cx| {
            if matches!(event, ClickEvent::Mouse(click) if click.up.button != MouseButton::Left) {
                return;
            }
            if !TextSelection::selected_text(window, cx).is_empty() {
                return;
            }
            match &found {
                Opens::Ref(found) => claim(found, window, cx),
                Opens::Url(url) => _ = subspace_framework::dashboard::links::open(url, cx),
                // An upload opens from its thumbnail under the text.
                Opens::Upload(_) => {}
            }
        }
    }

    /// Find-in-conversation matches inside the node's source.
    fn highlights(
        &self,
        node: &MarkdownNode,
        cx: &App,
    ) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
        self.context
            .as_ref()
            .zip(node.source_range())
            .map_or_else(Vec::new, |(context, range)| {
                context.source_highlights(range, cx)
            })
    }

    /// A reference chip offers the shared text menu; a link stays a plain trigger.
    fn inline<E>(&self, trigger: E, text: String, issue: bool) -> InlineElement
    where
        E: gpui_kit::InteractiveElement
            + gpui_kit::ParentElement
            + gpui_kit::Styled
            + gpui_kit::IntoElement
            + 'static,
    {
        match self.context.as_ref().filter(|_| issue) {
            Some(context) => InlineElement::new(context.text_menu(
                trigger,
                text,
                subspace_framework::dashboard::row_menu::MenuChat::Offered,
            )),
            None => InlineElement::new(trigger),
        }
    }
}

/// Kit reads canonical destinations; the styled ranges retain the typed URL.
fn same_target(original: &Opens, shown: &Opens) -> bool {
    match (original, shown) {
        (Opens::Url(original), Opens::Url(shown)) => {
            subspace_framework::dashboard::links::safe_url(original).as_deref()
                == Some(shown.as_str())
        }
        _ => original == shown,
    }
}

/// Count destinations as the literal writer spells them, including escapes.
fn link_occurrence(prefix: &str, url: &str) -> usize {
    let written = markup::destination(url).unwrap_or_else(|| url.to_owned());
    prefix.matches(&format!("]({written})")).count()
}

/// The tint behind a reference or a focused link, drawn on its own padded,
/// rounded box rather than as a text highlight, which would stop at the glyphs.
fn reference_fill(style: RunStyle, tokens: &Tokens) -> Option<Hsla> {
    ((style.reference && !style.unavailable) || style.focused).then(|| {
        tokens.accent.opacity(if style.focused {
            FOCUS_TINT
        } else {
            REFERENCE_TINT
        })
    })
}

/// Non-overlapping native runs keep reference styling outside and inside matches.
fn reference_highlights(
    style: HighlightStyle,
    len: usize,
    matches: Vec<(std::ops::Range<usize>, HighlightStyle)>,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let mut runs = Vec::new();
    let mut offset = 0;
    for (range, found) in matches {
        let start = range.start.max(offset).min(len);
        let end = range.end.min(len);
        if start >= end {
            continue;
        }
        if offset < start {
            runs.push((offset..start, style));
        }
        let mut matched = style;
        matched.background_color = found.background_color;
        runs.push((start..end, matched));
        offset = end;
    }
    if offset < len {
        runs.push((offset..len, style));
    }
    runs
}

fn reference_style(style: RunStyle, tokens: &Tokens) -> HighlightStyle {
    let color = if style.focused {
        tokens.fg_strong
    } else if style.unavailable {
        tokens.muted
    } else {
        tokens.accent
    };
    HighlightStyle {
        color: Some(color),
        font_weight: if style.bold {
            Some(FontWeight::BOLD)
        } else {
            style.reference.then_some(FontWeight::MEDIUM)
        },
        underline: (style.link || style.underline || style.unavailable || style.focused).then_some(
            UnderlineStyle {
                thickness: px(DECORATION_PX),
                color: Some(color),
                wavy: false,
            },
        ),
        ..HighlightStyle::default()
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::dashboard::Tokens;

    use super::{copied_link, pull_request_glyph, reference_fill, reference_style};
    use crate::markup::{PrRef, Ref};
    use crate::style::RunStyle;
    use crate::text::Opens;

    const WRITTEN: &str = "https://github.com/Octo-Org/API/pull/421/files?w=1#r9";

    fn pull_request() -> Opens {
        Opens::Ref(Ref::PullRequest(PrRef::from_url(WRITTEN).unwrap()))
    }

    #[test]
    fn copy_link_gives_a_pull_requests_url_as_written_and_other_links_unchanged() {
        assert_eq!(
            copied_link(&pull_request(), "subspace-irc:pr/x".into()),
            WRITTEN
        );
        let issue = Opens::Ref(Ref::Issue("TST-12".into()));
        assert_eq!(
            copied_link(&issue, "subspace-irc:issue/TST-12".into()),
            "subspace-irc:issue/TST-12"
        );
        let url = Opens::Url("https://example.org".into());
        assert_eq!(
            copied_link(&url, "https://example.org".into()),
            "https://example.org"
        );
    }

    #[test]
    fn only_a_pull_request_chip_leads_with_the_pull_request_glyph() {
        assert!(pull_request_glyph(&pull_request()).is_some());
        assert!(pull_request_glyph(&Opens::Ref(Ref::Issue("TST-12".into()))).is_none());
        assert!(pull_request_glyph(&Opens::Url("https://example.org".into())).is_none());
    }

    #[test]
    fn a_partial_find_match_preserves_reference_styling_and_native_run_order() {
        let tokens = Tokens::one_dark();
        let style = super::reference_style(
            crate::style::RunStyle {
                link: true,
                ..Default::default()
            },
            &tokens,
        );
        let highlight = gpui_kit::HighlightStyle {
            background_color: Some(tokens.select),
            ..Default::default()
        };
        let runs = super::reference_highlights(style, 8, vec![(2..5, highlight)]);
        assert_eq!(
            runs.iter()
                .map(|(range, _)| range.clone())
                .collect::<Vec<_>>(),
            [0..2, 2..5, 5..8]
        );
        assert_eq!(runs[1].1.background_color, Some(tokens.select));
        assert_eq!(runs[1].1.underline, style.underline);
        assert_eq!(runs[0].1.color, runs[1].1.color);
    }

    #[test]
    fn reference_style_distinguishes_unavailable_and_focused_references() {
        let tokens = Tokens::one_dark();
        let normal = reference_style(
            RunStyle {
                reference: true,
                ..RunStyle::default()
            },
            &tokens,
        );
        let unavailable = reference_style(
            RunStyle {
                unavailable: true,
                ..RunStyle::default()
            },
            &tokens,
        );
        let focused = reference_style(
            RunStyle {
                focused: true,
                unavailable: true,
                ..RunStyle::default()
            },
            &tokens,
        );
        assert_eq!(normal.color, Some(tokens.accent));
        assert_eq!(unavailable.color, Some(tokens.muted));
        assert!(unavailable.underline.is_some());
        assert_eq!(focused.color, Some(tokens.fg_strong));
        // The tint is the padded box's fill, never a glyph-tight text highlight.
        assert_eq!(normal.background_color, None);
        let fill = |style: RunStyle| reference_fill(style, &tokens);
        let reference = RunStyle {
            reference: true,
            ..RunStyle::default()
        };
        assert!(fill(reference).is_some());
        assert_eq!(
            fill(RunStyle {
                unavailable: true,
                ..RunStyle::default()
            }),
            None
        );
        assert_ne!(
            fill(RunStyle {
                focused: true,
                unavailable: true,
                ..RunStyle::default()
            }),
            fill(reference)
        );
    }

    #[test]
    fn normalised_urls_keep_the_original_links_underline_and_focus() {
        use super::References;
        use crate::markup::Mentions;
        use crate::style;
        use crate::text::{Opens, RefHandlers};
        use std::sync::Arc;

        for raw in [
            "https://example.org",
            "https://EXAMPLE.org/path",
            "https://example.org/é",
        ] {
            for text in [raw.to_owned(), format!("\u{2}{raw}\u{2}")] {
                let styled = style::with_refs(&text, &Mentions::default());
                let range = styled.links[0].0.clone();
                let styled = style::with_focus(styled, Some(range));
                let refs = References::new(RefHandlers {
                    open: Arc::new(|_, _, _| {}),
                    tooltip: Arc::new(|_, _, _| None),
                    look: Arc::new(|_, _| None),
                    styled: Some(styled),
                });
                let url = subspace_framework::dashboard::links::safe_url(raw).unwrap();
                let style = refs.style(&Opens::Url(url), 0);
                assert!(style.link, "link underline survives: {text:?}");
                assert!(style.focused, "keyboard focus survives: {text:?}");
                let look = reference_style(style, &Tokens::one_dark());
                assert!(look.underline.is_some());
                assert!(reference_fill(style, &Tokens::one_dark()).is_some());
            }
        }
    }

    #[test]
    fn escaped_duplicate_links_keep_the_second_occurrences_focus() {
        use super::{References, link_occurrence};
        use crate::markup::Mentions;
        use crate::style;
        use crate::text::{self, Opens, RefHandlers};
        use std::sync::Arc;

        let raw = "https://EXAMPLE.org/a(b)?x=1&y=2";
        let mut styled = style::with_refs(&format!("{raw} then {raw}"), &Mentions::default());
        let second = styled.links[1].0.clone();
        styled = style::with_focus(styled, Some(second));
        let markdown = text::literal(&styled, &Tokens::one_dark()).markdown;
        let prefix = &markdown[..markdown.rfind('[').unwrap()];
        let url = subspace_framework::dashboard::links::safe_url(raw).unwrap();
        let occurrence = link_occurrence(prefix, &url);
        assert_eq!(occurrence, 1);
        let refs = References::new(RefHandlers {
            open: Arc::new(|_, _, _| {}),
            tooltip: Arc::new(|_, _, _| None),
            look: Arc::new(|_, _| None),
            styled: Some(styled),
        });
        let found = Opens::Url(url);
        assert!(!refs.style(&found, 0).focused);
        assert!(refs.style(&found, occurrence).focused);
    }
}
