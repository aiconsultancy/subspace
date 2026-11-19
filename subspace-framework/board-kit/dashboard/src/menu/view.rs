//! How the menu draws itself: the popup, its keys, and one row per option.

use gpui_kit::component::input::Input;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, KeyContext, OperatingSystem,
    ParentElement, Render, SharedString, Stateful, StatefulInteractiveElement, Styled,
    TestSupportExt as _, Window, div, px,
};
use subspace_lucide::IconName;

use super::{
    ICON_SLOT_WIDTH_PX, MENU_CONTEXT, Menu, MenuClose, MenuEvent, MenuNext, MenuPick, MenuPick1,
    MenuPick2, MenuPick3, MenuPick4, MenuPick5, MenuPick6, MenuPick7, MenuPick8, MenuPick9,
    MenuPrevious, POPUP_MAX_HEIGHT_PX, POPUP_WIDTH_PX, ROW_HEIGHT_PX, RowAt,
};
use crate::MODAL_CONTEXT;
use crate::badge;
use crate::tokens::Tokens;
use crate::truncated;

impl Menu {
    /// The number badge that shows which digit key picks this row.
    fn badge(tokens: &Tokens, position: usize) -> Div {
        badge::badge((position + 1).to_string(), tokens.hair, tokens.muted)
    }

    /// The heading above a run of options.
    fn section_heading(tokens: &Tokens, index: usize, heading: SharedString) -> Stateful<Div> {
        div()
            .id(("menu-section", index))
            .px_2()
            .pt_1()
            .text_size(tokens.caption_size())
            .text_color(tokens.muted)
            .child(heading)
    }

    /// The icon slot: the option's glyph, or its favourite star.
    fn icon_slot(tokens: &Tokens, option: &super::MenuOption) -> Div {
        let slot = div()
            .w(px(ICON_SLOT_WIDTH_PX))
            .flex_shrink_0()
            .overflow_hidden()
            .whitespace_nowrap();
        match (option.starred, option.icon.clone()) {
            (Some(starred), _) => {
                let (icon, color) = match starred {
                    true => (IconName::StarFill, tokens.accent),
                    false => (IconName::Star, tokens.muted),
                };
                slot.child(Icon::from(icon).xsmall().text_color(color))
            }
            (None, Some(icon)) => slot.text_color(tokens.glyph(&icon)).child(icon.text),
            (None, None) => slot,
        }
    }

    /// One option row.
    fn option_row(
        &self,
        tokens: &Tokens,
        row: RowAt,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let position = row.position;
        let option = &self.options[row.index];
        h_flex()
            .id(("menu-option", row.index))
            .test_support()
            .h(px(ROW_HEIGHT_PX))
            .px_2()
            .gap_2()
            .rounded(tokens.control_radius)
            .when(option.starred.is_some(), |row| {
                row.aria_label(option.accessible_label())
            })
            .when(row.is_active(), |row| {
                row.bg(tokens.select).text_color(tokens.fg_strong)
            })
            .on_mouse_move(cx.listener(move |menu, _, _, cx| menu.hover(position, cx)))
            .on_click(cx.listener(move |menu, _, _, cx| menu.pick(position, cx)))
            .child(Self::icon_slot(tokens, option))
            .child(truncated::text(("menu-option", position), option.label.clone()).flex_1())
            .when(option.checked, |row| {
                row.child(div().text_color(tokens.accent).child("✓"))
            })
            .when(row.shows_badge(), |el| {
                el.child(Self::badge(tokens, position))
            })
    }

    /// holds the shell and dashboard keys off while the menu is open.
    fn key_context(os: OperatingSystem) -> KeyContext {
        let mut context = KeyContext::new_with_defaults(os);
        context.add(MENU_CONTEXT);
        context.add(MODAL_CONTEXT);
        context
    }

    /// Body styling and selection keys. The retained host owns dismissal.
    fn popup(
        tokens: &Tokens,
        active: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + ParentElement + use<> {
        v_flex()
            .id("menu")
            .test_support()
            .key_context(Self::key_context(cx.platform_info().os))
            .on_action(cx.listener(|menu, _: &MenuNext, _, cx| menu.step(1, cx)))
            .on_action(cx.listener(|menu, _: &MenuPrevious, _, cx| menu.step(-1, cx)))
            .on_action(cx.listener(move |menu, _: &MenuPick, _, cx| menu.pick(active, cx)))
            .on_action(cx.listener(|_, _: &MenuClose, _, cx| cx.emit(MenuEvent::Close)))
            .on_action(cx.listener(|m, _: &MenuPick1, _, cx| m.pick_digit(MenuPick1::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick2, _, cx| m.pick_digit(MenuPick2::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick3, _, cx| m.pick_digit(MenuPick3::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick4, _, cx| m.pick_digit(MenuPick4::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick5, _, cx| m.pick_digit(MenuPick5::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick6, _, cx| m.pick_digit(MenuPick6::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick7, _, cx| m.pick_digit(MenuPick7::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick8, _, cx| m.pick_digit(MenuPick8::DIGIT, cx)))
            .on_action(cx.listener(|m, _: &MenuPick9, _, cx| m.pick_digit(MenuPick9::DIGIT, cx)))
            .w(px(POPUP_WIDTH_PX))
            .max_h(px(POPUP_MAX_HEIGHT_PX))
            .p_1()
            .gap_1()
            .bg(tokens.surface)
            .border_1()
            .border_color(tokens.line)
            .rounded(tokens.surface_radius)
            .shadow_lg()
            .text_size(tokens.row_font)
            .text_color(tokens.fg)
    }
}

/// The option a digit key picks. The actions differ only in their digit.
trait DigitPick {
    const DIGIT: usize;
}

macro_rules! digit_pick {
    ($($action:ident => $digit:expr),* $(,)?) => {
        $(impl DigitPick for $action {
            const DIGIT: usize = $digit;
        })*
    };
}

digit_pick!(
    MenuPick1 => 1,
    MenuPick2 => 2,
    MenuPick3 => 3,
    MenuPick4 => 4,
    MenuPick5 => 5,
    MenuPick6 => 6,
    MenuPick7 => 7,
    MenuPick8 => 8,
    MenuPick9 => 9,
);

impl Render for Menu {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let shown = self.shown(cx);
        let active = self.active.min(shown.len().saturating_sub(1));
        let numbered = self.input.read(cx).value().is_empty();
        let headings = super::headings(&self.options, &shown);
        let rows: Vec<AnyElement> = shown
            .iter()
            .zip(headings)
            .enumerate()
            .map(|(position, (&index, heading))| {
                let row = RowAt {
                    index,
                    position,
                    active,
                    numbered,
                };
                v_flex()
                    .children(heading.map(|title| Self::section_heading(&tokens, index, title)))
                    .child(self.option_row(&tokens, row, cx))
                    .into_any_element()
            })
            .collect();
        let note = super::note_text(self.note.as_ref(), shown.is_empty());
        let no_match = note.map(|note| {
            div()
                .id("menu-note")
                .px_2()
                .py_1()
                .text_color(tokens.muted)
                .child(note)
        });
        let list = v_flex()
            .id("menu-list")
            .overflow_y_scroll()
            .border_t_1()
            .border_color(tokens.line)
            .pt_1()
            .children(rows)
            .children(no_match);

        Self::popup(&tokens, active, cx)
            .child(Input::new(&self.input).appearance(false))
            .child(list)
    }
}
