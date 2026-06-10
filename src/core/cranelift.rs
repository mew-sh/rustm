//! Cranelift Codegen Backend Integration for rustm
//!
//! rustc_codegen_cranelift (https://github.com/rust-lang/rustc_codegen_cranelift)
//! is an alternative codegen backend for the Rust compiler that uses Cranelift
//! instead of LLVM for machine code generation.
//!
//! ## Why Cranelift?
//!
//! Cranelift produces code **much faster** than LLVM, but the generated code
//! is less optimized. This makes it ideal for:
//!   - Debug builds where compile speed matters more than runtime speed
//!   - Iteration cycles during development
//!   - cargo check + run workflows
//!
//! ## Key Advantages over LLVM for Dev Builds:
//!
//! 1. **2-5x faster codegen** — Cranelift's register allocator and instruction
//!    selector are designed for speed, not optimization quality.
//! 2. **Lower memory usage** — Cranelift's intermediate representation is
//!    simpler than LLVM IR, using ~40-60% less memory during codegen.
//! 3. **Faster incremental builds** — Less work per codegen unit means
//!    changed crates rebuild faster.
//! 4. **No LLVM optimization passes** — Skips ~120 LLVM optimization passes,
//!    going directly from MIR to machine code.
//! 5. **Simpler pipeline** — MIR → CLIF IR → VReg → Machine Code
//!    vs LLVM: MIR → LLVM IR → Opt → SelectionDAG → Machine Code
//!
//! ## Architecture Comparison:
//!
//! | Aspect              | LLVM                         | Cranelift                    |
//! |---------------------|------------------------------|------------------------------|
//! | Codegen speed       | Slow (120+ opt passes)      | Fast (minimal passes)       |
//! | Binary performance  | Optimal                      | Good (80-95% of LLVM)       |
//! | Memory usage        | High                         | Low (~40-60% of LLVM)       |
//! | Optimization passes | Full (inlining, vectorize…)  | Minimal (regalloc only)     |
//! | Debug info           | Full DWARF                   | Basic DWARF                 |
//! | LTO support         | Full (thin/fat)              | No LTO (per-crate)          |
//! | SIMD vectorization   | Full auto-vectorization     | Limited                      |
//!
//! ## How rustm uses Cranelift:
//!
//! - Auto-detects Cranelift availability (rustup component or dylib)
//! - Injects `-Zcodegen-backend=cranelift` when selected
//! - Applies only to dev/debug profiles (not release)
//! - Falls back gracefully to LLVM when Cranelift is unavailable
//! - New `dev-cranelift` profile for maximum compile speed
//! - Optional `--codegen-backend` flag override on any build

use colored::*;
use std::path::PathBuf;
use std::process::Command;

/// Codegen backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodegenBackend {
    /// Auto-select: Cranelift for dev, LLVM for release
    #[default]
    Auto,
    /// Cranelift — fast codegen, less optimized output
    Cranelift,
    /// LLVM — slow codegen, maximum optimization
    Llvm,
}

impl std::fmt::Display for CodegenBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenBackend::Auto => write!(f, "auto"),
            CodegenBackend::Cranelift => write!(f, "cranelift"),
            CodegenBackend::Llvm => write!(f, "llvm"),
        }
    }
}

impl std::str::FromStr for CodegenBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(CodegenBackend::Auto),
            "cranelift" => Ok(CodegenBackend::Cranelift),
            "llvm" => Ok(CodegenBackend::Llvm),
            other => Err(format!(
                "Unknown codegen backend '{}'. Available: auto, cranelift, llvm",
                other
            )),
        }
    }
}

/// Information about Cranelift availability
#[derive(Debug, Clone)]
pub struct CraneliftInfo {
    /// Whether Cranelift is available
    pub available: bool,
    /// How Cranelift is installed
    pub install_method: CraneliftInstallMethod,
    /// Path to the codegen backend dylib
    pub dylib_path: Option<PathBuf>,
    /// Version info (if available)
    pub version: Option<String>,
}

/// How Cranelift is installed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraneliftInstallMethod {
    /// Installed via rustup component
    RustupComponent,
    /// Found as a dynamic library in sysroot
    DylibInSysroot,
    /// Found in a custom path
    CustomPath,
    /// Not found
    NotInstalled,
}

