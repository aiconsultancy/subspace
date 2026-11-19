//! Explicit GUI renderer. No MCP tools or host operations are registered here.
#[path = "canvas/history.rs"]
mod history;
use super::{ChatEvent, ChatId, Event, EventQueue};
use crate::canvas::{MAX_CANVASES, prepare};
use github_copilot_sdk::canvas::{CanvasDeclaration, CanvasError, CanvasHandler, CanvasResult};
use github_copilot_sdk::rpc::{
    CanvasProviderCloseRequest, CanvasProviderOpenRequest, CanvasProviderOpenResult,
};
pub(super) use history::remember;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
static COMPILE_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

pub(super) fn configure(
    config: super::SessionConfig,
    chat: ChatId,
    events: Arc<EventQueue>,
) -> super::SessionConfig {
    config
        .with_request_canvas_renderer(true)
        .with_canvases(declarations())
        .with_canvas_provider(
            github_copilot_sdk::types::CanvasProviderIdentity::new("subspace:native:v1")
                .with_name("Subspace"),
        )
        .with_canvas_handler(Arc::new(Host::new(chat, events)))
}

pub(super) struct Host {
    pub(super) chat: ChatId,
    pub(super) events: Arc<EventQueue>,
    pub(super) open: Mutex<HashSet<String>>,
}

#[async_trait::async_trait]
impl CanvasHandler for Host {
    async fn on_open(
        &self,
        request: CanvasProviderOpenRequest,
    ) -> CanvasResult<CanvasProviderOpenResult> {
        let id = request.instance_id;
        let newly_reserved = self.reserve(&id)?;
        let prepared = compile(
            &self.events,
            id.clone(),
            request.canvas_id,
            request.input.unwrap_or(Value::Null),
        )
        .await;
        let canvas = match prepared {
            Ok(canvas) => canvas,
            Err(error) => {
                if newly_reserved {
                    self.remove(&id);
                }
                return Err(CanvasError::new("invalid_canvas", error));
            }
        };
        let result = CanvasProviderOpenResult {
            title: Some(canvas.title.clone()),
            status: Some("Ready".into()),
            ..Default::default()
        };
        self.events
            .push(Event::of(self.chat, ChatEvent::CanvasOpened(canvas)))
            .await;
        Ok(result)
    }

    async fn on_close(&self, request: CanvasProviderCloseRequest) -> CanvasResult<()> {
        self.remove(&request.instance_id);
        self.events
            .push(Event::of(
                self.chat,
                ChatEvent::CanvasClosed(request.instance_id),
            ))
            .await;
        Ok(())
    }
}

impl Host {
    pub(super) fn new(chat: ChatId, events: Arc<EventQueue>) -> Self {
        Self {
            chat,
            events,
            open: Mutex::new(HashSet::new()),
        }
    }

    pub(super) fn restore_ids(&self, session: &github_copilot_sdk::session::Session) {
        for canvas in session.open_canvases().into_iter().take(MAX_CANVASES) {
            let _ = self.reserve(&canvas.instance_id);
        }
    }

    fn reserve(&self, id: &str) -> CanvasResult<bool> {
        crate::canvas::validate_id(id)
            .map_err(|error| CanvasError::new("invalid_canvas", error))?;
        let mut open = self
            .open
            .lock()
            .map_err(|_| CanvasError::new("unavailable", "Canvas state unavailable."))?;
        if !open.contains(id) && open.len() >= MAX_CANVASES {
            return Err(CanvasError::new(
                "limit",
                "Close a canvas first (maximum 16).",
            ));
        }
        Ok(open.insert(id.to_owned()))
    }

    fn remove(&self, id: &str) {
        if let Ok(mut open) = self.open.lock() {
            open.remove(id);
        }
    }
}

