//! Virtualised Space content boundary.
use super::state::key;
use gpui_kit::{App, ElementId, SharedString, Window};
use std::rc::Rc;
/// A loaded document in a virtualised Space, including rows outside the viewport.
#[derive(Clone)]
pub struct Document {
    pub id: ElementId,
    pub markdown: SharedString,
}
impl Document {
    pub fn key(&self) -> String {
        key(&self.id).to_string()
    }
    pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            markdown: source.into(),
        }
    }
}
/// A virtual content owner supplies its current scope, loaded rows and row reveal.
#[derive(Clone)]
pub struct Provider {
    pub virtual_content: Rc<dyn Fn(&App) -> bool>,
    pub snapshot: Rc<dyn Fn(&App) -> (SharedString, Vec<Document>)>,
    pub reveal: Rc<dyn Fn(&str, &mut Window, &mut App)>,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            virtual_content: Rc::new(|_| false),
            snapshot: Rc::new(|_| (SharedString::default(), Vec::new())),
            reveal: Rc::new(|_, _, _| {}),
        }
    }
}
