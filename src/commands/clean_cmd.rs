//! Clean command implementation

use crate::commands::cli;
use crate::core::cache::CacheManager;
use crate::core::config::RustmConfig;

pub fn execute(args: cli::CleanArgs) -> Result<(), String> {
    let project_dir =
        RustmConfig::find_project_root().ok_or("Not in a Rust project directory.".to_string())?;

    let config = RustmConfig::load(&project_dir);

    println!("🧹 Cleaning build artifacts...");

    // Run cargo clean
    let mut cargo_args = vec!["clean".to_string()];
    if args.release {
        cargo_args.push("--release".to_string());
    }
    if let Some(ref pkg) = args.package {
        cargo_args.push("--package".to_string());
        cargo_args.push(pkg.clone());
    }

    let status = std::process::Command::new("cargo")
        .args(&cargo_args)
        .current_dir(&project_dir)
        .status()
        .map_err(|e| format!("Failed to run cargo clean: {}", e))?;

    if !status.success() {
        return Err("cargo clean failed".to_string());
    }

    println!("  ✅ Cargo artifacts cleaned");

    // Clean sccache if requested
    if args.sccache || args.cache {
        let cache_manager = CacheManager::new(&config.cache);
        match cache_manager.clear() {
            Ok(()) => println!("  ✅ Cache cleared"),
            Err(e) => println!("  ⚠️ Cache clear failed: {}", e),
        }
    }

    // Full clean
    if args.full {
        let target_dir = project_dir.join("target");
        if target_dir.exists() {
            std::fs::remove_dir_all(&target_dir)
                .map_err(|e| format!("Failed to remove target dir: {}", e))?;
            println!("  ✅ Full target directory removed");
        }
    }

    println!("✨ Clean complete!");
    Ok(())
}
