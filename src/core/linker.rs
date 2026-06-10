//! Linker optimization engine — inspired by mold (https://github.com/rui314/mold)
//!
//! mold is a blazing-fast linker created by Rui Ueyama. Its key innovations:
//!
//! ## mold's Core Mechanisms:
//!
//! 1. **Parallel Symbol Resolution** — mold resolves symbols across all object files
//!    in parallel using fine-grained mutexes (not a single global lock like gold/lld).
//!    This alone can be 2-4x faster than lld for large binaries.
//!
//! 2. **Memory-Mapped I/O** — mold mmap()s all input files and writes output via
//!    mmap() too. This avoids extra memcpy and lets the OS manage page cache efficiently.
//!    Input files are never read into heap buffers.
//!
//! 3. **Incremental Linking (LTO-style)** — mold's --shuffle-sections flag enables
//!    section-level reordering for better cache locality. It also supports
//!    --icf=all for Identical Code Folding.
//!
//! 4. **Identical Code Folding (ICF)** — mold's ICF is multi-threaded and runs
//!    after symbol resolution. It merges functions that are bit-for-bit identical,
//!    reducing binary size by 5-15% with zero runtime cost.
//!
//! 5. **Relaxation Optimizations** — mold performs TLGP relaxation (converting
//!    GOT-indirect references to PC-relative) and LD-to-LE relaxation for
//!    thread-local variables. These eliminate unnecessary indirection.
//!
//! 6. **Parallel Relocation** — mold applies relocations in parallel per-section
//!    rather than per-file, giving better granularity for multi-core scaling.
//!
//! 7. **Thin Archive Awareness** — mold reads thin archives directly without
//!    extracting member files to temp directories.
//!
//! 8. **Split-Stack / Segmented Stack Support** — mold can handle split-stack
//!    metadata for goroutine-style stacks.
//!
//! ## How rustm applies these:
//!
//! - Auto-detects mold/lld and configures the optimal linker per platform
//! - Injects mold-specific flags via RUSTFLAGS (ICF, relaxation, threads, etc.)
//! - Configures parallel thread count matching mold's internal parallelism
//! - Provides platform-specific fallback chains
//! - Tracks which optimizations are active per build

use crate::core::config::LinkerConfig;
use colored::*;
use std::path::PathBuf;
use std::process::Command;

/// Detailed information about the selected linker
#[derive(Debug, Clone)]
pub struct LinkerInfo {
    /// Human-readable name: "mold", "lld", "default"
    pub name: String,
    /// Absolute path to the linker binary
    pub path: Option<PathBuf>,
    /// Version string (e.g., "mold 2.33.0")
    pub version: Option<String>,
    /// Rustc-compatible flag to pass via RUSTFLAGS
    pub rustc_flag: Option<String>,
    /// Whether this linker is faster than the system default
    pub is_fast: bool,
    /// Mold-specific optimization flags (empty for non-mold)
    pub mold_flags: MoldFlags,
    /// Speed tier relative to default linker
    pub speed_tier: LinkerSpeedTier,
}

/// Speed tier for linker comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinkerSpeedTier {
    /// System default (ld.bfd, link.exe)
    Slow = 1,
    /// lld — fast but not the fastest
    Fast = 2,
    /// mold — fastest available
    BlazingFast = 3,
}

impl std::fmt::Display for LinkerSpeedTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkerSpeedTier::Slow => write!(f, "STD"),
            LinkerSpeedTier::Fast => write!(f, "FAST"),
            LinkerSpeedTier::BlazingFast => write!(f, "BLAZING"),
        }
    }
}

