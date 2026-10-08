use crate::tree::{Node, Tree, NONE};
use jwalk::WalkDir;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn scan(root: &Path) -> io::Result<Tree> {
    let mut nodes: Vec<Node> = Vec::with_capacity(4096);
    let mut names = String::with_capacity(128 * 1024);

    let root_name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let start = names.len() as u32;
    names.push_str(&root_name);
    let root_len = root_name.len() as u32;

    nodes.push(Node {
        name_start: start,
        name_len: root_len,
        parent: NONE,
        first_child: NONE,
        next_sibling: NONE,
        size: 0,
        modified: 0,
        accessed: 0,
        is_dir: true,
    });

    let mut stack: Vec<(usize, u32)> = vec![(0, 0)];

    for entry in WalkDir::new(root).into_iter() {
        let entry = entry?;
        let depth = entry.depth();
        if depth == 0 {
            continue;
        }

        while let Some(&(d, _)) = stack.last() {
            if d >= depth {
                stack.pop();
            } else {
                break;
            }
        }
        let parent = stack.last().map(|&(_, id)| id).unwrap_or(0);

        let name = entry.file_name().to_string_lossy();
        let name_start = names.len() as u32;
        names.push_str(&name);
        let name_len = name.len() as u32;

        let md = entry.metadata().ok();
        let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = if is_dir {
            0
        } else {
            md.as_ref().map(|m| m.len()).unwrap_or(0)
        };
        let modified = to_unix(md.as_ref().and_then(|m| m.modified().ok()));
        let accessed = to_unix(md.as_ref().and_then(|m| m.accessed().ok()));

        let id = nodes.len() as u32;
        nodes.push(Node {
            name_start,
            name_len,
            parent,
            first_child: NONE,
            next_sibling: nodes[parent as usize].first_child,
            size,
            modified,
            accessed,
            is_dir,
        });
        nodes[parent as usize].first_child = id;

        if is_dir {
            stack.push((depth, id));
        }
    }

    for i in (1..nodes.len()).rev() {
        let p = nodes[i].parent as usize;
        let s = nodes[i].size;
        nodes[p].size += s;
    }

    Ok(Tree { nodes, names })
}

fn to_unix(t: Option<SystemTime>) -> u32 {
    t.and_then(|s| s.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs().min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}