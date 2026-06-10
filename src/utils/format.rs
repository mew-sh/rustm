//! Formatting utilities

use colored::*;

/// Format an error message
pub fn format_error(msg: &str) -> String {
    format!("{} {}", "error:".red().bold(), msg)
}

/// Format a warning message  
#[allow(dead_code)]
pub fn format_warning(msg: &str) -> String {
    format!("{} {}", "warning:".yellow().bold(), msg)
}

/// Format a success message
#[allow(dead_code)]
pub fn format_success(msg: &str) -> String {
    format!("{} {}", "✅", msg.green().bold())
}

/// Format a header
#[allow(dead_code)]
pub fn format_header(msg: &str) -> String {
    format!("{} {}", "⚡", msg.bold().bright_cyan())
}

/// Format duration nicely
#[allow(dead_code)]
pub fn format_duration(ms: u128) -> String {
    if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1000 {
        format!("{:.2}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

/// Format a table row
#[allow(dead_code)]
pub fn table_row(columns: &[&str], widths: &[usize]) -> String {
    columns
        .iter()
        .zip(widths.iter())
        .map(|(col, &width)| format!("{:<width$}", col, width = width))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Print a separator line
#[allow(dead_code)]
pub fn separator(width: usize) -> String {
    "─".repeat(width)
}
