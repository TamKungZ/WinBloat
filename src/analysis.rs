use crate::tree::{Tree, NONE};
use std::cmp::Reverse;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct FileTypeStats {
    pub extension: String,
    pub bytes: u64,
    pub files: usize,
}

pub fn largest_files(tree: &Tree, limit: usize) -> Vec<u32> {
    largest_nodes(tree, limit, false)
}

pub fn largest_directories(tree: &Tree, limit: usize) -> Vec<u32> {
    largest_nodes(tree, limit, true)
}

fn largest_nodes(tree: &Tree, limit: usize, directories: bool) -> Vec<u32> {
    let mut ids: Vec<u32> = (1..tree.nodes.len() as u32)
        .filter(|&id| tree.nodes[id as usize].is_dir == directories)
        .collect();
    ids.sort_unstable_by(|&left, &right| {
        let left_node = &tree.nodes[left as usize];
        let right_node = &tree.nodes[right as usize];
        right_node
            .size
            .cmp(&left_node.size)
            .then_with(|| tree.name(left).cmp(tree.name(right)))
    });
    ids.truncate(limit);
    ids
}

pub fn file_type_stats(tree: &Tree) -> Vec<FileTypeStats> {
    let mut stats = std::collections::HashMap::<String, (u64, usize)>::new();
    for (id, node) in tree.nodes.iter().enumerate().skip(1) {
        if node.is_dir {
            continue;
        }
        let extension = Path::new(tree.name(id as u32))
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy().to_lowercase()))
            .unwrap_or_else(|| "(no extension)".to_string());
        let entry = stats.entry(extension).or_default();
        entry.0 += node.size;
        entry.1 += 1;
    }

    let mut types: Vec<FileTypeStats> = stats
        .into_iter()
        .map(|(extension, (bytes, files))| FileTypeStats {
            extension,
            bytes,
            files,
        })
        .collect();
    types.sort_unstable_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.extension.cmp(&right.extension))
    });
    types
}

pub fn item_counts(tree: &Tree) -> (usize, usize) {
    tree.nodes
        .iter()
        .skip(1)
        .fold((0, 0), |(files, dirs), node| {
            if node.is_dir {
                (files, dirs + 1)
            } else {
                (files + 1, dirs)
            }
        })
}

pub fn share_of_parent(tree: &Tree, id: u32) -> f64 {
    let node = &tree.nodes[id as usize];
    if node.parent == NONE {
        return 1.0;
    }
    let parent_size = tree.nodes[node.parent as usize].size;
    if parent_size == 0 {
        0.0
    } else {
        node.size as f64 / parent_size as f64
    }
}

pub fn share_of_root(tree: &Tree, id: u32) -> f64 {
    let root_size = tree.nodes[0].size;
    if root_size == 0 {
        0.0
    } else {
        tree.nodes[id as usize].size as f64 / root_size as f64
    }
}

pub fn largest_children(tree: &Tree, id: u32, limit: usize) -> Vec<u32> {
    let mut children: Vec<u32> = tree.children(id).collect();
    children.sort_unstable_by_key(|&child| Reverse(tree.nodes[child as usize].size));
    children.truncate(limit);
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Node;

    fn fixture() -> Tree {
        let names = "rootarchive.ZIPnotes.TXTreadme".to_string();
        Tree {
            nodes: vec![
                Node {
                    name_start: 0,
                    name_len: 4,
                    parent: NONE,
                    first_child: 1,
                    next_sibling: NONE,
                    size: 30,
                    modified: 0,
                    accessed: 0,
                    is_dir: true,
                },
                Node {
                    name_start: 4,
                    name_len: 11,
                    parent: 0,
                    first_child: NONE,
                    next_sibling: 2,
                    size: 20,
                    modified: 0,
                    accessed: 0,
                    is_dir: false,
                },
                Node {
                    name_start: 15,
                    name_len: 9,
                    parent: 0,
                    first_child: NONE,
                    next_sibling: 3,
                    size: 10,
                    modified: 0,
                    accessed: 0,
                    is_dir: false,
                },
                Node {
                    name_start: 24,
                    name_len: 6,
                    parent: 0,
                    first_child: NONE,
                    next_sibling: NONE,
                    size: 0,
                    modified: 0,
                    accessed: 0,
                    is_dir: false,
                },
            ],
            names,
        }
    }

    #[test]
    fn groups_extensions_case_insensitively_and_counts_files() {
        let stats = file_type_stats(&fixture());
        assert_eq!(
            stats,
            vec![
                FileTypeStats {
                    extension: ".zip".into(),
                    bytes: 20,
                    files: 1,
                },
                FileTypeStats {
                    extension: ".txt".into(),
                    bytes: 10,
                    files: 1,
                },
                FileTypeStats {
                    extension: "(no extension)".into(),
                    bytes: 0,
                    files: 1,
                },
            ]
        );
    }

    #[test]
    fn largest_file_and_directory_lists_exclude_root_and_respect_limits() {
        let tree = fixture();
        assert_eq!(largest_files(&tree, 1), vec![1]);
        assert!(largest_directories(&tree, 10).is_empty());
        assert_eq!(item_counts(&tree), (3, 0));
        assert_eq!(share_of_parent(&tree, 1), 20.0 / 30.0);
    }
}
