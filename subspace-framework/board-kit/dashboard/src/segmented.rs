//! Compact exclusive choices that remain one atomic toolbar unit.

use std::rc::Rc;

use crate::icon_button::IconButton;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, ClickEvent, Div, ParentElement as _, SharedString, Styled as _, Window};
use subspace_lucide::SvgIcon;

use crate::Tokens;

/// One labelled choice in a [`control`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub label: SharedString,
    pub icon: Option<SvgIcon>,
}

impl Segment {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
        }
    }

    pub fn icon(mut self, icon: SvgIcon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// Returns the selected index if it names an item. A required choice that the
/// user has not made yet has no selection.
fn selected_index(item_count: usize, selected: Option<usize>) -> Option<usize> {
    selected.filter(|index| *index < item_count)
}

/// A compact, keyboard-focusable exclusive choice group. `selected` is `None`
/// until the user makes a choice that has no default.
// qual:api - board consumers own callbacks and place the returned atomic unit.
pub fn control(
    id: &'static str,
    items: impl IntoIterator<Item = Segment>,
    selected: Option<usize>,
    tokens: &Tokens,
    on_select: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let items: Vec<_> = items.into_iter().collect();
    let selected = selected_index(items.len(), selected);
    let on_select = Rc::new(on_select);
    h_flex()
        .flex_shrink_0()
        .border_1()
        .border_color(tokens.line)
        .rounded(tokens.control_radius)
        .children(items.into_iter().enumerate().map(|(index, item)| {
            let label = item.label;
            let callback = on_select.clone();
            let chosen = selected == Some(index);
            // The ghost variant's own selected tint is too faint to read at a
            // glance, so the chosen segment uses the shared selection colour.
            let mut button = IconButton::new((id, index))
                .compact()
                .ghost()
                .xsmall()
                .rounded(tokens.control_radius)
                .selected(chosen)
                .toggled(chosen)
                .when(chosen, |button| button.bg(tokens.select))
                .text_color(if chosen {
                    tokens.fg_strong
                } else {
                    tokens.muted
                })
                .accessibility_label(label.clone())
                .label(label);
            if let Some(icon) = item.icon {
                button = button.icon(icon);
            }
            button.on_click(move |event, window, cx| callback(index, event, window, cx))
        }))
}

#[cfg(test)]
mod tests {
    use super::selected_index;

    #[test]
    fn only_a_real_choice_is_selected() {
        assert_eq!(selected_index(3, Some(1)), Some(1));
        assert_eq!(selected_index(3, None), None);
        assert_eq!(selected_index(3, Some(9)), None);
        assert_eq!(selected_index(0, Some(0)), None);
    }
}
