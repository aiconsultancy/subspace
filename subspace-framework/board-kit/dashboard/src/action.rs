//! Canonical application action constructors. Each one takes the theme's
//! geometry for its [`ButtonRole`]: height, padding, text size and radius.
//! Boards must not restyle colour or typography.

use crate::icon_button::IconButton;
use crate::tokens::ButtonRole;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex};
use gpui_kit::{
    App, ElementId, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _, px,
};
use subspace_lucide::SvgIcon;

/// Header actions sit 4 px shorter than the previous 32 px board override.
const HEADER_BUTTON_HEIGHT: f32 = 28.;
/// GPUI Kit 0.9.1 labelled `Size::XSmall` buttons pad with `px_1` (4 px).
/// Header actions add 2 px on each side.
const HEADER_BUTTON_PAD_X: f32 = 6.;

/// Feedback for an action. Keep the result visible until the next action or edit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActionState {
    #[default]
    Idle,
    Busy,
    Success,
    Failure,
}

impl ActionState {
    /// Labels in idle, busy, success and failure order. Result text must not rely on colour.
    pub fn label(self, labels: [&str; 4]) -> &str {
        labels[match self {
            Self::Idle => 0,
            Self::Busy => 1,
            Self::Success => 2,
            Self::Failure => 3,
        }]
    }

    /// Apply native loading and result styles without changing the handler or label.
    // qual:api
    pub fn apply(self, button: IconButton) -> IconButton {
        match self {
            Self::Idle => button,
            Self::Busy => button.loading(true).disabled(true),
            Self::Success => button.success().icon(subspace_lucide::IconName::Check),
            Self::Failure => button
                .danger()
                .icon(subspace_lucide::IconName::TriangleAlert),
        }
    }
}

/// What a button does, which decides its native GPUI Kit variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Approve or confirm: green.
    Accept,
    /// Deny or remove: red.
    Reject,
    /// Allowed but risky, such as approving everything: amber.
    Caution,
    /// The main way forward, such as Reconnect.
    Primary,
    /// A plain choice, such as Cancel or an answer: outlined.
    Neutral,
    /// A small chrome control, such as Close: no frame until hovered.
    Quiet,
}

/// Menu triggers are primary only when they represent a non-default choice.
fn menu_button_intent(active: bool) -> Intent {
    if active {
        Intent::Primary
    } else {
        Intent::Neutral
    }
}

/// An icon button with visible text, such as ✓ Allow.
pub fn action_button(
    id: impl Into<ElementId>,
    intent: Intent,
    icon: Option<SvgIcon>,
    label: impl Into<SharedString>,
) -> IconButton {
    labelled(id, ButtonRole::Action, intent, icon, label)
}

/// The Space's one main action, at the trailing end of its pane toolbar,
/// such as New issue…: a filled primary button in the Toolbar role, its label
/// semibold. A toolbar has at most one; every other toolbar action is
/// neutral or quiet.
pub fn main_action(
    id: impl Into<ElementId>,
    icon: SvgIcon,
    label: impl Into<SharedString>,
) -> IconButton {
    labelled(id, ButtonRole::Toolbar, Intent::Primary, Some(icon), label)
        .font_weight(FontWeight::SEMIBOLD)
}

/// A dialog footer button, such as Cancel or Save. Put the dismissing action
/// first and the committing one last.
pub fn dialog_button(
    id: impl Into<ElementId>,
    intent: Intent,
    icon: Option<SvgIcon>,
    label: impl Into<SharedString>,
) -> IconButton {
    labelled(id, ButtonRole::Dialog, intent, icon, label)
}

fn labelled(
    id: impl Into<ElementId>,
    role: ButtonRole,
    intent: Intent,
    icon: Option<SvgIcon>,
    label: impl Into<SharedString>,
) -> IconButton {
    let label = label.into();
    let button = styled(IconButton::from_button(Button::new(id)), intent)
        .role(role)
        .xsmall()
        .accessibility_label(label.clone())
        .label(label);
    match icon {
        Some(icon) => button.icon(icon),
        None => button,
    }
}

/// A labelled menu-picker trigger with a decorative trailing chevron.
pub fn menu_button(
    id: impl Into<ElementId>,
    icon: SvgIcon,
    label: impl Into<SharedString>,
    active: bool,
) -> IconButton {
    // The native caret is decoration; the spoken label stays the plain label.
    let label = label.into();
    action_button(id, menu_button_intent(active), Some(icon), label.clone())
        .role(ButtonRole::Toolbar)
        .dropdown_caret(true)
        .accessibility_label(label)
}

