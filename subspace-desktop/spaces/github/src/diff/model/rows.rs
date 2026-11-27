use std::collections::HashSet;

use super::parse::{DiffLine, Gap, LineKind, gaps};
use super::{FileStatus, FileView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Unified,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRow {
    File(usize),
    Note(usize),
    More(usize),
    Gap {
        file: usize,
        gap: usize,
    },
    Line {
        file: usize,
        line: usize,
    },
    Split {
        file: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    Context {
        file: usize,
        old: u32,
        new: u32,
    },
}

pub struct RowOptions<'a> {
    pub layout: Layout,
    pub collapsed: &'a HashSet<usize>,
    pub expanded: &'a HashSet<(usize, usize)>,
    pub lengths: &'a dyn Fn(usize) -> Option<u32>,
}

pub fn build_rows(files: &[FileView], options: &RowOptions<'_>) -> (Vec<DiffRow>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut starts = Vec::with_capacity(files.len());
    for (index, view) in files.iter().enumerate() {
        starts.push(rows.len());
        rows.push(DiffRow::File(index));
        if options.collapsed.contains(&index) {
            continue;
        }
        if view.reason.is_some() {
            rows.push(DiffRow::Note(index));
        } else {
            project_file(&mut rows, index, view, options);
        }
        if view.next_page.is_some() {
            rows.push(DiffRow::More(index));
        }
    }
    (rows, starts)
}

fn project_file(rows: &mut Vec<DiffRow>, file: usize, view: &FileView, options: &RowOptions<'_>) {
    let file_gaps = gaps(&view.patch.hunks, (options.lengths)(file));
    let can_expand = !matches!(view.file.status, FileStatus::Added | FileStatus::Removed);
    for (hunk_index, hunk) in view.patch.hunks.iter().enumerate() {
        rows.push(DiffRow::Gap {
            file,
            gap: hunk_index,
        });
        let gap = file_gaps[hunk_index];
        if options.expanded.contains(&(file, hunk_index)) {
            push_context(rows, file, gap);
        }
        push_hunk(
            rows,
            file,
            &view.patch.lines[hunk.lines.clone()],
            hunk.lines.start,
            options.layout,
        );
    }
    let tail = file_gaps[view.patch.hunks.len()];
    if can_expand && !view.patch.hunks.is_empty() && tail.len != Some(0) {
        if options.expanded.contains(&(file, view.patch.hunks.len())) {
            push_context(rows, file, tail);
        } else {
            rows.push(DiffRow::Gap {
                file,
                gap: view.patch.hunks.len(),
            });
        }
    }
}

fn push_context(rows: &mut Vec<DiffRow>, file: usize, gap: Gap) {
    let len = gap.len.unwrap_or(0);
    rows.extend((0..len).map(|i| DiffRow::Context {
        file,
        old: gap.old_start + i,
        new: gap.new_start + i,
    }));
}

// qual:allow(dry, boilerplate) reason: "Context and paired changes construct the same enum variant with different line positions; Rust struct-update syntax cannot update enum variants."
fn push_hunk(
    rows: &mut Vec<DiffRow>,
    file: usize,
    lines: &[DiffLine],
    offset: usize,
    layout: Layout,
) {
    if layout == Layout::Unified {
        rows.extend((0..lines.len()).map(|i| DiffRow::Line {
            file,
            line: offset + i,
        }));
        return;
    }
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind == LineKind::Context {
            rows.push(DiffRow::Split {
                file,
                left: Some(offset + index),
                right: Some(offset + index),
            });
            index += 1;
            continue;
        }
        let dels = index;
        while index < lines.len() && lines[index].kind == LineKind::Del {
            index += 1;
        }
        let adds = index;
        while index < lines.len() && lines[index].kind == LineKind::Add {
            index += 1;
        }
        let (del_count, add_count) = (adds - dels, index - adds);
        for i in 0..del_count.max(add_count) {
            rows.push(DiffRow::Split {
                file,
                left: (i < del_count).then_some(offset + dels + i),
                right: (i < add_count).then_some(offset + adds + i),
            });
        }
    }
}

pub fn file_at(starts: &[usize], row: usize) -> usize {
    starts
        .partition_point(|&start| start <= row)
        .saturating_sub(1)
}