async fn compile(
    events: &EventQueue,
    id: String,
    kind: String,
    input: Value,
) -> Result<Arc<crate::canvas::Canvas>, String> {
    if events.has_secret(&input.to_string()) || events.has_secret(&id) {
        return Err("Canvas input contains credential-like content.".into());
    }
    let _permit = COMPILE_SLOTS
        .acquire()
        .await
        .map_err(|_| "Native renderer unavailable.".to_string())?;
    tokio::task::spawn_blocking(move || prepare(&id, &kind, input))
        .await
        .map_err(|_| "Native canvas renderer failed.".to_string())?
}

pub(super) fn declarations() -> Vec<CanvasDeclaration> {
    vec![
        declaration("chart", format!("Open a native chart to compare quantities, trends or proportions. Read actual data with board tools first; source is Flint ChartAssemblyInput with INLINE data.values rows (no named datasets in canvas input). Use scalar field encodings. Compile is embedded QuickJS. {}", subspace_charts::guide()), "object"),
        declaration("diagram", "Open a native Mermaid diagram for architecture, flowcharts or sequences. Use plain Mermaid source; flowchart, sequenceDiagram, classDiagram, stateDiagram-v2, erDiagram and gantt are supported subsets. No links or directives; maximum 200 lines and 16 KiB.".into(), "string"),
        declaration("drawing", "Open a native SVG drawing for spatial sketches not covered by charts or diagrams. Use basic SVG geometry and text with explicit width/height <=4096. No scripts, images, foreignObject, external links or resources. Use legible neutral colours.".into(), "string"),
    ]
}

fn declaration(kind: &str, guidance: String, source_type: &str) -> CanvasDeclaration {
    let mut declaration = CanvasDeclaration::new(kind, kind, guidance);
    declaration.input_schema = Some(
        json!({"type":"object","additionalProperties":false,"required":["title","source"],"properties":{
            "title":{"type":"string","maxLength":160},"caption":{"type":"string","maxLength":240},"source":{"type":source_type}
        }}),
    );
    declaration.actions = Some(Vec::new());
    declaration
}

pub(super) async fn close(
    session: &github_copilot_sdk::session::Session,
    id: String,
) -> Result<(), String> {
    session
        .rpc()
        .canvas()
        .close(github_copilot_sdk::rpc::CanvasCloseRequest { instance_id: id })
        .await
        .map(|_| ())
        .map_err(|_| "Copilot could not close this canvas.".into())
}

pub(super) async fn reopen(
    session: &github_copilot_sdk::session::Session,
    canvas: &crate::canvas::Canvas,
) -> Result<(), String> {
    let source = if canvas.kind == crate::canvas::CanvasKind::Chart {
        serde_json::from_str(&canvas.source)
            .map_err(|_| "Cannot read stored chart input.".to_string())?
    } else {
        Value::String(canvas.source.clone())
    };
    let request = github_copilot_sdk::rpc::CanvasOpenRequest {
        instance_id: canvas.id.clone(),
        canvas_id: canvas.kind.label().to_ascii_lowercase(),
        extension_id: Some("subspace:native:v1".into()),
        input: Some(json!({"title":canvas.title,"caption":canvas.caption,"source":source})),
    };
    session
        .rpc()
        .canvas()
        .open(request)
        .await
        .map(|_| ())
        .map_err(|_| "Copilot could not reopen this canvas.".into())
}

pub(super) async fn restore(
    session: &github_copilot_sdk::session::Session,
    chat: ChatId,
    events: &EventQueue,
) {
    for snapshot in session.open_canvases().into_iter().take(MAX_CANVASES) {
        let result = compile(
            events,
            snapshot.instance_id,
            snapshot.canvas_id,
            snapshot.input.unwrap_or(Value::Null),
        )
        .await;
        let event = match result {
            Ok(canvas) => ChatEvent::CanvasOpened(canvas),
            Err(error) => ChatEvent::Error(format!("Canvas restore: {error}")),
        };
        events.push(Event::of(chat, event)).await;
    }
}
