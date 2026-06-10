//! rustm - Blazing-fast Rust/Cargo build compiler & optimizer
//!
//! A smart Cargo wrapper that automatically applies performance optimizations
//! to make your Rust builds significantly faster.

mod commands;
mod core;
mod utils;

use clap::Parser;
use commands::cli::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(e) = commands::execute(cli) {
        eprintln!("{}", utils::format::format_error(&e.to_string()));
        std::process::exit(1);
    }
}