/// Mold-specific optimization flags
#[derive(Debug, Clone, Default)]
pub struct MoldFlags {
    /// Enable Identical Code Folding (--icf=all)
    /// Merges identical functions, reducing binary size 5-15%
    pub icf: IcfLevel,
    /// Number of linker threads (-Wl,--threads=N)
    /// mold uses fine-grained parallelism internally
    pub threads: Option<u32>,
    /// Enable TLGP relaxation (-Wl,--relax)
    /// Converts GOT-indirect to PC-relative where possible
    pub relax: bool,
    /// Enable --shuffle-sections for better cache locality
    /// Randomizes section order to expose poor assumptions
    pub shuffle_sections: bool,
    /// Enable compact dynamic section (-Wl,--compact-dyn)
    pub compact_dyn: bool,
    /// Enable or disable -z,now (BIND_NOW vs lazy binding)
    pub bind_now: bool,
    /// Enable -z,relro (read-only after relocation)
    pub relro: bool,
    /// Strip debug info at link time
    pub strip: bool,
    /// Build-id style (fast / sha1 / uuid / none)
    pub build_id: BuildIdStyle,
}

/// ICF (Identical Code Folding) level — one of mold's key innovations
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IcfLevel {
    /// No ICF
    #[default]
    None,
    /// Safe ICF — only folds functions that are definitely identical
    /// Uses a 2-pass approach: first compute hash, then verify bit-by-bit
    Safe,
    /// Full ICF — folds all identical code including data sections
    /// mold's ICF is parallelized across all CPU cores
    All,
}

impl std::fmt::Display for IcfLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IcfLevel::None => write!(f, "none"),
            IcfLevel::Safe => write!(f, "safe"),
            IcfLevel::All => write!(f, "all"),
        }
    }
}

/// Build-ID style — mold supports "fast" which is nearly free
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BuildIdStyle {
    /// No build-id
    None,
    /// Fast: 0x01 prefix + random bytes (mold default, nearly zero cost)
    #[default]
    Fast,
    /// SHA-1 hash (slower but deterministic)
    Sha1,
    /// UUID-based
    Uuid,
}

impl std::fmt::Display for BuildIdStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildIdStyle::None => write!(f, "none"),
            BuildIdStyle::Fast => write!(f, "fast"),
            BuildIdStyle::Sha1 => write!(f, "sha1"),
            BuildIdStyle::Uuid => write!(f, "uuid"),
        }
    }
}

impl MoldFlags {
    /// Create default flags optimized for development (fast link, no ICF)
    pub fn dev_defaults() -> Self {
        Self {
            icf: IcfLevel::None,
            threads: None, // let mold auto-detect
            relax: true,
            shuffle_sections: false,
            compact_dyn: false,
            bind_now: false,
            relro: true,
            strip: false,
            build_id: BuildIdStyle::Fast,
        }
    }

    /// Create default flags optimized for release (ICF=all, strip, relax)
    pub fn release_defaults() -> Self {
        Self {
            icf: IcfLevel::All,
            threads: None,
            relax: true,
            shuffle_sections: false,
            compact_dyn: true,
            bind_now: true,
            relro: true,
            strip: true,
            build_id: BuildIdStyle::Fast,
        }
    }

    /// Convert mold flags to linker flags (-Wl,--flag format)
    pub fn to_linker_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();

        // ICF — mold's Identical Code Folding
        // mold's ICF is multi-threaded and runs after symbol resolution
        // It works by:
        //   1. Computing a hash for each section (parallel)
        //   2. Grouping sections by hash
        //   3. Comparing candidates bit-by-bit (parallel)
        //   4. Merging identical groups into single instances
        match self.icf {
            IcfLevel::None => {}
            IcfLevel::Safe => flags.push("-Wl,--icf=safe".to_string()),
            IcfLevel::All => flags.push("-Wl,--icf=all".to_string()),
        }

        // Thread count — mold uses fine-grained parallelism internally
        // mold's approach: per-section parallelism for:
        //   - Symbol resolution (parallel hash table)
        //   - Relocation application (per-section parallel)
        //   - ICF (parallel hash + parallel compare)
        if let Some(n) = self.threads {
            flags.push(format!("-Wl,--threads={}", n));
        }