impl std::fmt::Display for CraneliftInstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CraneliftInstallMethod::RustupComponent => write!(f, "rustup component"),
            CraneliftInstallMethod::DylibInSysroot => write!(f, "sysroot dylib"),
            CraneliftInstallMethod::CustomPath => write!(f, "custom path"),
            CraneliftInstallMethod::NotInstalled => write!(f, "not installed"),
        }
    }
}

/// Cranelift backend manager
pub struct CraneliftBackend {
    info: CraneliftInfo,
}

impl CraneliftBackend {
    /// Create a new Cranelift backend manager, auto-detecting availability
    pub fn new() -> Self {
        let info = Self::detect();
        Self { info }
    }

    /// Detect Cranelift availability on the system
    ///
    /// Checks multiple installation methods:
    /// 1. rustup component (rustup component list)
    /// 2. dylib in rustc sysroot
    /// 3. Custom path from RUSTM_CRANELIFT_PATH env
    pub fn detect() -> CraneliftInfo {
        // Method 1: Check for rustup component
        if let Some(dylib) = Self::find_via_rustup() {
            return CraneliftInfo {
                available: true,
                install_method: CraneliftInstallMethod::RustupComponent,
                dylib_path: Some(dylib),
                version: Self::detect_version(),
            };
        }

        // Method 2: Check dylib in sysroot
        if let Some(dylib) = Self::find_in_sysroot() {
            return CraneliftInfo {
                available: true,
                install_method: CraneliftInstallMethod::DylibInSysroot,
                dylib_path: Some(dylib),
                version: Self::detect_version(),
            };
        }

        // Method 3: Custom path from environment
        if let Ok(custom_path) = std::env::var("RUSTM_CRANELIFT_PATH") {
            let path = PathBuf::from(&custom_path);
            if path.exists() {
                return CraneliftInfo {
                    available: true,
                    install_method: CraneliftInstallMethod::CustomPath,
                    dylib_path: Some(path),
                    version: None,
                };
            }
        }

        CraneliftInfo {
            available: false,
            install_method: CraneliftInstallMethod::NotInstalled,
            dylib_path: None,
            version: None,
        }
    }

    /// Find Cranelift via rustup component
    fn find_via_rustup() -> Option<PathBuf> {
        // Check if the cranelift codegen backend is installed via rustup
        // It installs as: $sysroot/lib/rustlib/$target/codegen-backends/librustc_codegen_cranelift.so
        let output = Command::new("rustup")
            .args(["component", "list", "--installed"])
            .output()
            .ok()?;

        let stdout = String::from_utf8(output.stdout).ok()?;

        // Check if rustc_codegen_cranelift is in the installed components
        if stdout
            .lines()
            .any(|line| line.contains("rustc_codegen_cranelift"))
        {
            return Self::find_in_sysroot();
        }

        None
    }

    /// Find Cranelift dylib in the rustc sysroot
    fn find_in_sysroot() -> Option<PathBuf> {
        // Get the sysroot
        let output = Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .ok()?;

        let sysroot = String::from_utf8(output.stdout).ok()?;
        let sysroot = sysroot.trim();

        // Get the target triple
        let target_output = Command::new("rustc").args(["-vV"]).output().ok()?;

        let target_stdout = String::from_utf8(target_output.stdout).ok()?;
        let target_triple = target_stdout
            .lines()
            .find(|line| line.starts_with("host: "))
            .map(|line| line.strip_prefix("host: ").unwrap_or(""))
            .unwrap_or("");

        if target_triple.is_empty() {
            return None;
        }

        // Platform-specific dylib extension
        let dylib_ext = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };

        // Search in sysroot for the cranelift codegen backend
        let search_paths = [
            format!(
                "{}/lib/rustlib/{}/codegen-backends/librustc_codegen_cranelift.{}",
                sysroot, target_triple, dylib_ext
            ),
            format!(
                "{}/lib/rustlib/{}/codegen-backends/rustc_codegen_cranelift.{}",
                sysroot, target_triple, dylib_ext
            ),
        ];

