use clap::{Parser, ValueEnum};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "WinBloat",
    version,
    about = "Fast and memory-efficient disk analyzer",
    long_about = None,
)]
pub struct Args {
    #[arg(default_value = ".", help = "Directory to scan")]
    pub path: PathBuf,

    #[arg(short, long, value_enum, default_value_t = Mode::List, help = "Output mode")]
    pub mode: Mode,

    #[arg(short = 'd', long, default_value_t = 2, help = "Maximum tree depth")]
    pub depth: usize,

    #[arg(short = 'n', long, default_value_t = 20, help = "Number of largest files")]
    pub top: usize,

    #[arg(short = 'r', long, default_value_t = 10, help = "Number of recently accessed files")]
    pub recent: usize,

    #[arg(long, default_value_t = 30, help = "Bar chart width")]
    pub bar_width: usize,

    #[arg(long, help = "Skip largest files section")]
    pub no_top: bool,

    #[arg(long, help = "Skip recent files section")]
    pub no_recent: bool,

    #[arg(long, help = "Skip directory tree section")]
    pub no_tree: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    List,
    Tui,
}

pub fn parse() -> Args {
    Args::parse()
}

pub fn resolve_root(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        let s = canonical.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    canonical
}