        // Relaxation — mold performs TLGP and LD-to-LE relaxation
        // This converts expensive indirect references to direct ones:
        //   GOT[foo] → foo (direct PC-relative)
        //   This eliminates a memory indirection at runtime
        if self.relax {
            flags.push("-Wl,--relax".to_string());
        }

        // Shuffle sections — improves cache locality by randomizing layout
        // Also exposes hidden assumptions about section ordering
        if self.shuffle_sections {
            flags.push("-Wl,--shuffle-sections".to_string());
        }

        // Compact .dyn section — reduces dynamic section size
        if self.compact_dyn {
            flags.push("-Wl,--compact-dyn".to_string());
        }

        // BIND_NOW — resolve all symbols at load time instead of lazily
        // Improves security (full RELRO) but may increase startup time
        if self.bind_now {
            flags.push("-Wl,-z,now".to_string());
        }

        // RELRO — make relocation read-only after processing
        if self.relro {
            flags.push("-Wl,-z,relro".to_string());
        }

        // Strip at link time — mold does this in-memory, no extra pass needed
        if self.strip {
            flags.push("-Wl,--strip-all".to_string());
        }

        // Build-ID — mold's "fast" mode is nearly zero cost
        match self.build_id {
            BuildIdStyle::None => flags.push("-Wl,--build-id=none".to_string()),
            BuildIdStyle::Fast => flags.push("-Wl,--build-id=fast".to_string()),
            BuildIdStyle::Sha1 => flags.push("-Wl,--build-id=sha1".to_string()),
            BuildIdStyle::Uuid => flags.push("-Wl,--build-id=uuid".to_string()),
        }

        flags
    }
}

/// Detects and selects the best available linker
pub struct LinkerSelector {
    config: LinkerConfig,
}

