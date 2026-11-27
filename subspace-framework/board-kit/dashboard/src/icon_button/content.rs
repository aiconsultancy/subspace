use super::IconButtonSize;
use super::size::Nominal;
use crate::tokens::{ButtonMetrics, ButtonRole};
use gpui_kit::component::{
    Disableable as _, Icon, Sizable as _, Size, button::Button, h_flex, spinner::Spinner,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Rems,
    RenderOnce, SharedString, Styled as _, TestSupportExt as _, Window, div, rems,
};

pub(super) struct Content {
    pub(super) icon: Option<Icon>,
    pub(super) label: Option<SharedString>,
    pub(super) loading: bool,
    pub(super) children: Vec<AnyElement>,
    /// A quiet button's glyph and text colour under the pointer.
    pub(super) hover_glyph: Option<Hsla>,
    /// A quiet button's glyph and text colour at rest, kept while it is busy.
    pub(super) glyph: Option<Hsla>,
}

/// The hover group that brightens a quiet button's glyph and text.
const HOVER_GROUP: &str = "subspace-icon-button";

/// The compact preset's nominal geometry, in rems at its default height.
const COMPACT: Nominal = Nominal {
    glyph: 0.75,
    target: 1.25,
    gap: 0.25,
    pad: 0.25,
};
const HALF: f32 = 0.5;

/// A button's resolved frame, glyph, spacing and text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    pub(super) glyph: Rems,
    pub(super) target: Rems,
    pub(super) gap: Rems,
    pub(super) pad: Rems,
    pub(super) font: Rems,
    /// The role's corner radius in token pixels, at most half the target.
    pub(super) radius: f32,
}

impl Geometry {
    /// Scale the role's nominal geometry by its height and padding tokens.
    /// Compact is a fixed preset; every other role follows the global size.
    pub(super) fn resolve(role: ButtonRole, metrics: ButtonMetrics, size: IconButtonSize) -> Self {
        let (nominal, reference) = match role {
            ButtonRole::Compact => (COMPACT, (COMPACT.target, COMPACT.pad)),
            _ => {
                let (height, pad) = IconButtonSize::reference();
                let reference = (height / REFERENCE, pad / REFERENCE);
                (size.nominal(), reference)
            }
        };
        let scale = f32::from(metrics.height) / REFERENCE / reference.0;
        let pad_scale = f32::from(metrics.pad_x) / REFERENCE / reference.1;
        let height = f32::from(metrics.height) * (nominal.target / reference.0);
        Self {
            glyph: rems(nominal.glyph * scale),
            target: rems(nominal.target * scale),
            gap: rems(nominal.gap * scale),
            pad: rems(nominal.pad * pad_scale),
            font: crate::tokens::rem(metrics.font),
            radius: f32::from(metrics.radius).min(height * HALF),
        }
    }

    /// The radius in window pixels for an interface base of `rem`.
    pub(super) fn radius_at(self, rem: Pixels) -> Pixels {
        rem * (self.radius / REFERENCE)
    }
}

const REFERENCE: f32 = crate::density::REFERENCE_REM;

/// A loading button: disabled, so pointers, keys and assistive technology
/// all find it unavailable, but drawn in its resting colours, faded as GPUI
/// Kit fades a loading button. It is not marked loading: GPUI Kit 0.11 then
/// draws its own spinner beside this glyph's. A quiet button keeps its glyph
/// colour; GPUI Kit replays the button's own fill and edge in its disabled
/// state, and a native variant takes GPUI Kit's disabled fill, as GPUI Kit
/// does not expose its resting colours.
fn busy(button: Button, glyph: Option<Hsla>) -> Button {
    button
        .disabled(true)
        .opacity(BUSY_OPACITY)
        .when_some(glyph, |button, glyph| button.text_color(glyph))
}

/// GPUI Kit's fade of a loading button.
const BUSY_OPACITY: f32 = 0.8;

pub(super) fn compose(mut button: Button, content: Content, geometry: Geometry) -> Button {
    let icon_only = content.label.is_none() && content.children.is_empty();
    let has_width = button.style().size.width.is_some();
    button = button
        .h(geometry.target)
        .min_w(geometry.target)
        .when(icon_only, |button| button.px_0())
        .when(!icon_only, |button| button.px(geometry.pad))
        .when(icon_only && !has_width, |button| button.w(geometry.target))
        .when(content.hover_glyph.is_some(), |button| {
            button.group(HOVER_GROUP)
        })
        .when(content.loading, |button| busy(button, content.glyph));
    let label = content.label.map(|label| {
        div()
            .id("icon-button-label")
            .test_support()
            .min_w_0()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(label)
    });
    button.child(
        h_flex()
            .min_w_0()
            .max_w_full()
            .gap(geometry.gap)
            .text_size(geometry.font)
            .when_some(content.hover_glyph, |row, color| {
                row.group_hover(HOVER_GROUP, |style| style.text_color(color))
            })
            .when_some(content.icon, |row, icon| {
                row.child(Glyph {
                    icon,
                    size: geometry.glyph,
                    loading: content.loading,
                })
            })
            .children(label)
            .children(content.children),
    )
}

#[derive(IntoElement)]
struct Glyph {
    icon: Icon,
    size: Rems,
    loading: bool,
}

impl RenderOnce for Glyph {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let glyph = if self.loading {
            // Spinner's typed Size API takes resolved pixels, unlike Icon's Styled API.
            Spinner::new()
                .with_size(Size::Size(self.size.to_pixels(window.rem_size())))
                .into_any_element()
        } else {
            self.icon.size(self.size).into_any_element()
        };
        div()
            .id("toolbar-icon")
            .test_support()
            .flex_shrink_0()
            .size(self.size)
            .child(glyph)
    }
}
