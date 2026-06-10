//! Build engine - the core orchestrator for rustm builds
//!
//! Coordinates caching, linker selection (mold/lld/default),
//! Cranelift codegen backend, profile application, and delegates
//! to Cargo for the actual compilation.
//!
//! Integration stack:
//!   1. Cranelift codegen backend — fast alternative to LLVM for dev builds
//!   2. mold/lld linker optimizations — ICF, relaxation, parallel threads
//!   3. Advanced linker algorithms — call graph clustering, DCE, section ordering
//!   4. LLVM optimizations — PGO, target-features, codegen tuning
//!   5. sccache — distributed compilation cache
//!
//! Profile settings (LTO, codegen-units, etc.) are applied via
//! CARGO_PROFILE_* environment variables, which is the correct
//! Cargo-compatible approach (avoids conflicts with -C embed-bitcode=no).

use crate::core::{RustmConfig, ProfilePreset};
use crate::core::cache::CacheManager;
use crate::core::linker::{LinkerSelector, LinkerInfo};
use crate::core::linker_algo::LinkerAlgorithmConfig;
use crate::core::profile::ProfileResolver;
use crate::core::parallel::ParallelOptimizer;
use crate::core::benchmark::BuildTimer;
use crate::core::cranelift::{CraneliftBackend, CodegenBackend};
use crate::core::llvm::LlvmOptimizer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use colored::*;

/// Build command type
#[derive(Debug, Clone, PartialEq)]
pub enum BuildType {
    Build,
    Run,
    Check,
    Test,
    Clippy,
}

impl std::fmt::Display for BuildType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildType::Build => write!(f, "build"),
            BuildType::Run => write!(f, "run"),
            BuildType::Check => write!(f, "check"),
            BuildType::Test => write!(f, "test"),
            BuildType::Clippy => write!(f, "clippy"),
        }
    }
}

/// Build request parameters
#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub build_type: BuildType,
    pub profile_name: Option<String>,
    pub release: bool,
    pub target: Option<String>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub jobs: Option<u32>,
    pub verbose: bool,
    pub quiet: bool,
    pub args: Vec<String>,
    pub project_dir: PathBuf,
    /// Codegen backend override: auto, cranelift, llvm
    pub codegen_backend: Option<String>,
    /// Enable PGO instrumented build
    pub pgo_generate: bool,
    /// Enable PGO optimized build
    pub pgo_use: bool,
    /// PGO profile path
    pub pgo_path: Option<String>,
    /// Auto-detect CPU features
    pub target_features: bool,
    /// Enable LLVM optimization remarks
    pub llvm_remarks: bool,
}

/// Result of a build operation
#[derive(Debug)]
pub struct BuildResult {
    pub success: bool,
    pub duration_ms: u128,
    pub profile_used: String,
    pub linker_used: String,
    pub linker_speed_tier: String,
    pub codegen_backend_used: String,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub mold_flags_applied: Vec<String>,
    pub cranelift_used: bool,
}

/// The main build engine
pub struct BuildEngine {
    config: RustmConfig,
    cache_manager: CacheManager,
    linker_selector: LinkerSelector,
    profile_resolver: ProfileResolver,
    parallel_optimizer: ParallelOptimizer,
    cranelift: CraneliftBackend,
}

impl BuildEngine {
    pub fn new(config: RustmConfig) -> Self {
        let cranelift = CraneliftBackend::new();
        Self {
            cache_manager: CacheManager::new(&config.cache),
            linker_selector: LinkerSelector::new(&config.linker),
            profile_resolver: ProfileResolver::new(&config.profiles),
            parallel_optimizer: ParallelOptimizer::new(&config.parallel),
            cranelift,
            config,
        }
    }