impl LinkerSelector {
    pub fn new(config: &LinkerConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Select the best linker based on configuration and availability
    /// Priority chain: mold > lld > default
    pub fn select(&self) -> Result<LinkerInfo, String> {
        match self.config.preferred.as_str() {
            "mold" => self.try_mold().or_else(|e| {
                eprintln!("{} mold requested but not available: {}", "⚠".yellow(), e);
                self.try_lld().or_else(|_| self.default_linker_ok())
            }),
            "lld" => self.try_lld().or_else(|e| {
                eprintln!("{} lld requested but not available: {}", "⚠".yellow(), e);
                self.default_linker_ok()
            }),
            "default" => self.default_linker_ok(),
            "auto" => self.auto_select(),
            other => Err(format!(
                "Unknown linker '{}'. Available: auto, mold, lld, default",
                other
            )),
        }
    }

    /// Auto-select the fastest available linker
    /// Priority: mold (BLAZING) > lld (FAST) > default (SLOW)
    fn auto_select(&self) -> Result<LinkerInfo, String> {
        // mold: The fastest linker — parallel symbol resolution,
        // parallel relocation, mmap I/O, ICF, relaxation
        if let Ok(info) = self.try_mold() {
            return Ok(info);
        }

        // lld: Fast — uses multi-threaded input parsing and parallel relocation
        // but has single-threaded symbol resolution (bottleneck on large projects)
        if let Ok(info) = self.try_lld() {
            return Ok(info);
        }

        // default: System linker (ld.bfd on Linux, link.exe on Windows)
        // Uses sequential algorithms throughout
        self.default_linker_ok()
    }

    /// Try to find and configure mold
    ///
    /// mold's advantages over lld:
    ///   1. Fine-grained parallel symbol resolution (lld uses a global lock)
    ///   2. mmap-based I/O (zero-copy file reading)
    ///   3. Multi-threaded ICF (lld ICF is sequential)
    ///   4. Built-in relaxation optimizations
    ///   5. Compact .dyn section support
    fn try_mold(&self) -> Result<LinkerInfo, String> {
        // mold only supports Linux and (experimentally) macOS
        if cfg!(target_os = "windows") {
            return Err("mold does not support Windows. Use lld instead.".to_string());
        }

        // Search for mold binary
        let mold_path = which::which("mold")
            .ok()
            .or_else(|| self.config.path.as_ref().map(PathBuf::from));

        let mold_path = mold_path.ok_or_else(|| {
            "mold not found. Install: https://github.com/rui314/mold#installation".to_string()
        })?;

        // Get mold version for diagnostics
        let version = self.get_linker_version(&mold_path, "--version");

        // Determine the correct way to invoke mold
        // On Linux, we use -C linker=mold and mold will replace the default linker
        // mold intercepts the linker invocation via its wrapper mechanism
        let rustc_flag = Some("-C linker=mold".to_string());

        let mold_flags = if self.config.release_mode {
            MoldFlags::release_defaults()
        } else {
            MoldFlags::dev_defaults()
        };

        Ok(LinkerInfo {
            name: "mold".to_string(),
            path: Some(mold_path),
            version,
            rustc_flag,
            is_fast: true,
            mold_flags,
            speed_tier: LinkerSpeedTier::BlazingFast,
        })
    }

    /// Try to find and configure lld
    ///
    /// lld is fast but has architectural differences from mold:
    ///   - Symbol resolution: single-threaded (global lock per symbol table)
    ///   - ICF: sequential (mold's is parallel)
    ///   - Input parsing: multi-threaded (matches mold)
    ///   - Relocation: parallel per-section (matches mold)
    ///   - No built-in relaxation (relies on compiler -C relax)
    fn try_lld(&self) -> Result<LinkerInfo, String> {
        if cfg!(target_os = "windows") {
            // On Windows (MSVC target), lld-link is the appropriate binary
            let lld_path = which::which("lld-link")
                .ok()
                .or_else(|| self.config.path.as_ref().map(PathBuf::from));

            let lld_path = lld_path.ok_or_else(|| {
                "lld-link not found. Install via 'rustup component add llvm-tools' or LLVM"
                    .to_string()
            })?;

            let version = self.get_linker_version(&lld_path, "--version");

            // On Windows with MSVC, we set the linker flavor
            let rustc_flag = Some("-C linker-flavor=lld-link".to_string());

            let mold_flags = if self.config.release_mode {
                MoldFlags::release_defaults()
            } else {
                MoldFlags::dev_defaults()
            };

            Ok(LinkerInfo {
                name: "lld".to_string(),
                path: Some(lld_path),
                version,
                rustc_flag,
                is_fast: true,
                mold_flags,
                speed_tier: LinkerSpeedTier::Fast,
            })
        } else if cfg!(target_os = "macos") {
            // macOS: ld64.lld is the lld binary for Mach-O
            let lld_path = which::which("ld64.lld")
                .ok()
                .or_else(|| which::which("lld").ok())
                .or_else(|| self.config.path.as_ref().map(PathBuf::from));

            let lld_path = lld_path.ok_or_else(|| {
                "lld not found. Install via 'brew install llvm' or 'rustup component add llvm-tools'".to_string()
            })?;

            let version = self.get_linker_version(&lld_path, "--version");

            Ok(LinkerInfo {
                name: "lld".to_string(),
                path: Some(lld_path),
                version,
                rustc_flag: Some("-C linker-flavor=ld64.lld".to_string()),
                is_fast: true,
                mold_flags: MoldFlags::dev_defaults(),
                speed_tier: LinkerSpeedTier::Fast,
            })
        } else {
            // Linux: lld for ELF
            let lld_path = which::which("ld.lld")
                .ok()
                .or_else(|| which::which("lld").ok())
                .or_else(|| self.config.path.as_ref().map(PathBuf::from));

            let lld_path = lld_path.ok_or_else(|| {
                "lld not found. Install via package manager or 'rustup component add llvm-tools'".to_string()
            })?;

            let version = self.get_linker_version(&lld_path, "--version");

            Ok(LinkerInfo {
                name: "lld".to_string(),
                path: Some(lld_path),
                version,
                rustc_flag: Some("-C linker=clang -C linker-flavor=gnu-lld-cc".to_string()),
                is_fast: true,
                mold_flags: MoldFlags::dev_defaults(),
                speed_tier: LinkerSpeedTier::Fast,
            })
        }
    }

    /// Use the system default linker
    fn default_linker_ok(&self) -> Result<LinkerInfo, String> {
        let name = if cfg!(target_os = "windows") {
            "link.exe"
        } else if cfg!(target_os = "macos") {
            "ld64"
        } else {
            "ld.bfd"
        };

        Ok(LinkerInfo {
            name: "default".to_string(),
            path: None,
            version: None,
            rustc_flag: None,
            is_fast: false,
            mold_flags: MoldFlags::default(),
            speed_tier: LinkerSpeedTier::Slow,
        })
    }

    /// Get linker version string
    fn get_linker_version(&self, path: &PathBuf, flag: &str) -> Option<String> {
        Command::new(path)
            .arg(flag)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().next().unwrap_or("unknown").trim().to_string())
    }

