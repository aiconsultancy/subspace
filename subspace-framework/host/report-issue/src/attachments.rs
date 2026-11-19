//! Bounded, in-memory image selection. IDs survive removal and replacement.

use std::path::{Path, PathBuf};

use crate::Attachment;

pub(super) const MAX_ATTACHMENTS: usize = 5;
pub(super) const IMAGE_LIMIT: &str =
    "You can add up to 5 images. Remove an image before choosing another.";

#[derive(Clone, Default)]
pub(super) struct Attachments {
    items: Vec<SelectedImage>,
    next_id: usize,
}

#[derive(Clone)]
pub(super) struct SelectedImage {
    pub id: usize,
    pub image: Attachment,
}

impl Attachments {
    pub fn items(&self) -> &[SelectedImage] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_full(&self) -> bool {
        self.len() == MAX_ATTACHMENTS
    }

    pub fn append(&mut self, image: Attachment) -> Result<(), String> {
        if self.is_full() {
            return Err(IMAGE_LIMIT.into());
        }
        self.items.push(SelectedImage {
            id: self.next_id,
            image,
        });
        self.next_id += 1;
        Ok(())
    }

    pub fn remove(&mut self, id: usize) -> bool {
        let before = self.len();
        self.items.retain(|image| image.id != id);
        before != self.len()
    }

    pub fn save_in(&self, directory: &Path) -> Result<(), String> {
        for (ix, image) in self.items.iter().enumerate() {
            let path = directory.join(format!("report-image-{}.png", ix + 1));
            if let Err(error) = image.image.save_new(&path) {
                return Err(format!(
                    "Saved {ix} of {} images. {error} Existing files were not replaced.",
                    self.len()
                ));
            }
        }
        Ok(())
    }
}

pub(super) struct ImageSelection {
    pub images: Vec<Attachment>,
    pub error: Option<String>,
}

/// Decode sequentially off the UI thread, retaining no more than the free slots.
/// Invalid files do not consume a slot or prevent later valid files from loading.
pub(super) fn read_selection(paths: Vec<PathBuf>, capacity: usize) -> ImageSelection {
    let mut selection = ImageSelection {
        images: Vec::new(),
        error: None,
    };
    for path in paths {
        if selection.images.len() == capacity {
            selection.error = Some(IMAGE_LIMIT.into());
            break;
        }
        match Attachment::read(&path) {
            Ok(image) => selection.images.push(image),
            Err(error) => selection.error = Some(error),
        }
    }
    selection
}