    /// Execute a build request
    pub fn build(&self, request: &BuildRequest) -> Result<BuildResult, String> {
        let timer = BuildTimer::new();
        
        // Print banner
        self.print_banner(request);

        // Resolve profile
        let profile_name = request.profile_name.clone()
            .unwrap_or_else(|| self.config.build.default_profile.clone());
        let profile = self.profile_resolver.resolve(&profile_name, request.release)?;
        
        // Determine linker — applies mold optimizations automatically
        let linker_info = self.linker_selector.select()?;
        
        // Determine codegen backend
        let backend = self.resolve_codegen_backend(request, &profile_name, request.release)?;
        let cranelift_used = backend == CodegenBackend::Cranelift;
        let backend_name = backend.to_string();
        
        // Set up cache
        let cache_status = self.cache_manager.setup()?;
        
        // Calculate parallel jobs
        let jobs = request.jobs
            .or(Some(self.config.parallel.jobs))
            .filter(|&j| j > 0)
            .unwrap_or_else(|| self.parallel_optimizer.optimal_jobs());
        
        // Build cargo command args
        let cargo_args = self.build_cargo_args(request, &profile, &linker_info, jobs)?;
        
        // Build environment variables
        // LTO and profile settings use CARGO_PROFILE_* env vars (not RUSTFLAGS)
        // This avoids conflicts with -C embed-bitcode=no in build scripts
        let env_vars = self.build_env_vars(&linker_info, &profile, jobs, &backend, request)?;
        
        // Print optimization summary
        if !request.quiet {
            self.print_optimization_summary(&profile_name, &linker_info, jobs, &cache_status, &backend);
        }

        // Execute cargo
        let success = self.execute_cargo(&cargo_args, &env_vars, &request.project_dir, request.verbose)?;
        let duration = timer.elapsed_ms();

        // Collect mold flags that were applied
        let mold_flags_applied = linker_info.to_rustflags();

        Ok(BuildResult {
            success,
            duration_ms: duration,
            profile_used: profile_name,
            linker_used: linker_info.name.clone(),
            linker_speed_tier: linker_info.speed_tier.to_string(),
            codegen_backend_used: backend_name,
            cache_hits: cache_status.hits,
            cache_misses: cache_status.misses,
            mold_flags_applied,
            cranelift_used,
        })
    }

    /// Resolve which codegen backend to use
    fn resolve_codegen_backend(&self, request: &BuildRequest, profile_name: &str, release: bool) -> Result<CodegenBackend, String> {
        // Priority: CLI flag > config > auto
        let backend_str = request.codegen_backend.as_ref()
            .unwrap_or(&self.config.build.codegen_backend);
        
        let backend: CodegenBackend = backend_str.parse()?;
        
        // Check if Cranelift should actually be used
        if self.cranelift.should_use_cranelift(backend, profile_name, release) {
            if !self.cranelift.is_available() {
                // Warn but don't error — fall back to LLVM
                eprintln!("{} Cranelift requested but not available, falling back to LLVM", "⚠️".yellow());
                eprintln!("  Install with: rustup component add rustc_codegen_cranelift --toolchain nightly");
                return Ok(CodegenBackend::Llvm);
            }
            Ok(CodegenBackend::Cranelift)
        } else {
            Ok(backend)
        }
    }

    fn build_cargo_args(
        &self,
        request: &BuildRequest,
        profile: &ProfilePreset,
        linker_info: &LinkerInfo,
        jobs: u32,
    ) -> Result<Vec<String>, String> {
        let mut args = vec![request.build_type.to_string()];

        // Determine cargo profile
        let cargo_profile = if request.release {
            "release"
        } else {
            match request.build_type {
                BuildType::Test => "test",
                _ => "dev",
            }
        };

        // Use custom profile name if it maps to a Cargo profile
        match request.profile_name.as_deref() {
            Some("fastest") | Some("release-fast") | Some("release-max") => {
                args.push("--release".to_string());
            }
            Some("dev-fast") | Some("dev-check") | Some("dev-cranelift") => {
                // Dev profile — no --release
            }
            Some("balanced") => {
                args.push("--release".to_string());
            }
            Some("size-opt") => {
                args.push("--release".to_string());
            }
            Some(name) => {
                // Custom profile — try to map to Cargo custom profile
                args.push("--profile".to_string());
                args.push(name.to_string());
            }
            None => {
                if request.release {
                    args.push("--release".to_string());
                }
            }
        }

        // Target
        if let Some(ref target) = request.target {
            args.push("--target".to_string());
            args.push(target.clone());
        }

        // Features
        if request.all_features {
            args.push("--all-features".to_string());
        } else {
            if request.no_default_features {
                args.push("--no-default-features".to_string());
            }
            if !request.features.is_empty() {
                args.push("--features".to_string());
                args.push(request.features.join(","));
            }
        }

        // Jobs
        args.push("-j".to_string());
        args.push(jobs.to_string());

        // Verbose
        if request.verbose {
            args.push("-v".to_string());
        }

        // Extra cargo args
        if request.build_type == BuildType::Run && !request.args.is_empty() {
            args.push("--".to_string());
            args.extend(request.args.iter().cloned());
        }

        Ok(args)
    }

