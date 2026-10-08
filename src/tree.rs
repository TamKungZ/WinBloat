use std::path::MAIN_SEPARATOR_STR;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Copy)]
pub struct Node {
    pub name_start: u32,
    pub name_len: u32,
    pub parent: u32,
    pub first_child: u32,
    pub next_sibling: u32,
    pub size: u64,
    pub modified: u32,
    pub accessed: u32,
    pub is_dir: bool,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub names: String,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum SortKey {
    Size,
    Name,
    Recent,
    Modified,
}

impl Tree {
    #[inline]
    pub fn name(&self, id: u32) -> &str {
        let n = &self.nodes[id as usize];
        let start = n.name_start as usize;
        &self.names[start..start + n.name_len as usize]
    }

    #[inline]
    pub fn children(&self, id: u32) -> ChildIter<'_> {
        ChildIter {
            tree: self,
            next: self.nodes[id as usize].first_child,
        }
    }

    pub fn full_path(&self, mut id: u32) -> String {
        let mut parts: Vec<&str> = Vec::new();
        while id != NONE {
            parts.push(self.name(id));
            id = self.nodes[id as usize].parent;
        }
        parts.reverse();
        parts.join(MAIN_SEPARATOR_STR)
    }

    pub fn child_count(&self, id: u32) -> usize {
        self.children(id).count()
    }

    pub fn sorted_children(&self, id: u32, sort: SortKey) -> Vec<u32> {
        let mut kids: Vec<u32> = self.children(id).collect();
        match sort {
            SortKey::Size => {
                kids.sort_unstable_by_key(|&c| std::cmp::Reverse(self.nodes[c as usize].size))
            }
            SortKey::Name => kids.sort_unstable_by_key(|&c| self.name(c).to_lowercase()),
            SortKey::Recent => {
                kids.sort_unstable_by_key(|&c| std::cmp::Reverse(self.nodes[c as usize].accessed))
            }
            SortKey::Modified => {
                kids.sort_unstable_by_key(|&c| std::cmp::Reverse(self.nodes[c as usize].modified))
            }
        }
        kids
    }
}

pub struct ChildIter<'a> {
    tree: &'a Tree,
    next: u32,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<u32> {
        if self.next == NONE {
            return None;
        }
        let id = self.next;
        self.next = self.tree.nodes[id as usize].next_sibling;
        Some(id)
    }
}