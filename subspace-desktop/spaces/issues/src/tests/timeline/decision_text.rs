use crate::tests::render;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{VisualTestContext, Window};

pub(super) fn assert_decision_text_has_no_uuid(cx: &mut VisualTestContext) {
    cx.update(|window, _| {
        assert_resolved_decision(window, "Use cursor paging", "Accepted");
        assert_eq!(
            window.find("timeline-event:missing-decision-event").label(),
            Some("Decision logged, details unavailable")
        );
        assert!(
            window
                .find("timeline-event:unknown-decision-event")
                .label()
                .unwrap()
                .contains("Unknown status")
        );
        assert_all_labels(window);
    });
    for (event, expected) in [
        (
            "unknown-decision-event",
            "Decision logged · Future decision",
        ),
        ("decision-event", "Decision logged · Use cursor paging"),
        (
            "missing-decision-event",
            "Decision logged · Earlier decision unavailable",
        ),
    ] {
        let title = copy(cx, event, "title");
        assert_eq!(title, expected);
        assert_no_uuid(&title);
        let byline = copy(cx, event, "byline");
        assert!(byline.contains("Ada Fixture"));
        assert_no_uuid(&byline);
    }
}

pub(super) fn assert_resolved_decision(window: &Window, title: &str, status: &str) {
    let row = "timeline-event:decision-event";
    let heading = format!("Decision logged · {title}");
    assert!(
        window
            .find(row)
            .label()
            .unwrap()
            .starts_with(&format!("{heading}, {status}, Ada Fixture, "))
    );
    let action = window.find("issue-activity-decision:decision-event");
    let label = format!("Open decision {title}, {status}, logged by Ada Fixture");
    assert_eq!(action.label(), Some(label.as_str()));
    assert_no_uuid(action.label().unwrap());
    let texts = gpui_kit::base::test_support::texts(window);
    for expected in [heading.as_str(), status] {
        assert!(
            texts.iter().any(|text| text.text() == expected
                && text.path().iter().any(|id| id.to_string() == row)),
            "missing painted text {expected:?} in {row}"
        );
    }
}

fn assert_all_labels(window: &Window) {
    for snapshot in gpui_kit::base::test_support::snapshots(window) {
        if timeline_path(snapshot.path()) {
            for value in [snapshot.label(), snapshot.description(), snapshot.value()]
                .into_iter()
                .flatten()
            {
                assert_no_uuid(value);
            }
        }
    }
    let texts = gpui_kit::base::test_support::texts(window);
    let timeline_texts: Vec<_> = texts
        .iter()
        .filter(|text| timeline_path(text.path()))
        .collect();
    for status in ["Accepted", "Unknown status"] {
        assert!(
            timeline_texts.iter().any(|text| text.text() == status),
            "missing painted status {status}"
        );
    }
    for text in timeline_texts {
        assert_no_uuid(text.text());
    }
}

fn timeline_path(path: &[gpui_kit::ElementId]) -> bool {
    path.iter()
        .any(|id| format!("{id:?}").contains("timeline-event:"))
}

fn copy(cx: &mut VisualTestContext, event: &str, part: &str) -> String {
    cx.update(|window, cx| {
        window.right_click(
            format!("plain-menu-Name(\"timeline-decision-{part}:{event}\")"),
            cx,
        )
    });
    render(cx);
    cx.update(|window, cx| window.within("popup-menu").click(0usize, cx));
    render(cx);
    cx.read_from_clipboard().unwrap().text().unwrap()
}

fn assert_no_uuid(text: &str) {
    let uuid =
        regex::Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
            .unwrap();
    assert!(!uuid.is_match(text), "UUID exposed in {text:?}");
}
