use super::NativeGrid;
mod toolbar;
use crate::anchored;
use crate::native_grid::{GridFormat, GridScope};
use crate::{
    action::{Intent, action_button},
    panel::IconName,
    toggle::toggle_row,
};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::v_flex;
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _};

pub(super) const OPTION_COUNT: usize = 8;
const OPTIONS: [&str; OPTION_COUNT] = [
    "Shaded header",
    "Alternating rows",
    "Cell boundaries",
    "Row hover",
    "Sorting",
    "Column filters",
    "Cell selection",
    "Clipboard copy",
];

impl NativeGrid {
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        toolbar::render(self, cx)
    }

    fn export_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let grid = cx.entity().downgrade();
        let selected = !self.model.selection.cells.is_empty();
        let button = action_button(
            self.child_id("export"),
            Intent::Quiet,
            Some(IconName::Download),
            "Export…",
        );
        anchored::dropdown_menu(button, move |mut menu, _, _| {
            for scope in [GridScope::Selected, GridScope::View, GridScope::All] {
                for format in [GridFormat::Csv, GridFormat::Json] {
                    let grid = grid.clone();
                    let label = format!(
                        "Export {} as {}…",
                        scope_label(scope),
                        if format == GridFormat::Csv {
                            "CSV"
                        } else {
                            "JSON"
                        }
                    );
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .disabled(scope == GridScope::Selected && !selected)
                            .on_click(move |_, _, cx| {
                                _ = grid.update(cx, |this, cx| this.export(scope, format, cx));
                            }),
                    );
                }
            }
            menu
        })
    }

    pub(super) fn options_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_1().p_2().children(
            OPTIONS
                .iter()
                .enumerate()
                .map(|(ix, label)| {
                    toggle_row(
                        *label,
                        crate::toggle::switch(self.child_id(format!("option-{ix}")), cx)
                            .checked(self.options[ix])
                            .on_change(cx.listener(move |this, value, window, cx| {
                                this.set_option(ix, *value, window, cx);
                            })),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
}

fn scope_label(scope: GridScope) -> &'static str {
    match scope {
        GridScope::Selected => "selected",
        GridScope::View => "view",
        GridScope::All => "all",
    }
}
