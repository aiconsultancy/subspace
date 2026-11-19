use super::{FlowGraph, GraphJob, JobSelected, layout::Point};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext};
use std::{cell::RefCell, rc::Rc};

fn jobs() -> Vec<GraphJob> {
    (1..=4)
        .map(|id| GraphJob {
            id,
            name: format!("Job {id}"),
            status: "queued".into(),
            duration: "—".into(),
            steps: "Waiting".into(),
        })
        .collect()
}

#[gpui_kit::test]
fn clicking_a_node_emits_selection_and_fit_contains_nodes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (graph, cx) = cx.add_window_view(|_, cx| {
        let mut graph = FlowGraph::new(cx);
        graph.set_jobs(jobs(), vec![(1, 2), (2, 3), (3, 4)], None, cx);
        graph
    });
    let selected = Rc::new(RefCell::new(Vec::new()));
    let events = selected.clone();
    let subscription = cx.update(|_, cx| {
        cx.subscribe(&graph, move |_, event: &JobSelected, _| {
            events.borrow_mut().push(event.0)
        })
    });
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    cx.update(|window, cx| window.click(("graph-job", 1_u64), cx));
    assert_eq!(&*selected.borrow(), &[1]);
    graph.read_with(cx, |graph, _| {
        assert!(
            !graph.fit_pending,
            "the initial non-empty frame must fit without a click"
        );
        for node in &graph.layout.nodes {
            let rect = graph.viewport.rect(node);
            let start = graph.viewport.screen(rect.origin);
            let end = graph.viewport.screen(Point {
                x: rect.origin.x + rect.width,
                y: rect.origin.y + rect.height,
            });
            assert!(start.x >= 0. && start.y >= 0.);
            assert!(end.x <= graph.size.x && end.y <= graph.size.y);
        }
    });
    drop(subscription);
}

#[gpui_kit::test]
fn polling_preserves_drag_and_selection_without_relayout(cx: &mut TestAppContext) {
    let graph = cx.new(FlowGraph::new);
    graph.update(cx, |graph, cx| {
        graph.set_jobs(jobs(), vec![(1, 2)], None, cx);
        graph.viewport.drag(Some(1), Point { x: 50., y: 80. });
        graph.selected = Some(1);
        let before = graph.layout.clone();
        let viewport = graph.viewport.clone();
        let mut updated = jobs();
        updated.reverse();
        updated[0].status = "completed:success".into();
        graph.set_jobs(updated, vec![(1, 2)], None, cx);
        assert_eq!(graph.layout, before);
        assert_eq!(graph.viewport, viewport);
        assert_eq!(graph.selected, Some(1));
    });
}