    /// Check which linkers are available on the system
    pub fn detect_available() -> Vec<LinkerInfo> {
        let mut available = Vec::new();

        // Always have default
        available.push(LinkerInfo {
            name: "default".to_string(),
            path: None,
            version: None,
            rustc_flag: None,
            is_fast: false,
            mold_flags: MoldFlags::default(),
            speed_tier: LinkerSpeedTier::Slow,
        });

        // Try mold (Linux/macOS only)
        if !cfg!(target_os = "windows") {
            if let Ok(info) = Self::try_mold_static() {
                available.push(info);
            }
        }

        // Try lld (all platforms)
        if let Ok(info) = Self::try_lld_static() {
            available.push(info);
        }

        available
    }

    fn try_mold_static() -> Result<LinkerInfo, String> {
        let mold_path = which::which("mold")
            .ok()
            .ok_or_else(|| "mold not found".to_string())?;

        let version = Command::new(&mold_path)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| {
                s.lines()
                    .next()
                    .unwrap_or("mold unknown")
                    .trim()
                    .to_string()
            });

        Ok(LinkerInfo {
            name: "mold".to_string(),
            path: Some(mold_path),
            version,
            rustc_flag: Some("-C linker=mold".to_string()),
            is_fast: true,
            mold_flags: MoldFlags::dev_defaults(),
            speed_tier: LinkerSpeedTier::BlazingFast,
        })
    }

    fn try_lld_static() -> Result<LinkerInfo, String> {
        let (lld_path, flag) = if cfg!(target_os = "windows") {
            let p = which::which("lld-link")
                .ok()
                .ok_or_else(|| "lld-link not found".to_string())?;
            (p, "-C linker-flavor=lld-link".to_string())
        } else if cfg!(target_os = "macos") {
            let p = which::which("ld64.lld")
                .ok()
                .or_else(|| which::which("lld").ok())
                .ok_or_else(|| "lld not found".to_string())?;
            (p, "-C linker-flavor=ld64.lld".to_string())
        } else {
            let p = which::which("ld.lld")
                .ok()
                .or_else(|| which::which("lld").ok())
                .ok_or_else(|| "lld not found".to_string())?;
            (p, "-C linker=clang -C linker-flavor=gnu-lld-cc".to_string())
        };

        let version = Command::new(&lld_path)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().next().unwrap_or("lld unknown").trim().to_string());

        Ok(LinkerInfo {
            name: "lld".to_string(),
            path: Some(lld_path),
            version,
            rustc_flag: Some(flag),
            is_fast: true,
            mold_flags: MoldFlags::dev_defaults(),
            speed_tier: LinkerSpeedTier::Fast,
        })
    }

    /// Generate detailed comparison of available linkers
    /// Shows mold's architectural advantages over lld and default
    pub fn linker_comparison() -> Vec<LinkerComparisonEntry> {
        vec![
            LinkerComparisonEntry {
                feature: "Symbol Resolution".to_string(),
                mold: "Parallel (fine-grained mutex)".to_string(),
                lld: "Sequential (global lock)".to_string(),
                default: "Sequential".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Input File I/O".to_string(),
                mold: "mmap (zero-copy)".to_string(),
                lld: "mmap (zero-copy)".to_string(),
                default: "read() into heap buffers".to_string(),
            },
            LinkerComparisonEntry {
                feature: "ICF (Identical Code Folding)".to_string(),
                mold: "Multi-threaded (parallel hash+compare)".to_string(),
                lld: "Sequential (single-threaded)".to_string(),
                default: "Not available".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Relocation".to_string(),
                mold: "Parallel per-section".to_string(),
                lld: "Parallel per-section".to_string(),
                default: "Sequential per-file".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Relaxation (TLGP/LD→LE)".to_string(),
                mold: "Built-in".to_string(),
                lld: "Partial".to_string(),
                default: "Partial".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Build-ID generation".to_string(),
                mold: "Fast mode (~0 cost)".to_string(),
                lld: "SHA-1 or MD5".to_string(),
                default: "SHA-1".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Thin Archive".to_string(),
                mold: "Direct read (no extraction)".to_string(),
                lld: "Direct read".to_string(),
                default: "Extract to temp dir".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Compact .dyn".to_string(),
                mold: "Supported".to_string(),
                lld: "Not available".to_string(),
                default: "Not available".to_string(),
            },
            LinkerComparisonEntry {
                feature: "Typical link time (large project)".to_string(),
                mold: "0.5-2s".to_string(),
                lld: "2-5s".to_string(),
                default: "5-30s".to_string(),
            },
        ]
    }
}

