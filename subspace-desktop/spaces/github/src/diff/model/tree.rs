use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub depth: usize,
    pub label: String,
    pub file: Option<usize>,
}

fn row(depth: usize, label: String, file: Option<usize>) -> TreeRow {
    TreeRow { depth, label, file }
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: Vec<(String, usize)>,
}

fn insert_path(root: &mut Dir, path: &str, index: usize) {
    let mut parts: Vec<&str> = path.split('/').collect();
    let name = parts.pop().unwrap_or_default();
    let mut dir = root;
    for part in parts {
        dir = dir.dirs.entry(part.to_string()).or_default();
    }
    dir.files.push((name.to_string(), index));
}

pub fn tree_rows(paths: &[&str], query: &str) -> Vec<TreeRow> {
    let query = query.trim().to_lowercase();
    let mut root = Dir::default();
    paths
        .iter()
        .enumerate()
        .filter(|(_, path)| query.is_empty() || path.to_lowercase().contains(&query))
        .for_each(|(index, path)| insert_path(&mut root, path, index));
    let mut rows = Vec::new();
    append_directory(&root, 0, &mut rows);
    rows
}

fn append_directory(dir: &Dir, depth: usize, rows: &mut Vec<TreeRow>) {
    dir.dirs
        .iter()
        .for_each(|(name, child)| append_subdirectory(name, child, depth, rows));
    rows.extend(sorted_files(dir, depth));
}

fn append_subdirectory(name: &str, child: &Dir, depth: usize, rows: &mut Vec<TreeRow>) {
    let (label, leaf) = joined_directory(name, child);
    rows.push(row(depth, label, None));
    append_directory(leaf, depth + 1, rows);
}

fn joined_directory<'a>(name: &str, mut dir: &'a Dir) -> (String, &'a Dir) {
    let mut label = name.to_string();
    while dir.files.is_empty() && dir.dirs.len() == 1 {
        let Some((next_name, next)) = dir.dirs.iter().next() else {
            break;
        };
        label = format!("{label}/{next_name}");
        dir = next;
    }
    (label, dir)
}

fn sorted_files(dir: &Dir, depth: usize) -> Vec<TreeRow> {
    let mut files = dir.files.clone();
    files.sort();
    files
        .into_iter()
        .map(|(label, file)| row(depth, label, Some(file)))
        .collect()
}

pub fn tree_order(paths: &[&str]) -> Vec<usize> {
    tree_rows(paths, "")
        .into_iter()
        .filter_map(|row| row.file)
        .collect()
}
