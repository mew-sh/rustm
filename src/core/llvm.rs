//! LLVM optimization engine for rustm
//!
//! Integrates LLVM-level optimizations beyond what Cargo provides:
//! PGO, BOLT, target-feature auto-detection, LLVM codegen tuning.

use crate::core::config::ProfilePreset;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// PGO workflow stage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgoStage {
    Generate,
    #[allow(dead_code)]
    Run,
    Use,
}

impl std::fmt::Display for PgoStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgoStage::Generate => write!(f, "generate"),
            PgoStage::Run => write!(f, "run"),
            PgoStage::Use => write!(f, "use"),
        }
    }
}

/// LLVM optimization configuration
#[derive(Debug, Clone)]
pub struct LlvmOptConfig {
    pub target_cpu: String,
    pub target_features: Vec<String>,
    pub llvm_args: Vec<String>,
    pub pgo_enabled: bool,
    pub pgo_stage: Option<PgoStage>,
    pub pgo_path: Option<String>,
    #[allow(dead_code)]
    pub bolt_enabled: bool,
    pub bolt_path: Option<PathBuf>,
    #[allow(dead_code)]
    pub autofdo_enabled: bool,
    pub optimization_remarks: bool,
    pub print_stats: bool,
}

impl Default for LlvmOptConfig {
    fn default() -> Self {
        Self {
            target_cpu: "native".to_string(),
            target_features: Vec::new(),
            llvm_args: Vec::new(),
            pgo_enabled: false,
            pgo_stage: None,
            pgo_path: None,
            bolt_enabled: false,
            bolt_path: None,
            autofdo_enabled: false,
            optimization_remarks: false,
            print_stats: false,
        }
    }
}

