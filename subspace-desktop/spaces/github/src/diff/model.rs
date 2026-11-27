//! Pure parsing and projection models for the GitHub diff view.

mod files;
pub mod parse;
pub mod rows;
pub mod tree;
pub mod viewed;

pub use files::*;
pub use parse::*;
pub use rows::*;
pub use tree::*;
pub use viewed::*;

#[cfg(test)]
mod tests {
    use super::{
        DiffFile, DiffRow, FileStatus, FileView, Layout, LineKind, RowOptions, TreeStatus,
        ViewedStore, build_rows, file_at, gaps, parse_patch, placeholder_reason, tree_order,
        tree_rows, tree_status, word_ranges,
    };
    use std::collections::HashSet;

    const PATCH: &str = "@@ -3,4 +3,5 @@ fn main() {\n     let a = 1;\n-    let b = 2;\n+    let b = 3;\n+    let c = 4;\n     done();\n\\ No newline at end of file\n@@ -20,2 +21,2 @@\n-x\n+y\n z";

    fn file(path: &str, status: FileStatus, patch: Option<&str>) -> DiffFile {
        DiffFile {
            path: path.into(),
            previous_path: None,
            status,
            additions: 1,
            deletions: 1,
            patch: patch.map(Into::into),
        }
    }

    #[test]
    fn patches_preserve_hunks_numbers_and_word_marks() {
        let patch = parse_patch(PATCH);
        assert_eq!(patch.hunks.len(), 2);
        assert_eq!(patch.hunks[0].header, "@@ -3,4 +3,5 @@ fn main() {");
        assert_eq!(
            (patch.hunks[1].old_start, patch.hunks[1].new_start),
            (20, 21)
        );
        let first = &patch.lines[patch.hunks[0].lines.clone()];
        let kinds: Vec<LineKind> = first.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Context,
                LineKind::Del,
                LineKind::Add,
                LineKind::Add,
                LineKind::Context
            ]
        );
        assert_eq!((first[1].old, first[1].new), (Some(4), None));
        assert_eq!((first[3].old, first[3].new), (None, Some(5)));
        assert_eq!((first[4].old, first[4].new), (Some(5), Some(6)));
        assert!(first[4].no_newline);
        assert_eq!(&first[1].text[first[1].words[0].clone()], "2;");
        assert_eq!(&first[2].text[first[2].words[0].clone()], "3;");
        assert!(first[3].words.is_empty());
    }

    #[test]
    fn word_ranges_keep_unicode_byte_spans_and_reject_unrelated_lines() {
        assert!(word_ranges("completely different", "nothing alike here at all").is_none());
        let (old, _) = word_ranges("keep one two keep", "keep three four keep").unwrap();
        assert_eq!((old.len(), old[0].clone()), (1, 5..12));
        let (old, new) = word_ranges("a b c", "a x c").unwrap();
        assert_eq!((old[0].clone(), new[0].clone()), (2..3, 2..3));
        let (old, new) = word_ranges("café one", "café two").unwrap();
        assert_eq!(
            (&"café one"[old[0].clone()], &"café two"[new[0].clone()]),
            ("one", "two")
        );
    }

    #[test]
    fn gaps_cover_between_hunks_tail_and_empty_added_side() {
        let patch = parse_patch(PATCH);
        let found = gaps(&patch.hunks, None);
        assert_eq!(found.len(), 3);
        assert_eq!((found[0].new_start, found[0].len), (1, Some(2)));
        assert_eq!(
            (found[1].old_start, found[1].new_start, found[1].len),
            (7, 8, Some(13))
        );
        assert_eq!(found[2].len, None);
        let known = gaps(&patch.hunks, Some(30));
        assert_eq!((known[2].new_start, known[2].len), (23, Some(8)));
        let added = parse_patch("@@ -0,0 +1,2 @@\n+a\n+b");
        assert_eq!(gaps(&added.hunks, Some(2))[0].len, Some(0));
    }

    #[test]
    fn rows_preserve_layout_collapse_expansion_and_file_starts() {
        let files = vec![
            FileView::new(file("src/a.rs", FileStatus::Modified, Some(PATCH))),
            FileView::new(file("img.png", FileStatus::Added, None)),
        ];
        let none = HashSet::new();
        let no_gaps = HashSet::new();
        let lengths = |_| None;
        let unified = RowOptions {
            layout: Layout::Unified,
            collapsed: &none,
            expanded: &no_gaps,
            lengths: &lengths,
        };
        let (rows, starts) = build_rows(&files, &unified);
        assert_eq!(rows.len(), 1 + 1 + 5 + 1 + 3 + 1 + 2);
        assert_eq!(starts, [0, 12]);
        assert_eq!(rows[13], DiffRow::Note(1));
        assert_eq!(file_at(&starts, 11), 0);
        assert_eq!(file_at(&starts, 12), 1);
        let split = RowOptions {
            layout: Layout::Split,
            ..unified
        };
        let (rows, _) = build_rows(&files, &split);
        assert_eq!(
            rows[3],
            DiffRow::Split {
                file: 0,
                left: Some(1),
                right: Some(2)
            }
        );
        assert_eq!(
            rows[4],
            DiffRow::Split {
                file: 0,
                left: None,
                right: Some(3)
            }
        );
        let expanded: HashSet<(usize, usize)> = [(0, 1)].into();
        let collapsed: HashSet<usize> = [1].into();
        let options = RowOptions {
            layout: Layout::Unified,
            collapsed: &collapsed,
            expanded: &expanded,
            lengths: &lengths,
        };
        let (rows, _) = build_rows(&files, &options);
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row, DiffRow::Context { .. }))
                .count(),
            13
        );
        assert_eq!(rows.last(), Some(&DiffRow::File(1)));
    }

    #[test]
    fn placeholders_explain_files_without_a_text_diff() {
        let mut renamed = file("b", FileStatus::Renamed, None);
        renamed.additions = 0;
        renamed.deletions = 0;
        assert_eq!(
            placeholder_reason(&renamed),
            Some("Renamed without changes")
        );
        let mut binary = file("b", FileStatus::Modified, None);
        binary.additions = 0;
        binary.deletions = 0;
        assert_eq!(
            placeholder_reason(&binary),
            Some("Binary file, or no text changes")
        );
        assert_eq!(
            placeholder_reason(&file("b", FileStatus::Modified, None)),
            Some("Diff too large to show here")
        );
        assert_eq!(
            placeholder_reason(&file("b", FileStatus::Modified, Some(""))),
            None
        );
    }

    #[test]
    fn file_statuses_map_to_tree_statuses() {
        assert_eq!(tree_status(FileStatus::Added), Some(TreeStatus::Added));
        assert_eq!(tree_status(FileStatus::Copied), Some(TreeStatus::Added));
        assert_eq!(tree_status(FileStatus::Removed), Some(TreeStatus::Deleted));
        assert_eq!(tree_status(FileStatus::Renamed), Some(TreeStatus::Renamed));
        assert_eq!(
            tree_status(FileStatus::Modified),
            Some(TreeStatus::Modified)
        );
        assert_eq!(tree_status(FileStatus::Unchanged), None);
    }

    #[test]
    fn tree_joins_single_child_directories_and_filters_by_query() {
        let paths = [
            "src/app/main.rs",
            "src/app/lib.rs",
            "README.md",
            "docs/a/b/c.md",
        ];
        let labels: Vec<(usize, String)> = tree_rows(&paths, "")
            .into_iter()
            .map(|row| (row.depth, row.label))
            .collect();
        assert_eq!(
            labels,
            [
                (0, "docs/a/b".into()),
                (1, "c.md".into()),
                (0, "src/app".into()),
                (1, "lib.rs".into()),
                (1, "main.rs".into()),
                (0, "README.md".into())
            ]
        );
        assert_eq!(tree_order(&paths), [3, 1, 0, 2]);
        let found = tree_rows(&paths, "MAIN");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].file, Some(0));
    }

    #[test]
    fn viewed_store_keeps_the_newest_fifty_pull_requests() {
        let mut store = ViewedStore::parse(Some("not json"));
        assert!(store.0.is_empty());
        for i in 0..55 {
            store = store.with(&format!("o/r#{i}@x"), vec!["a".into()]);
        }
        assert_eq!(store.0.len(), 50);
        assert_eq!(store.0[0].0, "o/r#5@x");
        store = store.with("o/r#5@x", vec!["a".into(), "b".into()]);
        assert_eq!(store.0.last().map(|(id, _)| id.as_str()), Some("o/r#5@x"));
        store = store.with("o/r#5@x", Vec::new());
        assert!(store.get("o/r#5@x").is_empty());
        assert_eq!(ViewedStore::parse(Some(&store.to_json())), store);
    }
}
