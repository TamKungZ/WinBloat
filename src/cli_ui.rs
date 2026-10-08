use crate::analysis::{self, FileTypeStats};
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
        "fast, memory-efficient, read-only disk analyzer".dimmed(),
        format!("by {} <{}>", AUTHOR_NAME, AUTHOR_EMAIL).dimmed()
    );
}

fn print_summary(tree: &Tree, elapsed: Duration) {
    let node_bytes = tree.nodes.len() * std::mem::size_of::<Node>();
    let name_bytes = tree.names.len();
    let total = node_bytes + name_bytes;

    println!();
    println!("  {:>12}  {}", "Items:".dimmed(), tree.nodes.len());
    let (files, dirs) = analysis::item_counts(tree);
    println!(
        "  {:>12}  {} files, {} directories",
        "Contents:".dimmed(),
        files,
        dirs
    );
    println!(
        "  {:>12}  {}",
        "Time:".dimmed(),
        format!("{:.2?}", elapsed).bold()
    );
    let memory = format!(
        "{} nodes + {} names = {}",
        format::human_size(node_bytes as u64).dimmed(),
        format::human_size(name_bytes as u64).dimmed(),
        format::human_size(total as u64).bold(),
    );
    println!("  {:>12}  {}", "Memory:".dimmed(), memory);
    println!(
        "  {:>12}  {}",
        "Total size:".dimmed(),
        format::human_size(tree.nodes[0].size).bold()
    );
    let items_per_second = tree.nodes.len() as f64 / elapsed.as_secs_f64().max(0.001);
    println!(
        "  {:>12}  {} items/sec",
        "Throughput:".dimmed(),
        format!("{items_per_second:.0}").bold()
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
    if !args.no_top_dirs {
        print_separator(&format!("Top {} largest directories", args.top_dirs));
        print_top_directories(tree, args.top_dirs, args.bar_width);
    }
    if !args.no_recent {
        print_separator(&format!("Top {} recently accessed files", args.recent));
        print_recent_files(tree, args.recent);
    }
    if !args.no_types {
        print_separator(&format!("Largest {} file types", args.types));
        print_file_types(tree, args.types, args.bar_width);
    }
    if !args.no_tree {
        print_separator(&format!("Directory tree (depth {})", args.depth));
        print_tree(tree, args.depth);
    }
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
    let files = analysis::largest_files(tree, top);
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

fn print_top_directories(tree: &Tree, top: usize, bar_width: usize) {
    let dirs = analysis::largest_directories(tree, top);
    if dirs.is_empty() {
        println!("  (no subdirectories)");
        return;
    }
    let max = tree.nodes[dirs[0] as usize].size.max(1);
    for id in dirs {
        let node = &tree.nodes[id as usize];
        let fraction = node.size as f64 / max as f64;
        println!(
            "  {:>10}  {}  {}",
            format::human_size(node.size),
            format::build_bar(fraction, bar_width).dimmed(),
            tree.full_path(id)
        );
    }
}

fn print_file_types(tree: &Tree, limit: usize, bar_width: usize) {
    let types = analysis::file_type_stats(tree);
    if types.is_empty() {
        println!("  (no files)");
        return;
    }
    let types = types.into_iter().take(limit).collect::<Vec<_>>();
    let max = types.first().map(|kind| kind.bytes).unwrap_or(1).max(1);
    for FileTypeStats {
        extension,
        bytes,
        files,
    } in types
    {
        let fraction = bytes as f64 / max as f64;
        let total_share = if tree.nodes[0].size == 0 {
            0.0
        } else {
            bytes as f64 / tree.nodes[0].size as f64 * 100.0
        };
        println!(
            "  {:>10}  {}  {:>6.2}%  {:>8} files  {}",
            format::human_size(bytes),
            format::build_bar(fraction, bar_width).dimmed(),
            total_share,
            files,
            extension
        );
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