/// Detect CPU features for maximum auto-vectorization
pub fn detect_cpu_features() -> Vec<String> {
    let mut features = Vec::new();

    if let Ok(output) = Command::new("rustc").args(["--print", "cfg"]).output()
        && let Ok(stdout) = String::from_utf8(output.stdout)
    {
        #[cfg(target_arch = "x86_64")]
        {
            if stdout.contains("target_feature=\"avx2\"") {
                features.push("+avx2".to_string());
                features.push("+fma".to_string());
            }
            if stdout.contains("target_feature=\"sse4.2\"") {
                features.push("+sse4.2".to_string());
            }
            if stdout.contains("target_feature=\"bmi1\"") {
                features.push("+bmi1".to_string());
            }
            if stdout.contains("target_feature=\"bmi2\"") {
                features.push("+bmi2".to_string());
            }
            if stdout.contains("target_feature=\"popcnt\"") {
                features.push("+popcnt".to_string());
            }
            if stdout.contains("target_feature=\"lzcnt\"") {
                features.push("+lzcnt".to_string());
            }
            if stdout.contains("target_feature=\"avx512f\"") {
                features.push("+avx512f".to_string());
                features.push("+avx512cd".to_string());
                features.push("+avx512bw".to_string());
                features.push("+avx512dq".to_string());
                features.push("+avx512vl".to_string());
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            features.push("+neon".to_string());
            if stdout.contains("target_feature=\"sve\"") {
                features.push("+sve".to_string());
            }
            if stdout.contains("target_feature=\"sve2\"") {
                features.push("+sve2".to_string());
            }
        }
    }

    features
}

/// LLVM optimization engine
pub struct LlvmOptimizer {
    config: LlvmOptConfig,
}

impl LlvmOptimizer {
    pub fn new(config: &LlvmOptConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Build RUSTFLAGS for LLVM optimizations
    pub fn build_rustflags(&self, profile: &ProfilePreset) -> Vec<String> {
        let mut flags = Vec::new();

        // 1. Target CPU — enables all host CPU features for auto-vectorization
        if self.config.target_cpu == "native" {
            flags.push("-C target-cpu=native".to_string());
        } else if !self.config.target_cpu.is_empty() {
            flags.push(format!("-C target-cpu={}", self.config.target_cpu));
        }

        // 2. Target Features — fine-grained SIMD/feature control
        let mut all_features = self.config.target_features.clone();
        if (profile.opt_level.as_deref() == Some("3") || profile.opt_level.as_deref() == Some("2"))
            && all_features.is_empty()
        {
            all_features = detect_cpu_features();
        }
        if !all_features.is_empty() {
            flags.push(format!("-C target-feature={}", all_features.join(",")));
        }

        // 3. PGO (Profile-Guided Optimization) — 10-30% runtime improvement
        //    Phase 1: -C profile-generate=<dir>  (instrumented binary)
        //    Phase 2: Run workload (collects .profraw files)
        //    Phase 3: -C profile-use=<file>       (optimized with profile)
        if self.config.pgo_enabled {
            match self.config.pgo_stage {
                Some(PgoStage::Generate) => {
                    let path = self
                        .config
                        .pgo_path
                        .as_deref()
                        .unwrap_or("./target/pgo-profiles");
                    flags.push(format!("-C profile-generate={}", path));
                }
                Some(PgoStage::Use) => {
                    let path = self
                        .config
                        .pgo_path
                        .as_deref()
                        .unwrap_or("./target/pgo-profiles/merged.profdata");
                    flags.push(format!("-C profile-use={}", path));
                }
                Some(PgoStage::Run) | None => {}
            }
        }

        // 4. LLVM optimization remarks
        if self.config.optimization_remarks {
            flags.push("-Z remark=loop-vectorize".to_string());
            flags.push("-Z remark=inline".to_string());
        }

        // 5. LLVM statistics
        if self.config.print_stats {
            flags.push("-Z print-stats".to_string());
        }

        // 6. Additional LLVM args
        if !self.config.llvm_args.is_empty() {
            flags.push(format!("-C llvm-args={}", self.config.llvm_args.join(",")));
        }

        // 7. Symbol mangling for size optimization
        if profile.opt_level.as_deref() == Some("z") || profile.opt_level.as_deref() == Some("s") {
            flags.push("-C symbol-mangling-version=v0".to_string());
        }

        flags
    }

    /// Generate environment variables for LLVM optimizations
    #[allow(dead_code)]
    pub fn build_env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        if self.config.optimization_remarks || self.config.print_stats {
            env.insert("RUSTC_BOOTSTRAP".to_string(), "1".to_string());
        }

        if self.config.pgo_enabled
            && let Some(PgoStage::Generate) = self.config.pgo_stage
        {
            let path = self
                .config
                .pgo_path
                .as_deref()
                .unwrap_or("./target/pgo-profiles");
            env.insert(
                "LLVM_PROFILE_FILE".to_string(),
                format!("{}/default_%p_%m.profraw", path),
            );
        }

        env
    }

    /// Check if BOLT is available
    pub fn detect_bolt(&self) -> Option<PathBuf> {
        which::which("llvm-bolt")
            .ok()
            .or_else(|| which::which("perf2bolt").ok())
    }

    /// Check if llvm-profdata is available
    pub fn detect_llvm_profdata(&self) -> Option<PathBuf> {
        which::which("llvm-profdata").ok()
    }

    /// Run BOLT optimization on a binary
    pub fn run_bolt(
        &self,
        binary_path: &Path,
        profile_path: Option<&Path>,
        output_path: &Path,
    ) -> Result<(), String> {
        let bolt = match self.config.bolt_path.as_ref() {
            Some(p) => p.clone(),
            None => self.detect_bolt()
                .ok_or_else(|| "BOLT not found. Install from: https://github.com/llvm/llvm-project/tree/main/bolt".to_string())?,
        };

        let mut cmd = Command::new(&bolt);
        cmd.arg(binary_path.to_string_lossy().to_string());
        cmd.arg("-o").arg(output_path.to_string_lossy().to_string());

        if let Some(profile) = profile_path {
            cmd.arg("-data").arg(profile.to_string_lossy().to_string());
        }

        // BOLT optimization passes
        cmd.arg("-reorder-blocks=cache+");
        cmd.arg("-reorder-functions=hfsort+");
        cmd.arg("-split-functions=3");
        cmd.arg("-split-all-cold");
        cmd.arg("-dyno-stats");
        cmd.arg("-icf=1");
        cmd.arg("-use-gnu-stack");

        let status = cmd
            .status()
            .map_err(|e| format!("Failed to run BOLT: {}", e))?;

        if !status.success() {
            return Err("BOLT optimization failed".to_string());
        }

        Ok(())
    }

    /// Merge PGO profile data
    pub fn merge_pgo_profiles(&self, profile_dir: &Path, output_path: &Path) -> Result<(), String> {
        let profdata = self.detect_llvm_profdata().ok_or_else(|| {
            "llvm-profdata not found. Install: rustup component add llvm-tools".to_string()
        })?;

        let profraw_files: Vec<_> = std::fs::read_dir(profile_dir)
            .map_err(|e| format!("Failed to read profile directory: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "profraw")
                    .unwrap_or(false)
            })
            .collect();

        if profraw_files.is_empty() {
            return Err("No .profraw files found. Run the instrumented binary first.".to_string());
        }

        let mut cmd = Command::new(&profdata);
        cmd.arg("merge");
        cmd.arg("-o").arg(output_path.to_string_lossy().to_string());

        for file in &profraw_files {
            cmd.arg(file.path().to_string_lossy().to_string());
        }

        let status = cmd
            .status()
            .map_err(|e| format!("Failed to run llvm-profdata: {}", e))?;

        if !status.success() {
            return Err("llvm-profdata merge failed".to_string());
        }

        Ok(())
    }

    /// Print PGO workflow instructions
    pub fn pgo_instructions(&self) -> String {
        let mut s = String::new();
        s.push_str("\n🎯 PGO (Profile-Guided Optimization) Workflow\n");
        s.push_str(&format!("{}\n\n", "═".repeat(60)));
        s.push_str("① Step 1: Build instrumented binary\n");
        s.push_str("  rustm build --release --profile release-fast --pgo-generate\n\n");
        s.push_str("② Step 2: Run representative workload\n");
        s.push_str("  ./target/release/your-binary <benchmark-args>\n");
        s.push_str("  (Profile data written to ./target/pgo-profiles/)\n\n");
        s.push_str("③ Step 3: Merge profile data\n");
        s.push_str("  rustm pgo merge\n\n");
        s.push_str("④ Step 4: Rebuild with profile data\n");
        s.push_str("  rustm build --release --profile release-max --pgo-use\n\n");
        s.push_str("📈 Expected improvement: 10-30% runtime speedup\n");
        s.push_str(&format!("{}\n", "═".repeat(60)));
        s
    }

    /// Print BOLT workflow instructions
    pub fn bolt_instructions(&self) -> String {
        let mut s = String::new();
        s.push_str("\n🔨 BOLT (Binary Optimization and Layout Tool) Workflow\n");
        s.push_str(&format!("{}\n\n", "═".repeat(60)));
        s.push_str("① Step 1: Build with debug info + profile\n");
        s.push_str("  rustm build --release --profile release-fast\n\n");
        s.push_str("② Step 2: Collect profile data\n");
        s.push_str("  perf record -g ./target/release/your-binary <args>\n\n");
        s.push_str("③ Step 3: Convert perf data for BOLT\n");
        s.push_str("  perf2bolt -p perf.data -o bolt.fdata ./target/release/your-binary\n\n");
        s.push_str("④ Step 4: Apply BOLT optimization\n");
        s.push_str("  rustm bolt --binary ./target/release/your-binary --profile bolt.fdata\n\n");
        s.push_str("📈 Expected improvement: 5-15% on top of PGO\n");
        s.push_str(&format!("{}\n", "═".repeat(60)));
        s
    }
}

/// Display LLVM optimization capabilities
pub fn print_llvm_info() -> String {
    let mut info = String::new();

    info.push_str("\n🔧 LLVM Optimization Capabilities\n");
    info.push_str(&format!("{}\n\n", "═".repeat(60)));

    info.push_str("🎯 1. PGO (Profile-Guided Optimization)\n");
    info.push_str("   - Instrument binary → Run workload → Rebuild with profile\n");
    info.push_str("   - Hot function inlining + basic block reordering\n");
    info.push_str("   - 10-30% runtime improvement\n\n");

    info.push_str("🔨 2. BOLT (Binary Optimization and Layout Tool)\n");
    info.push_str("   - Post-link function/block reordering\n");
    info.push_str("   - I-cache optimization + branch layout\n");
    info.push_str("   - 5-15% improvement on top of PGO\n\n");

    info.push_str("⚡ 3. AutoFDO (Hardware Profile-Guided Optimization)\n");
    info.push_str("   - Uses perf hardware counters (no instrumentation)\n");
    info.push_str("   - Zero overhead profiling\n");
    info.push_str("   - 5-15% improvement\n\n");

    info.push_str("🧬 4. Target Feature Auto-Detection\n");
    info.push_str("   - AVX2, AVX-512, FMA, NEON, SVE auto-detection\n");
    info.push_str("   - Enables LLVM auto-vectorizer for maximum SIMD\n\n");

    info.push_str("⚙️ 5. LLVM Codegen Tuning\n");
    info.push_str("   - Loop unroll threshold\n");
    info.push_str("   - Inlining threshold\n");
    info.push_str("   - Vectorization width\n\n");

    info.push_str(&format!("{}\n", "═".repeat(60)));

    // Check what's available
    let has_profdata = which::which("llvm-profdata").is_ok();
    let has_bolt = which::which("llvm-bolt").is_ok() || which::which("perf2bolt").is_ok();

    info.push_str("\n✅ Available Tools\n");
    info.push_str(&format!(
        "  {} llvm-profdata: {}\n",
        if has_profdata { "✅" } else { "❌" },
        if has_profdata {
            "installed"
        } else {
            "not found (rustup component add llvm-tools)"
        }
    ));
    info.push_str(&format!(
        "  {} BOLT: {}\n",
        if has_bolt { "✅" } else { "❌" },
        if has_bolt { "installed" } else { "not found" }
    ));

    // CPU features
    let features = detect_cpu_features();
    if !features.is_empty() {
        info.push_str("\n🧬 Detected CPU Features\n");
        for f in &features {
            info.push_str(&format!("  +{}\n", f));
        }
    }

    info
}
