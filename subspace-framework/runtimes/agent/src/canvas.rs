//! Read-only native canvas input and per-conversation selection.
mod chart_input;
mod diagram;
mod input;
mod state;
mod svg;

pub use input::prepare;
pub(crate) use input::validate_id;
pub use state::CanvasList;
use std::sync::Arc;
use subspace_charts::ChartModel;

pub const MAX_CANVASES: usize = 16;
pub const MAX_CANVAS_HISTORY: usize = 64;
pub const MAX_INPUT_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasKind {
    Chart,
    Diagram,
    Drawing,
}

impl CanvasKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chart => "Chart",
            Self::Diagram => "Diagram",
            Self::Drawing => "Drawing",
        }
    }
}

#[derive(Clone, Debug)]
pub enum CanvasContent {
    Chart(Arc<ChartModel>),
    /// One rendering per appearance and the drawing's height over width.
    Svg {
        light: Arc<str>,
        dark: Arc<str>,
        aspect: f32,
    },
    Error(String),
}

#[derive(Clone, Debug)]
pub struct Canvas {
    pub id: String,
    pub kind: CanvasKind,
    pub title: String,
    pub caption: String,
    pub opened_at: String,
    pub source: String,
    pub content: CanvasContent,
}
