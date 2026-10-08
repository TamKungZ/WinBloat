use owo_colors::OwoColorize;
use std::io::IsTerminal;

pub const SEPARATOR_WIDTH: usize = 72;

pub fn init_colors() {
    let disabled = std::env::var_os("NO_COLOR").is_some();
    let is_tty = std::io::stdout().is_terminal();
    if disabled || !is_tty {
        owo_colors::unset_override();
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

pub fn size_tier(size: u64) -> u8 {
    const GB: u64 = 1024 * 1024 * 1024;
    const HUNDRED_MB: u64 = 100 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    if size >= GB {
        3
    } else if size >= HUNDRED_MB {
        2
    } else if size >= MB {
        1
    } else {
        0
    }
}

pub fn print_size_right(size: u64, width: usize) {
    let s = format!("{:>width$}", human_size(size), width = width);
    match size_tier(size) {
        3 => print!("{}", s.red().bold()),
        2 => print!("{}", s.yellow()),
        1 => print!("{}", s.green()),
        _ => print!("{}", s.cyan()),
    }
}

pub fn build_bar(fraction: f64, width: usize) -> String {
    let filled = ((fraction * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), " ".repeat(empty))
}

pub fn format_timestamp(secs: u32) -> String {
    if secs == 0 {
        return "unknown".to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs = secs as u64;
    if now <= secs {
        return "just now".to_string();
    }
    let diff = now - secs;
    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86_400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 86_400 * 30 {
        format!("{}d ago", diff / 86_400)
    } else if diff < 86_400 * 365 {
        format!("{}mo ago", diff / (86_400 * 30))
    } else {
        format!("{}y ago", diff / (86_400 * 365))
    }
}