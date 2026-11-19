use super::{Canvas, MAX_CANVASES};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct CanvasList {
    pub items: Vec<Arc<Canvas>>,
    pub selected: usize,
    pub hidden: bool,
    pub gallery: bool,
    pub data: bool,
}

impl CanvasList {
    pub fn position(&self) -> String {
        format!(
            "{} of {}",
            self.selected.saturating_add(1),
            self.items.len()
        )
    }
    pub fn current(&self) -> Option<&Arc<Canvas>> {
        self.items.get(self.selected)
    }

    pub fn open(&mut self, canvas: Arc<Canvas>) -> Result<(), String> {
        if let Some(index) = self.items.iter().position(|item| item.id == canvas.id) {
            self.items[index] = canvas;
            self.selected = index;
        } else {
            if self.items.len() >= MAX_CANVASES {
                return Err("Close a canvas before opening another (maximum 16).".into());
            }
            self.items.push(canvas);
            self.selected = self.items.len() - 1;
        }
        self.hidden = false;
        self.data = false;
        Ok(())
    }

    pub fn close(&mut self, id: &str) {
        let Some(index) = self.items.iter().position(|item| item.id == id) else {
            return;
        };
        self.items.remove(index);
        if index < self.selected {
            self.selected = self.selected.saturating_sub(1);
        }
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.gallery &= !self.items.is_empty();
        self.data = false;
    }

    pub fn step(&mut self, direction: isize) {
        if self.items.is_empty() {
            return;
        }
        self.selected =
            (self.selected as isize + direction).rem_euclid(self.items.len() as isize) as usize;
        self.data = false;
    }
}

#[cfg(test)]
mod tests {
    use super::CanvasList;
    use crate::canvas::{Canvas, CanvasContent, CanvasKind, MAX_CANVASES};
    use std::sync::Arc;

    fn item(id: usize) -> Arc<Canvas> {
        Arc::new(Canvas {
            id: id.to_string(),
            kind: CanvasKind::Drawing,
            title: id.to_string(),
            caption: String::new(),
            opened_at: String::new(),
            source: String::new(),
            content: CanvasContent::Error("fixture".into()),
        })
    }

    #[test]
    fn selection_wraps_and_close_preserves_the_adjacent_item() {
        let mut list = CanvasList::default();
        for id in 0..3 {
            list.open(item(id)).unwrap();
        }
        list.step(1);
        assert_eq!(list.current().unwrap().id, "0");
        list.step(-1);
        list.close("1");
        assert_eq!(list.current().unwrap().id, "2");
        list.gallery = true;
        list.close("2");
        list.close("0");
        list.step(-1);
        assert!(list.current().is_none());
        assert!(!list.gallery);
    }

    #[test]
    fn open_reuses_ids_and_enforces_limit_without_changing_selection() {
        let mut list = CanvasList::default();
        for id in 0..MAX_CANVASES {
            list.open(item(id)).unwrap();
        }
        assert!(list.open(item(MAX_CANVASES)).is_err());
        list.hidden = true;
        list.open(item(0)).unwrap();
        assert_eq!(list.items.len(), MAX_CANVASES);
        assert_eq!(list.selected, 0);
        assert!(!list.hidden);
        list.close("0");
        list.open(item(MAX_CANVASES)).unwrap();
        assert_eq!(list.items.len(), MAX_CANVASES);
    }
}