    /// Build environment variables for the build
    fn build_env_vars(
        &self,
        linker_info: &LinkerInfo,
        profile: &ProfilePreset,
        jobs: u32,
        backend: &CodegenBackend,
        request: &BuildRequest,
    ) -> Result<HashMap<String, String>, String> {
        let mut env = HashMap::new();

        // Determine cargo profile env name
        let profile_env = if request.release { "release" } else { "dev" };

        // ═══════════════════════════════════════════════════════════
        // RUSTFLAGS — linker flags + Cranelift + LLVM + algo flags
        // ═══════════════════════════════════════════════════════════
        let mut rustflags = Vec::new();

        // 1. Profile rustflags (target-cpu, etc.)
        rustflags.extend(profile.rustflags.iter().cloned());

        // 2. Config rustflags
        rustflags.extend(self.config.build.rustflags.iter().cloned());

        // 3. Linker selection (mold/lld/default)
        rustflags.extend(linker_info.to_rustflags());

        // 4. Advanced linker algorithm flags
        let algo_config = if request.release { LinkerAlgorithmConfig::release_max() } else { LinkerAlgorithmConfig::dev_fast() };
        rustflags.extend(algo_config.to_linker_flags());

        // 5. LLVM optimizer flags (target-features, PGO, etc.)
        if request.target_features || request.pgo_generate || request.pgo_use || request.llvm_remarks {
            let mut llvm_config = crate::core::llvm::LlvmOptConfig::default();
            if request.target_features {
                llvm_config.target_features = crate::core::llvm::detect_cpu_features();
            }
            if request.pgo_generate {
                llvm_config.pgo_enabled = true;
                llvm_config.pgo_stage = Some(crate::core::llvm::PgoStage::Generate);
                llvm_config.pgo_path = request.pgo_path.clone();
            }
            if request.pgo_use {
                llvm_config.pgo_enabled = true;
                llvm_config.pgo_stage = Some(crate::core::llvm::PgoStage::Use);
                llvm_config.pgo_path = request.pgo_path.clone();
            }
            if request.llvm_remarks {
                llvm_config.optimization_remarks = true;
            }
            let llvm_optimizer = LlvmOptimizer::new(&llvm_config);
            rustflags.extend(llvm_optimizer.build_rustflags(profile));
        }

        // 6. Cranelift codegen backend injection
        if *backend == CodegenBackend::Cranelift {
            rustflags.extend(self.cranelift.build_rustflags());
        }

        // Set RUSTFLAGS
        if !rustflags.is_empty() {
            env.insert("RUSTFLAGS".to_string(), rustflags.join(" "));
        }

        // ═══════════════════════════════════════════════════════════
        // CARGO_PROFILE_* — LTO, codegen-units, opt-level, etc.
        // ═══════════════════════════════════════════════════════════
        
        // LTO
        if let Some(ref lto) = profile.lto {
            if lto != "none" {
                env.insert(format!("CARGO_PROFILE_{}_LTO", profile_env.to_uppercase()), lto.clone());
            }
        }

        // Codegen units
        if let Some(cgu) = profile.codegen_units {
            env.insert(format!("CARGO_PROFILE_{}_CODEGEN_UNITS", profile_env.to_uppercase()), cgu.to_string());
        }

        // Opt level
        if let Some(ref opt) = profile.opt_level {
            if opt != "0" {
                env.insert(format!("CARGO_PROFILE_{}_OPT_LEVEL", profile_env.to_uppercase()), opt.clone());
            }
        }

        // Strip
        if profile.strip.unwrap_or(false) {
            env.insert(format!("CARGO_PROFILE_{}_STRIP", profile_env.to_uppercase()), "symbols".to_string());
        }

        // Panic strategy
        if let Some(ref panic) = profile.panic {
            env.insert(format!("CARGO_PROFILE_{}_PANIC", profile_env.to_uppercase()), panic.clone());
        }

        // Incremental
        if let Some(inc) = profile.incremental {
            env.insert(format!("CARGO_PROFILE_{}_INCREMENTAL", profile_env.to_uppercase()), inc.to_string());
        }

        // Debug info
        if let Some(ref debug) = profile.debug {
            env.insert(format!("CARGO_PROFILE_{}_DEBUG", profile_env.to_uppercase()), debug.clone());
        }

        // sccache — distributed compilation cache
        if self.cache_manager.is_sccache_active() {
            if let Ok(sccache_path) = which::which("sccache") {
                env.insert("RUSTC_WRAPPER".to_string(), sccache_path.to_string_lossy().to_string());
            }
        }

        // mold uses parallel threads internally
        if linker_info.name == "mold" {
            let mold_threads = linker_info.mold_flags.threads
                .unwrap_or_else(|| (jobs as f64 * 0.75).ceil() as u32);
            env.insert("MOLD_JOBS".to_string(), mold_threads.to_string());
        }

        // Custom env from config
        for (key, value) in &self.config.env {
            env.insert(key.clone(), value.clone());
        }

        Ok(env)
    }

