mod analysis;
mod cli;
mod cli_ui;
mod format;
mod gui;
mod scanner;
mod tree;
mod tui;

use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let args = cli::parse();
    let root = cli::resolve_root(&args.path);

    match args.mode {
        cli::Mode::List => cli_ui::run(&args, &root),
        cli::Mode::Tui => tui::run(&args, &root),
        cli::Mode::Gui => gui::run(&root),
    }
}