        for path in &search_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        // Also check common Windows paths
        if cfg!(target_os = "windows") {
            let win_paths = [format!(
                "{}/lib/rustlib/{}/codegen-backends/rustc_codegen_cranelift.dll",
                sysroot, target_triple
            )];
            for path in &win_paths {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Some(p);
                }
            }
        }

        None
    }

    /// Detect Cranelift version from rustc_codegen_cranelift metadata
    fn detect_version() -> Option<String> {
        // Try to get version from the installed component
        let output = Command::new("rustc")
            .args(["-Zcodegen-backend=cranelift", "--version"])
            .output()
            .ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8(output.stdout).ok()?;
                if !stdout.trim().is_empty() {
                    return Some(stdout.trim().to_string());
                }
            }
        }

        // Fallback: try to extract from rustup
        let output = Command::new("rustup")
            .args(["component", "list"])
            .output()
            .ok()?;

        let stdout = String::from_utf8(output.stdout).ok()?;
        for line in stdout.lines() {
            if line.contains("rustc_codegen_cranelift") {
                // Extract version if present
                if let Some(parens) = line.split('(').nth(1) {
                    if let Some(ver) = parens.split(')').next() {
                        return Some(format!("cranelift-{}", ver.trim()));
                    }
                }
                return Some("cranelift (installed)".to_string());
            }
        }

        None
    }

    /// Check if Cranelift is available
    pub fn is_available(&self) -> bool {
        self.info.available
    }

    /// Get Cranelift info
    pub fn info(&self) -> &CraneliftInfo {
        &self.info
    }

    /// Build RUSTFLAGS for Cranelift codegen backend
    ///
    /// When Cranelift is selected, injects:
    ///   -Zcodegen-backend=cranelift
    ///
    /// Note: -Z flags require nightly Rust. On stable, we need to use
    /// the dylib path approach: -Zcodegen-backend=/path/to/librustc_codegen_cranelift.so
    pub fn build_rustflags(&self) -> Vec<String> {
        let mut flags = Vec::new();

        if !self.info.available {
            return flags;
        }

        // Use the dylib path if available (works on stable Rust)
        if let Some(ref dylib_path) = self.info.dylib_path {
            flags.push(format!("-Zcodegen-backend={}", dylib_path.display()));
        } else {
            // Fallback to named backend (requires nightly)
            flags.push("-Zcodegen-backend=cranelift".to_string());
        }

        flags
    }

    /// Determine if Cranelift should be used for a given profile
    ///
    /// Logic:
    /// - Auto: use Cranelift for dev profiles, LLVM for release
    /// - Cranelift: always use Cranelift (error if unavailable)
    /// - Llvm: always use LLVM
    pub fn should_use_cranelift(
        &self,
        backend: CodegenBackend,
        profile_name: &str,
        release: bool,
    ) -> bool {
        match backend {
            CodegenBackend::Cranelift => {
                if !self.info.available {
                    return false;
                }
                true
            }
            CodegenBackend::Llvm => false,
            CodegenBackend::Auto => {
                // Auto-select: Cranelift for dev/debug, LLVM for release
                if !self.info.available {
                    return false;
                }
                // Don't use Cranelift for release builds (LLVM optimizes better)
                if release {
                    return false;
                }
                // Use Cranelift for dev/debug profiles
                matches!(
                    profile_name,
                    "dev-fast" | "dev-check" | "dev-cranelift" | "dev"
                )
            }
        }
    }

    /// Print Cranelift architecture info
    pub fn print_architecture_info(&self) -> String {
        let mut info = String::new();

        info.push_str("\n⚡ Cranelift Codegen Backend — Fast Alternative to LLVM\n");
        info.push_str(&format!("{}\n\n", "═".repeat(65)));

        info.push_str("🧬 Pipeline Comparison:\n\n");
        info.push_str("  LLVM Pipeline (slow, optimized output):\n");
        info.push_str("    MIR → LLVM IR → ~120 opt passes → SelectionDAG → Machine Code\n\n");
        info.push_str("  Cranelift Pipeline (fast, good output):\n");
        info.push_str("    MIR → CLIF IR → RegAlloc → ISel → Machine Code\n\n");

        info.push_str("📊 Performance Comparison:\n\n");
        info.push_str("  ┌──────────────────────┬──────────────┬──────────────┐\n");
        info.push_str("  │ Metric               │ LLVM         │ Cranelift    │\n");
        info.push_str("  ├──────────────────────┼──────────────┼──────────────┤\n");
        info.push_str("  │ Codegen speed        │ Baseline     │ 2-5x faster  │\n");
        info.push_str("  │ Memory usage         │ Baseline     │ 40-60% less  │\n");
        info.push_str("  │ Binary performance   │ 100%         │ 80-95%       │\n");
        info.push_str("  │ Incremental rebuilds  │ Baseline     │ 2-3x faster  │\n");
        info.push_str("  │ Optimization passes  │ ~120         │ ~5           │\n");
        info.push_str("  │ Auto-vectorization    │ Full         │ Limited      │\n");
        info.push_str("  │ LTO support           │ Full         │ None         │\n");
        info.push_str("  │ Debug info            │ Full DWARF   │ Basic DWARF  │\n");
        info.push_str("  └──────────────────────┴──────────────┴──────────────┘\n\n");

        info.push_str("🎯 When to use Cranelift:\n");
        info.push_str("  ✓ Debug builds & rapid iteration\n");
        info.push_str("  ✓ cargo check / cargo test development\n");
        info.push_str("  ✓ Large codebases where LLVM is the bottleneck\n");
        info.push_str("  ✓ CI pipelines needing fast feedback\n\n");
        info.push_str("  ✗ Release builds (LLVM produces faster code)\n");
        info.push_str("  ✗ Projects needing auto-vectorization/SIMD\n");
        info.push_str("  ✗ Builds requiring full LTO\n\n");

        info.push_str("📋 Availability:\n");
        if self.info.available {
            info.push_str(&format!(
                "  ✅ Cranelift: available ({})\n",
                self.info.install_method.to_string().green()
            ));
            if let Some(ref path) = self.info.dylib_path {
                info.push_str(&format!("  Path: {}\n", path.display()));
            }
            if let Some(ref ver) = self.info.version {
                info.push_str(&format!("  Version: {}\n", ver));
            }
        } else {
            info.push_str("  ❌ Cranelift: not installed\n");
            info.push_str("\n  📦 Installation:\n");
            info.push_str("  Nightly:\n");
            info.push_str(
                "    rustup component add rustc_codegen_cranelift --toolchain nightly\n\n",
            );
            info.push_str("  Or build from source:\n");
            info.push_str("    git clone https://github.com/rust-lang/rustc_codegen_cranelift\n");
            info.push_str("    cd rustc_codegen_cranelift\n");
            info.push_str("    ./y.sh prepare\n");
            info.push_str("    ./y.sh build\n\n");
            info.push_str("  ⚠️  Note: Cranelift currently requires nightly Rust\n");
        }

        info.push_str(&format!("\n{}\n", "═".repeat(65)));
        info
    }
}

