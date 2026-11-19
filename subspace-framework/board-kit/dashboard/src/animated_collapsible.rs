//! A measured disclosure with a reversible, owner-controlled transition.
//! The owner retains `Disclosure` and invalidates virtual row measurements via
//! `on_resize`. Standard GPUI layout measures the unconstrained content.
mod frame;
mod state;
#[cfg(test)]
mod tests;
pub use state::Disclosure;

use gpui_kit::{AnyElement, App, ElementId, IntoElement, RenderOnce, Window};
use std::{rc::Rc, time::Duration};

pub const CLOSED_PROGRESS: f32 = 0.;
pub const OPEN_PROGRESS: f32 = 1.;
pub const DISCLOSURE_DURATION: Duration = Duration::from_millis(200);
pub type Resize = Rc<dyn Fn(&mut App)>;

/// IDs must include the owning message and issue. The owner retains measured
/// and sampled geometry, so direction and content changes keep their continuity.
#[derive(IntoElement)]
pub struct AnimatedCollapsible {
    id: ElementId,
    state: Disclosure,
    content: AnyElement,
    resize: Option<Resize>,
    preview: Option<f32>,
}
impl AnimatedCollapsible {
    pub fn new(id: impl Into<ElementId>, state: &Disclosure, content: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            content: content.into_any_element(),
            resize: None,
            preview: None,
        }
    }
    /// Freeze a presentation sample at a normalised progress value.
    pub fn preview(mut self, progress: f32) -> Self {
        self.preview = Some(progress.clamp(CLOSED_PROGRESS, OPEN_PROGRESS));
        self
    }
    fn retained_state(&self, window: &mut Window, cx: &mut App) -> Disclosure {
        self.preview.map_or_else(
            || self.state.clone(),
            |_| {
                window
                    .use_keyed_state((self.id.clone(), "preview-geometry"), cx, |_, _| {
                        Disclosure::default()
                    })
                    .read(cx)
                    .clone()
            },
        )
    }
    pub fn on_resize(mut self, resize: Resize) -> Self {
        self.resize = Some(resize);
        self
    }
}
impl RenderOnce for AnimatedCollapsible {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.retained_state(window, cx);
        frame::Frame::new(state, self.content, self.resize, self.preview)
    }
}
