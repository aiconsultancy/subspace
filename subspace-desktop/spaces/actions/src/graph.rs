//! Interactive run-scoped job graph. Status refreshes do not move nodes.
mod interaction;
pub mod layout;
mod paint;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
mod view;
pub mod viewport;

use gpui_kit::{Context, EventEmitter, FocusHandle};
use layout::{Layout, Point};
use viewport::Viewport;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphJob {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub duration: String,
    pub steps: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobSelected(pub u64);

pub struct FlowGraph {
    jobs: Vec<GraphJob>,
    layout: Layout,
    viewport: Viewport,
    selected: Option<u64>,
    focus: FocusHandle,
    origin: Point,
    size: Point,
    drag: Option<Drag>,
    fit_pending: bool,
    source_edges: Vec<(u64, u64)>,
    source_degraded: Option<String>,
}

struct Drag {
    node: Option<u64>,
    position: Point,
}

impl EventEmitter<JobSelected> for FlowGraph {}

impl FlowGraph {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            jobs: Vec::new(),
            layout: Layout::default(),
            viewport: Viewport::default(),
            selected: None,
            focus: cx.focus_handle(),
            origin: Point::default(),
            size: Point::default(),
            drag: None,
            fit_pending: true,
            source_edges: Vec::new(),
            source_degraded: None,
        }
    }

    /// Edges are `(predecessor, dependent)`. Create a new entity for another run.
    pub fn set_jobs(
        &mut self,
        jobs: Vec<GraphJob>,
        edges: Vec<(u64, u64)>,
        degraded: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let changed = topology_changed(&self.jobs, &jobs)
            || self.source_edges != edges
            || self.source_degraded != degraded;
        self.jobs = jobs;
        if changed {
            self.replace_layout(edges, degraded);
        }
        cx.notify();
    }

    fn replace_layout(&mut self, edges: Vec<(u64, u64)>, degraded: Option<String>) {
        let ids: Vec<_> = self.jobs.iter().map(|job| job.id).collect();
        self.layout = layout::layout(&ids, &edges, degraded.clone());
        self.source_edges = edges;
        self.source_degraded = degraded;
        self.viewport.offsets.retain(|id, _| ids.contains(id));
        self.selected = self.selected.filter(|id| ids.contains(id));
    }

    fn select(&mut self, id: u64, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.emit(JobSelected(id));
        cx.notify();
    }

    fn fit(&mut self, cx: &mut Context<Self>) {
        self.viewport
            .fit(self.viewport.extent(&self.layout), self.size);
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.viewport = Viewport::default();
        self.fit(cx);
    }
}

fn topology_changed(before: &[GraphJob], after: &[GraphJob]) -> bool {
    before.len() != after.len()
        || before
            .iter()
            .any(|job| !after.iter().any(|next| next.id == job.id))
}
