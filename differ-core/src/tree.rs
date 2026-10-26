//! The sidebar tree: changed paths grouped by their directories.
//!
//! Only files are selectable; directory rows are labels. The caller keeps a
//! plain file index (`selected`) and `file_row` maps it to the row it occupies,
//! so movement stays `selected ± 1` and never lands on a directory.

use crate::git::{FileEntry, Status};

/// One line in the sidebar.
pub enum Row {
    Dir {
        name: String,
        depth: usize,
    },
    File {
        name: String,
        depth: usize,
        status: Status,
    },
}

/// Order entries so that, at every level, subdirectories come before files;
/// within each group entries are alphabetical by name. This is the order
/// `build` expects (and the order movement relies on).
pub fn order(files: &mut [FileEntry]) {
    files.sort_by(|a, b| compare(&a.path, &b.path));
}

/// Compare two paths component by component: at the first difference a
/// directory (a component with more path after it) sorts before a file.
fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let ac: Vec<&str> = a.split('/').collect();
    let bc: Vec<&str> = b.split('/').collect();
    for (i, (ca, cb)) in ac.iter().zip(&bc).enumerate() {
        match (i + 1 < ac.len(), i + 1 < bc.len()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        match ca.cmp(cb) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    ac.len().cmp(&bc.len())
}

/// Build the sidebar rows from `files`, which must already be in `order` so
/// that consecutive entries share a directory prefix. Returns the rows plus,
/// for each file in `files` order, the row index it occupies.
pub fn build(files: &[FileEntry]) -> (Vec<Row>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut file_row = Vec::with_capacity(files.len());
    let mut prev: Vec<&str> = Vec::new();
    for entry in files {
        let parts: Vec<&str> = entry.path.split('/').collect();
        let (dirs, tail) = parts.split_at(parts.len() - 1);
        let name = tail[0];
        // Emit only the directory levels this path doesn't share with the last.
        let common = dirs.iter().zip(&prev).take_while(|(a, b)| a == b).count();
        for (depth, dir) in dirs.iter().enumerate().skip(common) {
            rows.push(Row::Dir {
                name: (*dir).to_string(),
                depth,
            });
        }
        file_row.push(rows.len());
        rows.push(Row::File {
            name: name.to_string(),
            depth: dirs.len(),
            status: entry.status,
        });
        prev = dirs.to_vec();
    }
    (rows, file_row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(path: &str) -> FileEntry {
        FileEntry::worktree(path, None, Some('M'))
    }

    #[test]
    fn order_lists_directories_before_files() {
        let mut files = vec![
            f("legacy.rs"),
            f("examples/arrow_added.rs"),
            f("notes.txt"),
            f("examples/arrow_removed.rs"),
            f("src/ast.rs"),
        ];
        order(&mut files);
        let paths: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "examples/arrow_added.rs",
                "examples/arrow_removed.rs",
                "src/ast.rs",
                "legacy.rs",
                "notes.txt",
            ]
        );
    }

    #[test]
    fn order_puts_subdirectories_before_files_at_every_level() {
        let mut files = vec![f("src/a.rs"), f("src/sub/c.rs")];
        order(&mut files);
        let paths: Vec<&str> = files.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["src/sub/c.rs", "src/a.rs"]);
    }

    #[test]
    fn nests_shared_directories_once() {
        let mut files = vec![f("src/a.rs"), f("src/b.rs"), f("src/sub/c.rs"), f("top.rs")];
        order(&mut files);
        let (rows, file_row) = build(&files);
        assert_eq!(file_row, vec![2, 3, 4, 5]);
        assert!(matches!(&rows[0], Row::Dir { name, depth } if name == "src" && *depth == 0));
        assert!(matches!(&rows[1], Row::Dir { name, depth } if name == "sub" && *depth == 1));
        assert!(matches!(&rows[2], Row::File { name, depth, .. } if name == "c.rs" && *depth == 2));
        assert!(matches!(&rows[3], Row::File { name, depth, .. } if name == "a.rs" && *depth == 1));
        assert!(matches!(&rows[4], Row::File { name, depth, .. } if name == "b.rs" && *depth == 1));
        assert!(
            matches!(&rows[5], Row::File { name, depth, .. } if name == "top.rs" && *depth == 0)
        );
    }

    #[test]
    fn every_file_row_points_at_a_file() {
        let mut files = vec![f("a/x.rs"), f("a/y/z.rs"), f("b.rs")];
        order(&mut files);
        let (rows, file_row) = build(&files);
        assert_eq!(file_row.len(), files.len());
        for &r in &file_row {
            assert!(matches!(rows[r], Row::File { .. }));
        }
    }

    #[test]
    fn file_rows_are_increasing() {
        // So `selected ± 1` always moves to the next/previous file in the tree.
        let mut files = vec![f("a/x.rs"), f("a/y/z.rs"), f("b.rs")];
        order(&mut files);
        let (_, file_row) = build(&files);
        assert!(file_row.windows(2).all(|w| w[0] < w[1]));
    }
}
