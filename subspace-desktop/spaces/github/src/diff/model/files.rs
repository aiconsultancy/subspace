//! Files as displayed by the diff viewer, independently of their wire source.

use subspace_framework::client_github::model::image_type;

use super::{Patch, parse_patch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Removed,
    Modified,
    Renamed,
    Copied,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub patch: Option<String>,
}

pub fn placeholder_reason(file: &DiffFile) -> Option<&'static str> {
    if file.patch.is_some() {
        return None;
    }
    let changes = file.additions + file.deletions;
    Some(if file.status == FileStatus::Renamed && changes == 0 {
        "Renamed without changes"
    } else if changes == 0 {
        "Binary file, or no text changes"
    } else {
        "Diff too large to show here"
    })
}

pub fn tree_status(status: FileStatus) -> Option<TreeStatus> {
    Some(match status {
        FileStatus::Added | FileStatus::Copied => TreeStatus::Added,
        FileStatus::Removed => TreeStatus::Deleted,
        FileStatus::Renamed => TreeStatus::Renamed,
        FileStatus::Unchanged => return None,
        FileStatus::Modified => TreeStatus::Modified,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
}

fn is_image(file: &DiffFile) -> bool {
    image_type(&file.path).is_some()
}

#[derive(Debug, Clone)]
pub struct FileView {
    pub file: DiffFile,
    pub patch: Patch,
    pub reason: Option<&'static str>,
    pub next_page: Option<String>,
    pub binary: bool,
    pub binary_size: Option<u64>,
    pub tags: &'static [&'static str],
}

impl FileView {
    pub fn new(file: DiffFile) -> Self {
        let patch = file.patch.as_deref().map(parse_patch).unwrap_or_default();
        let reason = placeholder_reason(&file);
        Self {
            file,
            patch,
            reason,
            next_page: None,
            binary: false,
            binary_size: None,
            tags: &[],
        }
    }

    pub fn image_only(&self) -> bool {
        self.file.patch.is_none() && is_image(&self.file)
    }

    /// Only a displayed code row can be cut; a partial hunk header or
    /// no-newline marker is not the preceding row's text.
    pub fn partial_line(&self) -> Option<usize> {
        self.next_page.as_ref()?;
        let patch = self.file.patch.as_deref()?;
        let tail = patch.rsplit('\n').next()?;
        if matches!(tail.as_bytes().first(), Some(b'+' | b'-' | b' ')) {
            self.patch.lines.len().checked_sub(1)
        } else {
            None
        }
    }
}
