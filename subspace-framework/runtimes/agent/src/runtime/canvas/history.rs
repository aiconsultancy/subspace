use super::{ChatEvent, ChatId, Event, EventQueue, compile};
use crate::canvas::MAX_CANVAS_HISTORY;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) async fn remember(
    history: &[github_copilot_sdk::types::SessionEvent],
    chat: ChatId,
    events: &EventQueue,
) {
    let values = history
        .iter()
        .rev()
        .filter_map(|event| serde_json::to_value(event).ok());
    for snapshot in snapshots(values) {
        if let Ok(canvas) = compile(events, snapshot.0, snapshot.1, snapshot.2).await {
            events
                .push(Event::of(chat, ChatEvent::CanvasRemembered(canvas)))
                .await;
        }
    }
}

fn snapshots(events: impl Iterator<Item = Value>) -> Vec<(String, String, Value)> {
    let mut seen = HashSet::new();
    let mut snapshots = Vec::new();
    for data in events.flat_map(inputs) {
        let (Some(id), Some(kind), Some(input)) = (
            data["instanceId"].as_str(),
            data["canvasId"].as_str(),
            data.get("input"),
        ) else {
            continue;
        };
        if seen.insert(id.to_owned()) {
            snapshots.push((id.to_owned(), kind.to_owned(), input.clone()));
        }
        if snapshots.len() == MAX_CANVAS_HISTORY {
            break;
        }
    }
    snapshots.reverse();
    snapshots
}

fn inputs(event: Value) -> Vec<Value> {
    match event["type"].as_str() {
        Some("session.canvas.opened") => vec![event["data"].clone()],
        Some("assistant.message") => event["data"]["toolRequests"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .filter(|request| request["name"] == "open_canvas")
            .map(|request| request["arguments"].clone())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::snapshots;
    use serde_json::json;

    #[test]
    fn closed_canvases_keep_the_latest_sdk_input_for_reopening() {
        let history = [
            json!({"type":"session.canvas.opened","data":{"instanceId":"a","canvasId":"drawing","input":{"source":"old"}}}),
            json!({"type":"assistant.message","data":{"toolRequests":[{"name":"open_canvas","arguments":{"instanceId":"a","canvasId":"drawing","input":{"source":"new"}}}]}}),
            json!({"type":"session.canvas.opened","data":{"instanceId":"a","canvasId":"drawing"}}),
            json!({"type":"session.canvas.closed","data":{"instanceId":"a"}}),
        ];
        let restored = snapshots(history.into_iter().rev());
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].2["source"], "new");
    }
}