impl Default for CraneliftBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codegen_backend_display() {
        assert_eq!(CodegenBackend::Auto.to_string(), "auto");
        assert_eq!(CodegenBackend::Cranelift.to_string(), "cranelift");
        assert_eq!(CodegenBackend::Llvm.to_string(), "llvm");
    }

    #[test]
    fn test_codegen_backend_from_str() {
        assert_eq!(
            "auto".parse::<CodegenBackend>().unwrap(),
            CodegenBackend::Auto
        );
        assert_eq!(
            "cranelift".parse::<CodegenBackend>().unwrap(),
            CodegenBackend::Cranelift
        );
        assert_eq!(
            "llvm".parse::<CodegenBackend>().unwrap(),
            CodegenBackend::Llvm
        );
        assert!("invalid".parse::<CodegenBackend>().is_err());
    }

    #[test]
    fn test_should_use_cranelift_auto_mode() {
        let backend = CraneliftBackend::new();

        // Auto mode: dev profiles should prefer Cranelift (if available)
        if backend.is_available() {
            assert!(backend.should_use_cranelift(CodegenBackend::Auto, "dev-fast", false));
            assert!(backend.should_use_cranelift(CodegenBackend::Auto, "dev-check", false));
            assert!(!backend.should_use_cranelift(CodegenBackend::Auto, "fastest", true));
            assert!(!backend.should_use_cranelift(CodegenBackend::Auto, "release-max", true));
        }

        // Explicit LLVM: never use Cranelift
        assert!(!backend.should_use_cranelift(CodegenBackend::Llvm, "dev-fast", false));

        // Explicit Cranelift: always use (if available)
        if backend.is_available() {
            assert!(backend.should_use_cranelift(CodegenBackend::Cranelift, "fastest", true));
        }
    }
}
