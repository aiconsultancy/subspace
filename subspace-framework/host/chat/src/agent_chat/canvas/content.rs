use super::ChatPanel;
use super::table;
use crate::chart::Chart;
use crate::rich;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Image, ImageFormat, IntoElement as _, ObjectFit, ParentElement as _,
    SharedString, Styled as _, div, img, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _, StyledImage as _};
use std::sync::Arc;
use subspace_agent::canvas::{Canvas, CanvasContent};
const DOCK_ASPECT: f32 = 0.8;
const DARK_LIGHTNESS: f32 = 0.5;

impl ChatPanel {
    pub(super) fn canvas_thumbnail(canvas: &Canvas, cx: &mut Context<Self>) -> AnyElement {
        match &canvas.content {
            CanvasContent::Chart(model) => Chart::new(
                format!("canvas-preview-{}", canvas.id),
                model.as_ref().clone(),
            )
            .label(canvas.title.clone())
            .fill()
            .into_any_element(),
            CanvasContent::Svg { light, dark, .. } => img(Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                themed_svg(light, dark, crate::Tokens::current(cx).bg.l),
            )))
            .size_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
            CanvasContent::Error(_) => div().child("Preview unavailable").into_any_element(),
        }
    }

    pub(super) fn canvas_content(
        &self,
        canvas: &Canvas,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .when(self.tab().canvases.gallery, |body| {
                body.child(Self::canvas_previous("gallery-prev", cx))
            })
            .child(self.canvas_rendering(canvas, width, cx))
            .when(self.tab().canvases.gallery, |body| {
                body.child(Self::canvas_next("gallery-next", cx))
            })
            .into_any_element()
    }

    fn canvas_rendering(&self, canvas: &Canvas, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let list = &self.tab().canvases;
        if source_only(canvas, list.data) {
            return text_alternative(canvas);
        }
        self.render_visual(canvas, width, cx)
    }

    fn render_visual(&self, canvas: &Canvas, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let list = &self.tab().canvases;
        match &canvas.content {
            CanvasContent::Chart(model) => {
                let chart = Chart::new("chat-native-chart", model.as_ref().clone())
                    .label(canvas.title.clone());
                v_flex()
                    .flex_1()
                    .h_full()
                    .min_h_0()
                    .min_w_0()
                    .child(chart_view(chart, list.gallery, list.data, width))
                    .when(list.data, |body| body.child(chart_table(model)))
                    .into_any_element()
            }
            CanvasContent::Svg {
                light,
                dark,
                aspect,
            } => v_flex()
                .id("canvas-image")
                .aria_label(canvas.title.clone())
                .flex_1()
                .h_full()
                .min_h_0()
                .min_w_0()
                .child(
                    div()
                        .w_full()
                        .when(list.gallery, |frame| frame.flex_1().min_h_0())
                        .when(!list.gallery, |frame| {
                            frame.h(svg_dock_height(width, *aspect))
                        })
                        .child(
                            img(Arc::new(Image::from_bytes(
                                ImageFormat::Svg,
                                themed_svg(light, dark, crate::Tokens::current(cx).bg.l),
                            )))
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                        ),
                )
                .into_any_element(),
            CanvasContent::Error(error) => v_flex()
                .flex_1()
                .child(rich::plain_text("canvas-error", error))
                .child(text_alternative(canvas))
                .into_any_element(),
        }
    }
}

fn source_only(canvas: &Canvas, data: bool) -> bool {
    data && !matches!(canvas.content, CanvasContent::Chart(_))
}

fn themed_svg(light: &str, dark: &str, lightness: f32) -> Vec<u8> {
    if lightness < DARK_LIGHTNESS {
        dark.as_bytes().to_vec()
    } else {
        light.as_bytes().to_vec()
    }
}

fn chart_view(chart: Chart, gallery: bool, data: bool, width: f32) -> AnyElement {
    if full_chart(gallery, data) {
        chart.fill().into_any_element()
    } else {
        chart.height(dock_height(width)).into_any_element()
    }
}

fn full_chart(gallery: bool, data: bool) -> bool {
    gallery && !data
}

fn text_alternative(canvas: &Canvas) -> AnyElement {
    match &canvas.content {
        CanvasContent::Chart(model) => chart_table(model),
        _ => div()
            .id("canvas-source")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_2()
            .child(canvas.source.clone())
            .into_any_element(),
    }
}

fn chart_table(model: &subspace_charts::ChartModel) -> AnyElement {
    v_flex()
        .id("canvas-data-table")
        .role(gpui_kit::accesskit::Role::Table)
        .aria_label("Chart data table")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(table_row(table::HEADER.map(str::to_owned)))
        .children(table::rows(model).into_iter().map(table_row))
        .into_any_element()
}

fn table_row(cells: table::Row) -> gpui_kit::Div {
    h_flex().children(cells.map(cell))
}

fn dock_height(width: f32) -> gpui_kit::Pixels {
    px(width * DOCK_ASPECT)
}

/// A docked drawing keeps its own proportions up to a square, so a wide
/// flowchart sits at the top at full width instead of shrinking into the
/// middle of the column.
fn svg_dock_height(width: f32, aspect: f32) -> gpui_kit::Pixels {
    px(width * aspect.clamp(MIN_SVG_ASPECT, MAX_SVG_ASPECT))
}

const MIN_SVG_ASPECT: f32 = 0.2;
const MAX_SVG_ASPECT: f32 = 1.;

const TABLE_CELL_WIDTH: f32 = 150.;

fn cell(text: impl Into<SharedString>) -> gpui_kit::Div {
    div()
        .w(px(TABLE_CELL_WIDTH))
        .flex_shrink_0()
        .p_1()
        .child(text.into())
}
