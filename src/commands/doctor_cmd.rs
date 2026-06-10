//! Doctor command - check system for optimization opportunities
//! Includes mold linker diagnostics, Cranelift backend detection,
//! and architecture comparison

use crate::core::linker::LinkerSelector;
use crate::core::cache::CacheManager;
use crate::core::config::{RustmConfig, CacheConfig};
use crate::core::cranelift::CraneliftBackend;
use crate::utils::system;
use colored::*;

pub fn execute() -> Result<(), String> {
    println!();
    println!("{} rustm Doctor — System Optimization Check\n", "🏥".to_string());
    println!("{}", "═".repeat(65));

    // System info
    println!("\n{} System Information", "💻".to_string());
    println!("  {}", system::system_info());
    println!("  {}", system::rust_info());

    // Check linkers with detailed comparison
    println!("\n{} Linker Availability & Comparison", "🔗".to_string());
    let available = LinkerSelector::detect_available();
    for linker in &available {
        let (icon, tier) = match linker.speed_tier {
            crate::core::linker::LinkerSpeedTier::BlazingFast => ("⚡", "BLAZING — mold"),
            crate::core::linker::LinkerSpeedTier::Fast => ("🔥", "FAST — lld"),
            crate::core::linker::LinkerSpeedTier::Slow => ("📦", "STD — default"),
        };
        let version_str = linker.version.as_ref()
            .map(|v| format!("({})", v))
            .unwrap_or_default();
        println!("  {} {} {} — {}",
            icon, linker.name.cyan(),
            version_str.dimmed(),
            tier.green()
        );
    }

    let best_linker = available.iter().max_by_key(|l| l.speed_tier);
    match best_linker {
        Some(linker) if linker.is_fast => {
            println!("  {} Best available: {} ({})",
                "✅".to_string(),
                linker.name.green().bold(),
                linker.speed_tier.to_string().green()
            );
        }
        _ => {
            println!("  {} No fast linker found!", "⚠️".to_string());
            println!("    {} mold is 5-10x faster than default ld, 2-3x faster than lld",
                "→".yellow());
        }
    }

    // mold architecture explanation
    if available.iter().any(|l| l.name == "mold") {
        println!("\n  {} mold's key architectural advantages:", "📐".to_string());
        println!("     {} Parallel symbol resolution (fine-grained mutex per bucket)", "→".green());
        println!("     {} mmap I/O — zero-copy file reading (no heap buffering)", "→".green());
        println!("     {} Multi-threaded ICF — parallel hash + compare", "→".green());
        println!("     {} TLGP relaxation — GOT-indirect → PC-relative", "→".green());
        println!("     {} Fast build-ID (~zero cost vs SHA-1)", "→".green());
        println!("     {} Compact .dyn section support", "→".green());
    }

    // Linker comparison table
    println!("\n{} Linker Architecture Comparison", "📊".to_string());
    println!("  {:<25} {:<20} {:<20} {:<20}", "Feature", "mold", "lld", "default (ld)");
    println!("  {}", "─".repeat(85));
    let comparisons = LinkerSelector::linker_comparison();
    for entry in &comparisons {
        println!("  {:<25} {:<20} {:<20} {:<20}",
            entry.feature.dimmed(),
            entry.mold.green(),
            entry.lld.yellow(),
            entry.default.red()
        );
    }

    // ═══════════════════════════════════════════════════════════
    // Cranelift Codegen Backend Detection
    // ═══════════════════════════════════════════════════════════
    println!("\n{} Codegen Backend Availability", "⚡".to_string());
    let cranelift = CraneliftBackend::new();
    let cranelift_info = cranelift.info();
    
    if cranelift_info.available {
        println!("  {} Cranelift: available ({})", "✅".to_string(),
            cranelift_info.install_method.to_string().green());
        if let Some(ref path) = cranelift_info.dylib_path {
            println!("  Path: {}", path.display());
        }
        if let Some(ref ver) = cranelift_info.version {
            println!("  Version: {}", ver);
        }
        println!("  {} Cranelift provides 2-5x faster codegen for dev builds", "→".green());
    } else {
        println!("  {} Cranelift: not installed", "⚠️".to_string());
        println!("  {} Install with:", "→".to_string());
        println!("    {} rustup component add rustc_codegen_cranelift --toolchain nightly",
            "→".cyan());
        println!("  {} Or build from source: https://github.com/rust-lang/rustc_codegen_cranelift",
            "→".dimmed());
    }

    // LLVM backend info
    println!("  {} LLVM: always available (default backend)", "✅".to_string());
    println!("  {} Auto mode: Cranelift for dev, LLVM for release", "→".dimmed());

    // Codegen backend comparison
    println!("\n{} Codegen Backend Comparison", "📊".to_string());
    println!("  ┌──────────────────────┬──────────────┬──────────────┐");
    println!("  │ Metric               │ LLVM         │ Cranelift    │");
    println!("  ├──────────────────────┼──────────────┼──────────────┤");
    println!("  │ Codegen speed        │ Baseline     │ 2-5x faster  │");
    println!("  │ Memory usage          │ Baseline     │ 40-60% less  │");
    println!("  │ Binary performance   │ 100%         │ 80-95%       │");
    println!("  │ Optimization passes  │ ~120         │ ~5           │");
    println!("  │ Auto-vectorization   │ Full         │ Limited      │");
    println!("  │ LTO support           │ Full         │ None         │");
    println!("  └──────────────────────┴──────────────┴──────────────┘");

    // Check sccache
    println!("\n{} Build Cache", "💾".to_string());
    let cache_config = CacheConfig::default();
    let cache = CacheManager::new(&cache_config);
    if cache.is_sccache_active() {
        println!("  {} sccache: installed and ready", "✅".to_string());
    } else {
        println!("  {} sccache: not found", "⚠️".to_string());
        println!("    {} Install with: {}",
            "→".to_string(),
            "cargo install sccache".cyan()
        );
    }

    // Check rustm config
    println!("\n{} rustm Configuration", "⚙️".to_string());
    match RustmConfig::find_project_root() {
        Some(project_dir) => {
            let config_path = project_dir.join("rustm.toml");
            if config_path.exists() {
                println!("  {} rustm.toml: found at {}", "✅".to_string(),
                    config_path.display().to_string().cyan());

                let config = RustmConfig::load(&project_dir);
                println!("  {} Preferred linker: {}", "→".to_string(), config.linker.preferred.cyan());
                println!("  {} Codegen backend: {}", "→".to_string(), config.build.codegen_backend.cyan());
                println!("  {} ICF level: {}", "→".to_string(), config.linker.icf.cyan());
                println!("  {} Relaxation: {}", "→".to_string(),
                    if config.linker.relax { "ON".green() } else { "OFF".yellow() }.to_string());
                println!("  {} Build-ID: {}", "→".to_string(), config.linker.build_id.cyan());
            } else {
                println!("  {} rustm.toml: not found", "⚠️".to_string());
                println!("    {} Run: {} to create one", "→".to_string(), "rustm init".cyan());
            }
        }
        None => {
            println!("  {} Not in a Rust project", "⚠️".to_string());
        }
    }

    // Check parallelism
    println!("\n{} Parallelism", "🔄".to_string());
    let parallel_config = crate::core::config::ParallelConfig::default();
    let optimizer = crate::core::parallel::ParallelOptimizer::new(&parallel_config);
    println!("  {}", optimizer.system_info());
    println!("  {} mold also uses parallel threads for linking", "→".dimmed());

    // Recommendations
    println!("\n{} Recommendations", "📌".to_string());
    let mut recommendations = Vec::new();

    if !cache.is_sccache_active() {
        recommendations.push("Install sccache for distributed caching (30-70% faster incremental builds)".to_string());
    }

    if !available.iter().any(|l| l.name == "mold") {
        if cfg!(target_os = "linux") {
            recommendations.push("Install mold for 5-10x faster linking: apt install mold / brew install mold".to_string());
        }
        if !available.iter().any(|l| l.name == "lld") {
            recommendations.push("Install lld for 2-5x faster linking: rustup component add llvm-tools".to_string());
        }
    }

    if !cranelift_info.available {
        recommendations.push("Install Cranelift for 2-5x faster dev builds: rustup component add rustc_codegen_cranelift --toolchain nightly".to_string());
    }

    if RustmConfig::find_project_root().is_some() {
        if let Some(dir) = RustmConfig::find_project_root() {
            if !dir.join("rustm.toml").exists() {
                recommendations.push("Run 'rustm init' to create optimized config".to_string());
            }
        }
    }

    if recommendations.is_empty() {
        println!("  {} Your system is fully optimized! 🚀", "✅".to_string());
    } else {
        for (i, rec) in recommendations.iter().enumerate() {
            println!("  {}. {}", i + 1, rec.yellow());
        }
    }

    println!("\n{}", "═".repeat(65));
    println!("{} Doctor check complete!\n", "✨".to_string());

    Ok(())
}