/// A filter's menu trigger named by its label alone, such as Assignee or
/// Priority in a pane toolbar: the decorative trailing chevron and no leading
/// glyph. Like [`menu_button`], it is primary only while its filter is set.
pub fn filter_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
) -> IconButton {
    let label = label.into();
    let intent = menu_button_intent(active);
    labelled(id, ButtonRole::Toolbar, intent, None, label.clone())
        .dropdown_caret(true)
        .accessibility_label(label)
}

/// A header action with the same text and colour as [`action_button`].
/// Icon-bearing controls use the global size; text-only controls keep the
/// historical 28px frame and 6px horizontal padding.
pub fn header_button(
    id: impl Into<ElementId>,
    intent: Intent,
    icon: Option<SvgIcon>,
    label: impl Into<SharedString>,
) -> IconButton {
    action_button(id, intent, icon, label)
        .h(px(HEADER_BUTTON_HEIGHT))
        .px(px(HEADER_BUTTON_PAD_X))
}

/// An icon-only button; `name` is its tooltip and accessible label.
pub fn icon_button(
    id: impl Into<ElementId>,
    intent: Intent,
    icon: SvgIcon,
    name: impl Into<SharedString>,
) -> IconButton {
    let name = name.into();
    styled(IconButton::from_button(Button::new(id)), intent)
        .xsmall()
        .icon(icon)
        .tooltip(name.clone())
        .accessibility_label(name)
}

/// A full-width quiet row control, such as a person in a member list: its
/// `content` (an avatar and a name, say) starts at the leading edge and
/// `name` is its accessible label.
// qual:api
pub fn row_button(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    content: impl IntoElement,
) -> IconButton {
    styled(IconButton::from_button(Button::new(id)), Intent::Quiet)
        .small()
        .accessibility_label(name)
        .w_full()
        .child(h_flex().flex_1().min_w_0().gap_2().child(content))
}

/// A bounded card whose contents set its height. The whole card opens the
/// destination and keeps the native Button's focus and press behavior.
pub fn card_button(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    content: impl IntoElement,
) -> Button {
    Button::new(id)
        .outline()
        .accessibility_label(name)
        .w_full()
        .h_auto()
        .px_3()
        .py_2()
        .child(content)
}

/// A borderless toolbar action with optional text and semantic state colour.
pub fn toolbar_button(
    id: impl Into<ElementId>,
    icon: SvgIcon,
    name: impl Into<SharedString>,
    presentation: (Intent, bool),
    cx: &App,
) -> IconButton {
    let (intent, show_label) = presentation;
    let name = name.into();
    let tokens = crate::Tokens::current(cx);
    let button = IconButton::new(id)
        .ghost()
        .xsmall()
        .icon(icon)
        .tooltip(name.clone())
        .accessibility_label(name.clone());
    let button = match intent {
        Intent::Accept => button.text_color(tokens.green),
        Intent::Reject => button.text_color(tokens.red),
        Intent::Caution => button.text_color(tokens.orange),
        Intent::Primary => button.text_color(tokens.accent),
        Intent::Neutral | Intent::Quiet => button,
    };
    if show_label {
        button.label(name)
    } else {
        button
    }
}

fn styled(button: IconButton, intent: Intent) -> IconButton {
    match intent {
        Intent::Accept => button.success(),
        Intent::Reject => button.danger(),
        Intent::Caution => button.warning(),
        Intent::Primary => button.primary(),
        Intent::Neutral => button.outline(),
        Intent::Quiet => button.ghost(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionState, Intent, menu_button_intent};

    #[test]
    fn action_results_have_distinct_text_even_without_colour() {
        let labels = ["Validate", "Validating…", "Valid", "Invalid"];
        assert_eq!(ActionState::default().label(labels), "Validate");
        assert_eq!(ActionState::Busy.label(labels), "Validating…");
        assert_eq!(ActionState::Success.label(labels), "Valid");
        assert_eq!(ActionState::Failure.label(labels), "Invalid");
    }

    #[test]
    fn menu_buttons_are_primary_only_for_a_non_default_choice() {
        assert_eq!(menu_button_intent(false), Intent::Neutral);
        assert_eq!(menu_button_intent(true), Intent::Primary);
    }
}
