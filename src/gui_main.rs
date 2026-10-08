#![cfg_attr(windows, windows_subsystem = "windows")]

#[allow(dead_code)]
mod analysis;
#[allow(dead_code)]
mod format;
mod gui;
mod scanner;
mod tree;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "WinBloat", version, about = "Read-only disk usage analyzer")]
struct Args {
    #[arg(default_value = ".", help = "Directory to scan")]
    path: PathBuf,
}

fn main() {
    let args = Args::parse();
    let root = args.path.canonicalize().unwrap_or(args.path);
    if let Err(error) = gui::run(&root) {
        show_error(&error.to_string());
    }
}

#[cfg(windows)]
fn show_error(message: &str) {
    let title: Vec<u16> = "WinBloat".encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            0x0000_0010,
        );
    }
}

#[cfg(not(windows))]
fn show_error(message: &str) {
    eprintln!("error: {message}");
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(
        window: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        flags: u32,
    ) -> i32;
}
