use super::NativeGrid;
use crate::native_grid::selection::Cell;
use crate::{
    Tokens,
    action::{Intent, action_button},
    fields::{self, InputRole},
    panel::IconName,
};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, rems, uniform_list,
};
use std::ops::Range;

const COLUMN_WIDTH_REM: f32 = 6.;

impl Render for NativeGrid {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .id(self.id.clone())
            .min_w_0()
            .gap_1()
            .child(self.label.clone())
            .child(self.toolbar(cx))
            .when(self.options_open, |view| view.child(self.options_panel(cx)))
            .child(self.table(cx))
            .child(
                div()
                    .id(self.child_id("status"))
                    .test_support()
                    .role(gpui_kit::Role::Status)
                    .aria_label(format!(
                        "{} of {} rows · {} cells selected · {}",
                        self.model.shown.len(),
                        self.model.source.rows.len(),
                        self.model.selection.cells.len(),
                        self.status
                    ))
                    .text_sm()
                    .text_color(tokens.muted)
                    .child(format!(
                        "{} of {} rows · {} cells selected · {}",
                        self.model.shown.len(),
                        self.model.source.rows.len(),
                        self.model.selection.cells.len(),
                        self.status
                    )),
            )
    }
}

impl NativeGrid {
    fn table(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(self.child_id("horizontal"))
            .overflow_x_scroll()
            .track_scroll(&self.horizontal)
            .w_full()
            .min_w_0()
            .child(
                v_flex()
                    .w_full()
                    .min_w(rems(
                        COLUMN_WIDTH_REM * self.model.source.columns.len() as f32,
                    ))
                    .child(self.header(cx))
                    .when(self.options[5], |view| view.child(self.filter_row()))
                    .child(self.viewport(cx)),
            )
    }

    fn viewport(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        div()
            .id(self.child_id("viewport"))
            .test_support()
            .role(gpui_kit::Role::Grid)
            .aria_label(self.label.clone())
            .track_focus(&self.focus)
            .key_context("NativeGrid")
            .on_action(cx.listener(Self::copy_action))
            .on_key_down(cx.listener(Self::key_down))
            .border_y_1()
            .border_color(tokens.line)
            .focus_visible(|style| style.border_color(tokens.accent))
            .h_64()
            .w_full()
            .min_w_0()
            .when(self.model.shown.is_empty(), |view| {
                view.child(
                    div()
                        .p_3()
                        .child("No matching rows. Clear filters to show all results."),
                )
            })
            .when(!self.model.shown.is_empty(), |view| {
                view.child(
                    uniform_list(
                        self.child_id(format!("rows-{}", self.revision)),
                        self.model.shown.len(),
                        cx.processor(|this, range: Range<usize>, _, cx| {
                            range.map(|ix| this.row(ix, cx)).collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .size_full(),
                )
            })
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        h_flex()
            .items_stretch()
            .w_full()
            .when(self.options[0], |view| view.bg(tokens.pane))
            .children(
                self.model
                    .source
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(column, _)| {
                        div()
                            .id(self.child_id(format!("header-{column}")))
                            .test_support()
                            .role(gpui_kit::Role::ColumnHeader)
                            .flex_1()
                            .min_w_0()
                            .when(self.options[2], |view| {
                                view.border_r_1().border_b_1().border_color(tokens.line)
                            })
                            .child(
                                action_button(
                                    self.child_id(format!("sort-{column}")),
                                    Intent::Quiet,
                                    Some(IconName::ArrowUpDown),
                                    self.sort_label(column),
                                )
                                .compact()
                                .w_full()
                                .disabled(!self.options[4])
                                .on_click(cx.listener(move |this, _, _, cx| this.sort(column, cx))),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
    }

    fn filter_row(&self) -> impl IntoElement {
        h_flex().items_stretch().w_full().children(
            self.filters
                .iter()
                .enumerate()
                .map(|(column, input)| {
                    div().flex_1().min_w_0().child(
                        fields::input(
                            input,
                            InputRole::Compact,
                            format!("Filter {}", self.model.source.columns[column]),
                        )
                        .id(self.child_id(format!("filter-{column}"))),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    fn row(&self, visible: usize, cx: &mut Context<Self>) -> AnyElement {
        let row = self.model.shown[visible];
        let tokens = Tokens::current(cx);
        h_flex()
            .id(self.child_id(format!("row-{row}")))
            .test_support()
            .role(gpui_kit::Role::Row)
            .aria_label(
                self.model.source.rows[row]
                    .iter()
                    .map(|cell| cell.display())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
            .w_full()
            .h_8()
            .items_stretch()
            .when(self.options[1] && visible % 2 == 1, |view| {
                view.bg(tokens.pane)
            })
            .when(self.options[3], |view| {
                view.hover(|style| style.bg(tokens.select))
            })
            .children(
                self.model.source.rows[row]
                    .iter()
                    .enumerate()
                    .map(|(column, _)| self.cell(row, column, cx))
                    .collect::<Vec<_>>(),
            )
            .into_any_element()
    }

    fn cell(&self, row: usize, column: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let cell = &self.model.source.rows[row][column];
        let tokens = Tokens::current(cx);
        let selected = self.model.selection.cells.contains(&(row, column));
        let active = self.model.selection.active == Some(Cell { row, column });
        let revision = self.revision;
        div()
            .id(self.child_id(format!("row-{row}-cell-{column}")))
            .test_support()
            .role(gpui_kit::Role::Cell)
            .aria_label(cell.display())
            .aria_selected(selected)
            .flex_1()
            .min_w_0()
            .relative()
            .h_full()
            .px_2()
            .flex()
            .items_center()
            .truncate()
            .when(cell.is_numeric(), |view| view.justify_end())
            .when(self.options[2], |view| {
                view.border_r_1().border_b_1().border_color(tokens.line)
            })
            .when(selected, |view| {
                view.bg(tokens.select).text_color(tokens.fg_strong)
            })
            .when(active, |view| {
                view.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_1()
                        .border_color(tokens.accent),
                )
            })
            .on_click(
                cx.listener(move |this, event: &gpui_kit::ClickEvent, window, cx| {
                    if this.revision == revision {
                        this.select(Cell { row, column }, event.modifiers().shift, window, cx);
                    }
                }),
            )
            .child(cell.display())
    }
}
