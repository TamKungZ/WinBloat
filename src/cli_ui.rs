use crate::cli::Args;
use crate::format::{self, SEPARATOR_WIDTH};
use crate::scanner;
use crate::tree::{Node, SortKey, Tree};
use owo_colors::OwoColorize;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

const APP_NAME: &str = "WinBloat";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR_NAME: &str = "TamKungZ_";
const AUTHOR_EMAIL: &str = "dev@tamkungz.me";

pub fn run(args: &Args, root: &Path) -> io::Result<()> {
    format::init_colors();
    print_header();
    println!();
    println!("  {}  {}", "Scanning".bold(), root.display());

    let start = Instant::now();
    let tree = scanner::scan(root)?;
    let elapsed = start.elapsed();

    print_summary(&tree, elapsed);
    print_sections(&tree, args);
    println!();
    Ok(())
}

fn print_header() {
    println!(
        "{}  {}",
        APP_NAME.bold().cyan(),
        format!("v{}", APP_VERSION).dimmed()
    );
    println!(
        "{}  {}",
        "fast and memory-efficient disk analyzer".dimmed(),
        format!("by {} <{}>", AUTHOR_NAME, AUTHOR_EMAIL).dimmed()
    );
}

fn print_summary(tree: &Tree, elapsed: Duration) {
    let node_bytes = tree.nodes.len() * std::mem::size_of::<Node>();
    let name_bytes = tree.names.len();
    let total = node_bytes + name_bytes;

    println!();
    println!("  {:>12}  {}", "Items:".dimmed(), tree.nodes.len());
    println!(
        "  {:>12}  {}",
        "Time:".dimmed(),
        format!("{:.2?}", elapsed).bold()
    );
    println!(
        "  {:>12}  {}",
        "Memory:".dimmed(),
        format!(
            "{} nodes + {} names = {}",
            format::human_size(node_bytes as u64).dimmed(),
            format::human_size(name_bytes as u64).dimmed(),
            format::human_size(total as u64).bold(),
        )
    );
    println!(
        "  {:>12}  {}",
        "Total size:".dimmed(),
        format::human_size(tree.nodes[0].size).bold()
    );
}

fn print_separator(label: &str) {
    let text = format!("{} ", label);
    let bar_len = SEPARATOR_WIDTH.saturating_sub(label.len() + 1);
    println!();
    println!("{}{}", text.bold(), "─".repeat(bar_len).dimmed());
}

fn print_sections(tree: &Tree, args: &Args) {
    if !args.no_top {
        print_separator(&format!("Top {} largest files", args.top));
        print_top_files(tree, args.top, args.bar_width);
    }
    if !args.no_recent {
        print_separator(&format!("Top {} recently accessed files", args.recent));
        print_recent_files(tree, args.recent);
    }
    if !args.no_tree {
        print_separator(&format!("Directory tree (depth {})", args.depth));
        print_tree(tree, args.depth);
    }
}

fn collect_top_files(tree: &Tree, n: usize) -> Vec<u32> {
    let mut files: Vec<u32> = (1..tree.nodes.len() as u32)
        .filter(|&i| !tree.nodes[i as usize].is_dir)
        .collect();
    files.sort_unstable_by_key(|&i| std::cmp::Reverse(tree.nodes[i as usize].size));
    files.truncate(n);
    files
}

fn collect_recent_files(tree: &Tree, n: usize) -> Vec<u32> {
    let mut files: Vec<u32> = (1..tree.nodes.len() as u32)
        .filter(|&i| !tree.nodes[i as usize].is_dir && tree.nodes[i as usize].accessed > 0)
        .collect();
    files.sort_unstable_by_key(|&i| std::cmp::Reverse(tree.nodes[i as usize].accessed));
    files.truncate(n);
    files
}

fn print_top_files(tree: &Tree, top: usize, bar_width: usize) {
    let files = collect_top_files(tree, top);
    if files.is_empty() {
        println!("  (no files)");
        return;
    }
    let max = tree.nodes[files[0] as usize].size.max(1);

    for id in files {
        let node = &tree.nodes[id as usize];
        let path = tree.full_path(id);
        print!("  ");
        format::print_size_right(node.size, 10);
        print!("  ");
        let fraction = node.size as f64 / max as f64;
        print!("{}", format::build_bar(fraction, bar_width).dimmed());
        print!("  ");
        println!("{}", path);
    }
}

fn print_recent_files(tree: &Tree, n: usize) {
    let files = collect_recent_files(tree, n);
    if files.is_empty() {
        println!("  (no access timestamps available)");
        return;
    }
    for id in files {
        let node = &tree.nodes[id as usize];
        let path = tree.full_path(id);
        print!("  ");
        format::print_size_right(node.size, 10);
        print!("  ");
        println!(
            "{}  {}",
            format::format_timestamp(node.accessed).dimmed(),
            path
        );
    }
}

fn print_tree(tree: &Tree, max_depth: usize) {
    print_tree_node(tree, 0, "", true, max_depth, 0);
}

fn print_tree_node(
    tree: &Tree,
    id: u32,
    prefix: &str,
    is_last: bool,
    max_depth: usize,
    depth: usize,
) {
    let node = &tree.nodes[id as usize];

    let branch: String = if depth == 0 {
        String::new()
    } else if is_last {
        format!("{}└── ", prefix)
    } else {
        format!("{}├── ", prefix)
    };

    print!("{}", branch.dimmed());
    format::print_size_right(node.size, 10);
    print!("  ");

    if node.is_dir {
        println!("{}", tree.name(id).bold());
    } else {
        println!("{}", tree.name(id));
    }

    if !node.is_dir || depth >= max_depth {
        return;
    }

    let kids = tree.sorted_children(id, SortKey::Size);
    let n = kids.len();
    let next_prefix: String = if depth == 0 {
        String::new()
    } else if is_last {
        format!("{}    ", prefix)
    } else {
        format!("{}│   ", prefix)
    };

    for (i, &c) in kids.iter().enumerate() {
        let last = i + 1 == n;
        print_tree_node(tree, c, &next_prefix, last, max_depth, depth + 1);
    }
}