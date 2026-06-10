//! Init command - initialize rustm configuration

use crate::commands::cli;
use crate::core::config::RustmConfig;
use colored::*;

pub fn execute(args: cli::InitArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory. No Cargo.toml found.".to_string())?;

    let config_path = project_dir.join("rustm.toml");

    if config_path.exists() && !args.force {
        println!("{} rustm.toml already exists. Use --force to overwrite.", "⚠️".to_string());
        return Ok(());
    }

    // Generate config based on preset
    let content = if let Some(ref preset) = args.preset {
        generate_preset_config(preset)?
    } else {
        RustmConfig::generate_default()
    };

    std::fs::write(&config_path, &content)
        .map_err(|e| format!("Failed to write rustm.toml: {}", e))?;

    println!();
    println!("{} rustm initialized!", "✨".to_string());
    println!("  Config written to: {}", config_path.display().to_string().cyan());
    println!();
    println!("  {} Available profiles:", "⚡".to_string());
    println!("    {} — Fast dev iteration", "dev-fast".cyan());
    println!("    {} — Fastest cargo check", "dev-check".cyan());
    println!("    {} — Fast release with thin LTO", "release-fast".cyan());
    println!("    {} — Maximum optimization with fat LTO", "release-max".cyan());
    println!("    {} — Minimal binary size", "size-opt".cyan());
    println!("    {} — Good compile/runtime tradeoff", "balanced".cyan());
    println!();
    println!("  {} Run {} to build with optimizations!", "💡".to_string(), "rustm build".green().bold());

    Ok(())
}

fn generate_preset_config(preset: &str) -> Result<String, String> {
    match preset {
        "fast" => Ok(generate_fast_preset()),
        "release" => Ok(generate_release_preset()),
        "minimal" => Ok(generate_minimal_preset()),
        _ => Err(format!("Unknown preset: '{}'. Available: fast, release, minimal", preset)),
    }
}

fn generate_fast_preset() -> String {
    r#"# rustm configuration (fast preset)
# Optimized for fastest development iteration

[build]
default_profile = "dev-fast"
incremental = true
strip = false
lto = "none"
rustflags = []

[cache]
sccache = true
max_size_gb = 10
local_cache = true

[linker]
preferred = "auto"

[parallel]
jobs = 0
"#.to_string()
}

fn generate_release_preset() -> String {
    r#"# rustm configuration (release preset)
# Optimized for maximum runtime performance

[build]
default_profile = "release-max"
incremental = false
strip = true
lto = "fat"
rustflags = ["-C", "target-cpu=native"]

[cache]
sccache = true
max_size_gb = 10
local_cache = true

[linker]
preferred = "auto"

[parallel]
jobs = 0
"#.to_string()
}

fn generate_minimal_preset() -> String {
    r#"# rustm configuration (minimal preset)
# Minimal config with sensible defaults

[build]
default_profile = "dev-fast"
incremental = true
lto = "none"

[cache]
sccache = false
local_cache = true

[linker]
preferred = "default"

[parallel]
jobs = 0
"#.to_string()
}