    fn execute_cargo(
        &self,
        args: &[String],
        env_vars: &HashMap<String, String>,
        project_dir: &Path,
        verbose: bool,
    ) -> Result<bool, String> {
        let mut cmd = Command::new("cargo");
        cmd.args(args);
        cmd.current_dir(project_dir);

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        if verbose {
            println!("{} cargo {}", "$".dimmed(), args.join(" "));
            if let Some(rustflags) = env_vars.get("RUSTFLAGS") {
                println!("  {} RUSTFLAGS={}", "$".dimmed(), rustflags.dimmed());
            }
            // Show profile env vars
            for (key, value) in env_vars.iter().filter(|(k, _)| k.starts_with("CARGO_PROFILE")) {
                println!("  {} {}={}", "$".dimmed(), key.dimmed(), value.dimmed());
            }
            if let Some(mold_jobs) = env_vars.get("MOLD_JOBS") {
                println!("  {} MOLD_JOBS={}", "$".dimmed(), mold_jobs.dimmed());
            }
        }

        let status = cmd.status()
            .map_err(|e| format!("Failed to execute cargo: {}", e))?;

        Ok(status.success())
    }

    fn print_banner(&self, request: &BuildRequest) {
        let version = env!("CARGO_PKG_VERSION");
        println!();
        println!("{} {} {}",
            "rustm".bold().bright_cyan(),
            format!("v{}", version).dimmed(),
            "- Blazing-fast Rust builder".dimmed()
        );
        println!("{} {}",
            "Command:".dimmed(),
            request.build_type.to_string().yellow().bold()
        );
    }

    fn print_optimization_summary(
        &self,
        profile_name: &str,
        linker_info: &LinkerInfo,
        jobs: u32,
        cache_status: &crate::core::cache::CacheStatus,
        backend: &CodegenBackend,
    ) {
        println!();
        println!("{} Optimization Stack", "⚡".to_string());
        println!("{}", "─".repeat(50));
        println!("  {} Profile: {}", "→".green(), profile_name.cyan());
        println!("  {} Linker:  {} ({})", "→".green(),
            linker_info.name.cyan(),
            linker_info.speed_tier.to_string().green()
        );
        println!("  {} Backend: {}", "→".green(),
            match backend {
                CodegenBackend::Cranelift => "Cranelift ⚡".bright_yellow(),
                CodegenBackend::Llvm => "LLVM".cyan(),
                CodegenBackend::Auto => "Auto (LLVM)".cyan(),
            }
        );
        println!("  {} Jobs:    {}", "→".green(), jobs);
        println!("  {} Cache:   {} (hits: {}, misses: {})", "→".green(),
            if cache_status.sccache_active { "active".green() } else { "inactive".yellow() },
            cache_status.hits.to_string().green(),
            cache_status.misses.to_string().yellow()
        );

        if linker_info.name == "mold" {
            println!("  {} mold optimizations:", "→".green());
            println!("     ICF:       {}", linker_info.mold_flags.icf);
            println!("     Relaxation: {}", if linker_info.mold_flags.relax { "ON".green() } else { "OFF".yellow() });
            println!("     Threads:   {}", linker_info.mold_flags.threads
                .map(|n| n.to_string()).unwrap_or_else(|| "auto".to_string()));
        }

        if *backend == CodegenBackend::Cranelift {
            println!("  {} Cranelift: Fast codegen (2-5x faster than LLVM)", "→".bright_yellow());
            println!("     Note: Binary performance ~80-95% of LLVM");
        }

        println!("{}", "─".repeat(50));
    }
}

/// Print build result summary
pub fn print_build_summary(result: &BuildResult) {
    println!();
    if result.success {
        let duration_str = if result.duration_ms >= 1000 {
            format!("{:.2}s", result.duration_ms as f64 / 1000.0)
        } else {
            format!("{}ms", result.duration_ms)
        };
        println!("{} Build completed in {}",
            "✅",
            duration_str.green().bold()
        );
    } else {
        let duration_str = if result.duration_ms >= 1000 {
            format!("{:.2}s", result.duration_ms as f64 / 1000.0)
        } else {
            format!("{}ms", result.duration_ms)
        };
        println!("{} Build failed after {}",
            "❌",
            duration_str.red().bold()
        );
    }

    // Show linker optimization applied
    if !result.mold_flags_applied.is_empty() && result.linker_used != "default" {
        println!("   Linker: {} ({}) — {} optimizations applied",
            result.linker_used.green(),
            result.linker_speed_tier.green(),
            result.mold_flags_applied.len()
        );
    }

    // Show codegen backend
    if result.cranelift_used {
        println!("   Backend: {} (fast codegen)", "Cranelift".bright_yellow());
    } else {
        println!("   Backend: {}", result.codegen_backend_used.cyan());
    }

    if result.cache_hits > 0 {
        println!("   Cache hits: {} | Misses: {}",
            result.cache_hits.to_string().green(),
            result.cache_misses.to_string().yellow()
        );
    }
}


