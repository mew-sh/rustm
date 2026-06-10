//! System detection utilities

/// Get system information string
pub fn system_info() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    format!("OS: {} | Arch: {} | CPUs: {}", os, arch, cpu_count)
}

/// Detect native CPU target
#[allow(dead_code)]
pub fn native_target() -> String {
    format!("{}-unknown-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// Check if a command is available
#[allow(dead_code)]
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Get Rust toolchain info
pub fn rust_info() -> String {
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let cargo = std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    format!("{} | {}", rustc, cargo)
}
