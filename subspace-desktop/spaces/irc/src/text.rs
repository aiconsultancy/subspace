//! One selectable text view per transcript row. Kit numbers views in paint
//! order, so drag selections copy every body in reading order. Plain and
//! mIRC text becomes literal Markdown; colours become theme tints.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::{AnyElement, AnyView, App, Hsla, IntoElement as _, SharedString, Window};
use subspace_framework::dashboard::{Tokens, Tone, rich};

use crate::markup::{self, Ref, Rendered};
use crate::style::Styled;
use crate::transcript::Claim;

pub(crate) mod look;
mod references;

pub(crate) use look::{Look, RefLook};

const MIRC_TINT: f32 = 0.16;

/// What a clickable range of message text opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Opens {
    Url(String),
    Ref(Ref),
    /// A chat upload's thumbnail under the text, by its key.
    Upload(String),
}

/// Clickable ranges in reading order, also used by transcript keyboard navigation.
pub(crate) fn targets(styled: &Styled, references: bool) -> Vec<(Range<usize>, Opens)> {
    let urls = styled
        .links
        .iter()
        .map(|(range, url)| (range.clone(), Opens::Url(url.clone())));
    let refs = styled
        .refs
        .iter()
        .filter(|_| references)
        .map(|(range, found)| (range.clone(), Opens::Ref(found.clone())));
    let mut targets: Vec<_> = urls.chain(refs).collect();
    targets.sort_by_key(|(range, _)| range.start);
    targets
}

pub(crate) type RefTooltip =
    Arc<dyn Fn(&Ref, &mut Window, &mut App) -> Option<AnyView> + Send + Sync>;

/// Reference behaviour plus the literal row's presentation, when applicable.
pub(crate) struct RefHandlers {
    pub open: Claim,
    pub tooltip: RefTooltip,
    /// The answered state a reference's chip shows.
    pub look: RefLook,
    pub styled: Option<Styled>,
}

/// `styled` as literal Markdown, with mIRC colours as tints.
pub(crate) fn literal(styled: &Styled, tokens: &Tokens) -> Rendered {
    let tint = |tone: Tone| tinted(tone).then(|| css_hex(tokens.tone(tone).opacity(MIRC_TINT)));
    Rendered {
        markdown: markup::literal(styled, &tint),
        plain: styled.text.clone(),
        refs: Vec::new(),
    }
}

fn tinted(tone: Tone) -> bool {
    !matches!(tone, Tone::Fg | Tone::Strong)
}

fn css_hex(color: Hsla) -> String {
    format!("#{:08x}", u32::from(color.to_rgb()))
}

/// One selectable view and the shared Copy menu. Literal references keep
/// their native styling and hover cards through Kit's inline plugin API.
pub(crate) fn selectable(
    id: SharedString,
    text: Rendered,
    tone: Tone,
    refs: Option<RefHandlers>,
    cx: &App,
) -> AnyElement {
    prepared(id, text, tone, refs, cx).into_any_element()
}

pub(crate) fn prepared(
    id: SharedString,
    text: Rendered,
    tone: Tone,
    refs: Option<RefHandlers>,
    cx: &App,
) -> rich::RichText {
    let claim = refs.as_ref().map(|refs| refs.open.clone());
    let view = rich::rich_text_prepared(id, text.markdown, move |url, window, cx| {
        let found = claim.as_ref().zip(markup::parse_ref_url(url));
        found.is_some_and(|(claim, found)| {
            claim(&found, window, cx);
            true
        })
    });
    let view = match refs {
        Some(refs) => view.native(move |view, context| {
            view.plugin(references::References::new(refs).context(context))
        }),
        None => view,
    };
    let view = if tone == Tone::Fg {
        view
    } else {
        rich::tinted(view, Tokens::current(cx).tone(tone), cx)
    };
    view.copy_text(text.plain)
}

#[cfg(test)]
mod tests {
    use gpui_kit::hsla;
    use subspace_framework::dashboard::Tone;

    use super::{css_hex, tinted};

    #[test]
    fn tints_are_css_hex_with_alpha_and_skip_the_text_tones() {
        assert_eq!(css_hex(hsla(0., 1., 0.5, 0.5)), "#ff00007f");
        assert!(tinted(Tone::Red));
        assert!(!tinted(Tone::Fg) && !tinted(Tone::Strong));
    }
}