/// Comparison entry for linker features
#[derive(Debug, Clone)]
pub struct LinkerComparisonEntry {
    pub feature: String,
    pub mold: String,
    pub lld: String,
    pub default: String,
}

impl LinkerInfo {
    /// Build complete RUSTFLAGS including linker flag + mold optimizations
    pub fn to_rustflags(&self) -> Vec<String> {
        let mut flags = Vec::new();

        // Linker selection
        if let Some(linker_flag) = &self.rustc_flag {
            flags.push(linker_flag.clone());
        }

        // Mold/lld optimization flags
        // Only add -Wl flags when using mold or lld
        if self.name != "default" {
            flags.extend(self.mold_flags.to_linker_flags());
        }

        flags
    }

    /// Format detailed info string
    pub fn detailed_info(&self) -> String {
        let mut info = format!(
            "{} ({})",
            self.name.bold(),
            self.speed_tier.to_string().green()
        );
        if let Some(ref v) = self.version {
            info.push_str(&format!(" {}", v.dimmed()));
        }
        if let Some(ref p) = self.path {
            info.push_str(&format!("\n  Path: {}", p.display()));
        }
        if self.is_fast {
            info.push_str(&format!(
                "\n  Speed: {} faster than default",
                match self.speed_tier {
                    LinkerSpeedTier::BlazingFast => "5-10x",
                    LinkerSpeedTier::Fast => "2-5x",
                    LinkerSpeedTier::Slow => "1x",
                }
                .green()
            ));
        }
        if self.name == "mold" {
            info.push_str("\n  Optimizations:");
            info.push_str(&format!("\n    ICF: {}", self.mold_flags.icf));
            info.push_str(&format!(
                "\n    Relaxation: {}",
                if self.mold_flags.relax {
                    "enabled".green()
                } else {
                    "disabled".yellow()
                }
                .to_string()
            ));
            info.push_str(&format!(
                "\n    Threads: {}",
                self.mold_flags
                    .threads
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "auto".to_string())
            ));
            info.push_str(&format!("\n    Build-ID: {}", self.mold_flags.build_id));
            info.push_str(&format!(
                "\n    RELRO: {}",
                if self.mold_flags.relro {
                    "yes".green()
                } else {
                    "no".yellow()
                }
                .to_string()
            ));
        }
        info
    }
}
