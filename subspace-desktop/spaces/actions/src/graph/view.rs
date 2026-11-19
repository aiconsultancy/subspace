use super::{
    FlowGraph, GraphJob,
    layout::{Node, Point},
    paint,
};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, TestSupportExt as _, Window, div, px,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    feedback,
};
use subspace_framework::lucide::IconName;

const ZOOM_STEP: f32 = 1.2;
const ZOOM_OUT: f32 = 1. / ZOOM_STEP;
const CARD_PADDING: f32 = 12.;
const CARD_GAP: f32 = 6.;

impl Render for FlowGraph {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .size_full()
            .min_h_0()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .child(Self::toolbar(&tokens, cx))
            .children(self.layout.degraded.as_ref().map(|reason| {
                feedback::error_banner_with_actions(
                    &tokens,
                    "actions-graph",
                    "Dependencies unavailable",
                    reason.clone(),
                    std::iter::empty::<gpui_kit::AnyElement>(),
                )
            }))
            .child(self.surface(&tokens, cx))
    }
}

impl FlowGraph {
    fn toolbar(tokens: &Tokens, cx: &Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .p_2()
            .flex_shrink_0()
            .border_b_1()
            .border_color(tokens.line)
            .child(zoom_button(true, cx))
            .child(zoom_button(false, cx))
            .child(
                action_button("graph-fit", Intent::Quiet, Some(IconName::Maximize), "Fit")
                    .on_click(cx.listener(|graph, _, _, cx| graph.fit(cx))),
            )
            .child(
                action_button(
                    "graph-reset",
                    Intent::Quiet,
                    Some(IconName::RotateCcw),
                    "Reset",
                )
                .on_click(cx.listener(|graph, _, _, cx| graph.reset(cx))),
            )
    }

    fn zoom_center(&mut self, factor: f32, cx: &mut Context<Self>) {
        self.viewport.zoom(
            factor,
            Point {
                x: self.size.x / super::layout::MIDPOINT_DIVISOR,
                y: self.size.y / super::layout::MIDPOINT_DIVISOR,
            },
        );
        cx.notify();
    }

    fn surface(&self, tokens: &Tokens, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("graph-surface")
            .test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .tab_stop(true)
            .role(gpui_kit::accesskit::Role::Group)
            .aria_label("Job flow. Arrow keys select jobs, Enter opens details, 0 fits the graph.")
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|graph, event: &gpui_kit::MouseDownEvent, window, cx| {
                    graph.begin_drag(None, event.position, window, cx)
                }),
            )
            .on_mouse_move(cx.listener(Self::move_pointer))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|graph, _, _, _| graph.drag = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|graph, _, _, _| graph.drag = None),
            )
            .on_scroll(cx.listener(Self::scroll))
            .on_pinch(cx.listener(Self::pinch))
            .child(paint::edges(self, *tokens, cx))
            .children(self.layout.nodes.iter().filter_map(|node| {
                let job = self.jobs.iter().find(|job| job.id == node.id)?;
                Some(self.card((node, job), tokens, cx))
            }))
    }

    fn card(
        &self,
        item: (&Node, &GraphJob),
        tokens: &Tokens,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let (node, job) = item;
        let id = job.id;
        let rect = self.viewport.rect(node);
        let screen = self.viewport.screen(rect.origin);
        let scale = self.viewport.scale;
        v_flex()
            .id(("graph-job", id))
            .test_support()
            .absolute()
            .left(px(screen.x))
            .top(px(screen.y))
            .w(px(rect.width * scale))
            .h(px(rect.height * scale))
            .p(px(CARD_PADDING * scale))
            .gap(px(CARD_GAP * scale))
            .overflow_hidden()
            .bg(tokens.pane)
            .rounded(tokens.surface_radius * scale)
            .border_1()
            .border_color(if self.selected == Some(id) {
                tokens.accent
            } else {
                tokens.line
            })
            .text_size(tokens.row_font * scale)
            .cursor_pointer()
            .role(gpui_kit::accesskit::Role::Button)
            .aria_label(format!("Job {}, {}", job.name, job.status))
            .aria_description(self.dependencies(id))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |graph, event: &gpui_kit::MouseDownEvent, window, cx| {
                    graph.begin_drag(Some(id), event.position, window, cx)
                }),
            )
            .on_click(cx.listener(move |graph, _, _, cx| {
                graph.select(id, cx);
                cx.stop_propagation();
            }))
            .children(card_text(job, tokens, scale))
    }

    fn dependencies(&self, id: u64) -> String {
        let names: Vec<_> = self
            .layout
            .edges
            .iter()
            .filter(|(_, to)| *to == id)
            .filter_map(|(from, _)| self.jobs.iter().find(|job| job.id == *from))
            .map(|job| job.name.as_str())
            .collect();
        match self.layout.degraded.as_deref() {
            Some(reason) => format!("Dependencies unavailable: {reason}"),
            None if names.is_empty() => "No dependencies".into(),
            None => format!("Needs {}", names.join(", ")),
        }
    }
}

fn zoom_button(zoom_in: bool, cx: &Context<FlowGraph>) -> impl IntoElement {
    let (id, icon, label, factor) = if zoom_in {
        ("graph-zoom-in", IconName::Plus, "Zoom in", ZOOM_STEP)
    } else {
        ("graph-zoom-out", IconName::Minus, "Zoom out", ZOOM_OUT)
    };
    action_button(id, Intent::Quiet, Some(icon), label)
        .on_click(cx.listener(move |graph, _, _, cx| graph.zoom_center(factor, cx)))
}

fn card_text(job: &GraphJob, tokens: &Tokens, scale: f32) -> [Div; 3] {
    let (glyph, color) = status(&job.status, tokens);
    let label = job
        .status
        .strip_prefix("completed:")
        .unwrap_or(&job.status)
        .replace('_', " ");
    [
        div()
            .text_color(tokens.fg_strong)
            .truncate()
            .child(job.name.clone()),
        div()
            .text_color(color)
            .truncate()
            .child(format!("{glyph} {label} · {}", job.duration)),
        div()
            .text_size(tokens.caption_size() * scale)
            .text_color(tokens.muted)
            .truncate()
            .child(job.steps.clone()),
    ]
}

fn status(status: &str, tokens: &Tokens) -> (&'static str, gpui_kit::Hsla) {
    match status {
        "completed:success" => ("✓", tokens.green),
        "completed:failure" | "completed:timed_out" => ("✕", tokens.red),
        "in_progress" => ("◉", tokens.blue),
        "waiting" => ("◷", tokens.orange),
        "queued" => ("○", tokens.yellow),
        "completed:cancelled" => ("⊘", tokens.muted),
        _ => ("−", tokens.muted),
    }
}
