//! Configuration management for rustm
//!
//! Handles loading/saving rustm.toml config files, merging with
//! CLI args, and providing sensible defaults for all settings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Main rustm configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RustmConfig {
    /// Build settings
    #[serde(default)]
    pub build: BuildConfig,

    /// Cache settings
    #[serde(default)]
    pub cache: CacheConfig,

    /// Linker settings (includes mold/lld optimization)
    #[serde(default)]
    pub linker: LinkerConfig,

    /// Parallel build settings
    #[serde(default)]
    pub parallel: ParallelConfig,

    /// Profile presets
    #[serde(default)]
    pub profiles: HashMap<String, ProfilePreset>,

    /// Environment variables to set during builds
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Default build profile name
    #[serde(default = "default_profile")]
    pub default_profile: String,

    /// Target triple (e.g., x86_64-unknown-linux-gnu)
    #[serde(default)]
    pub target: Option<String>,

    /// Number of codegen units (lower = slower compile, faster runtime)
    #[serde(default)]
    pub codegen_units: Option<u32>,

    /// Enable incremental compilation
    #[serde(default = "default_true")]
    pub incremental: bool,

    /// Strip debug symbols in release
    #[serde(default = "default_true")]
    pub strip: bool,

    /// LTO setting: "none", "thin", "fat"
    #[serde(default = "default_lto")]
    pub lto: String,

    /// Opt-level: 0-3, "s", "z"
    #[serde(default)]
    pub opt_level: Option<String>,

    /// Panic strategy: "unwind" or "abort"
    #[serde(default)]
    pub panic: Option<String>,

    /// Debug info level: 0-2 or "line-tables-only"
    #[serde(default)]
    pub debug: Option<String>,

    /// Rust flags to append
    #[serde(default)]
    pub rustflags: Vec<String>,

    /// Cargo features to enable
    #[serde(default)]
    pub features: Vec<String>,

    /// All features
    #[serde(default)]
    pub all_features: bool,

    /// No default features
    #[serde(default)]
    pub no_default_features: bool,

    /// Codegen backend: "auto", "cranelift", "llvm"
    /// auto = Cranelift for dev, LLVM for release
    #[serde(default = "default_codegen_backend")]
    pub codegen_backend: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            default_profile: default_profile(),
            target: None,
            codegen_units: None,
            incremental: true,
            strip: true,
            lto: default_lto(),
            opt_level: None,
            panic: None,
            debug: None,
            rustflags: Vec::new(),
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            codegen_backend: default_codegen_backend(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable sccache integration
    #[serde(default = "default_true")]
    pub sccache: bool,

    /// Custom cache directory
    #[serde(default)]
    pub dir: Option<String>,

    /// Cache size limit in GB
    #[serde(default = "default_cache_size")]
    pub max_size_gb: u32,

    /// Enable local artifact caching
    #[serde(default = "default_true")]
    pub local_cache: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            sccache: true,
            dir: None,
            max_size_gb: default_cache_size(),
            local_cache: true,
        }
    }
}

/// Linker configuration — includes mold-specific optimization settings
///
/// mold (https://github.com/rui314/mold) is the fastest available linker.
/// Its key architectural advantages:
///   - Parallel symbol resolution (fine-grained mutex vs global lock in lld)
///   - mmap-based I/O (zero-copy file reading)
///   - Multi-threaded ICF (Identical Code Folding)
///   - Built-in TLGP and LD→LE relaxation
///   - Compact .dyn section support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkerConfig {
    /// Preferred linker: "auto", "mold", "lld", "default"
    #[serde(default = "default_linker")]
    pub preferred: String,

    /// Custom linker path
    #[serde(default)]
    pub path: Option<String>,

    /// Use release-mode linker optimizations (ICF, strip, RELRO, etc.)
    #[serde(default)]
    pub release_mode: bool,

    /// ICF level: "none", "safe", "all"
    /// mold's ICF is multi-threaded and can reduce binary size 5-15%
    #[serde(default = "default_icf")]
    pub icf: String,

    /// Enable linker relaxation (TLGP, LD→LE)
    /// Converts GOT-indirect to PC-relative where possible
    #[serde(default = "default_true")]
    pub relax: bool,

    /// Number of linker threads (0 = auto-detect from CPU count)
    /// mold uses fine-grained parallelism: per-section symbol resolution,
    /// per-section relocation, parallel ICF
    #[serde(default)]
    pub threads: u32,

    /// Build-ID style: "none", "fast", "sha1", "uuid"
    /// mold's "fast" mode is nearly zero cost
    #[serde(default = "default_build_id")]
    pub build_id: String,

    /// Enable compact .dyn section (mold only)
    #[serde(default)]
    pub compact_dyn: bool,

    /// Enable full RELRO (-z,relro) for security
    #[serde(default = "default_true")]
    pub relro: bool,

    /// Enable BIND_NOW (-z,now) for full RELRO
    #[serde(default)]
    pub bind_now: bool,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            preferred: default_linker(),
            path: None,
            release_mode: false,
            icf: default_icf(),
            relax: true,
            threads: 0,
            build_id: default_build_id(),
            compact_dyn: false,
            relro: true,
            bind_now: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    /// Max parallel jobs (0 = auto-detect from CPU cores)
    #[serde(default)]
    pub jobs: u32,

    /// Parallelize codegen units
    #[serde(default = "default_true")]
    pub parallel_codegen: bool,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            jobs: 0,
            parallel_codegen: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePreset {
    /// LTO setting
    #[serde(default)]
    pub lto: Option<String>,

    /// Codegen units
    #[serde(default)]
    pub codegen_units: Option<u32>,

    /// Opt level
    #[serde(default)]
    pub opt_level: Option<String>,

    /// Strip
    #[serde(default)]
    pub strip: Option<bool>,

    /// Debug info
    #[serde(default)]
    pub debug: Option<String>,

    /// Panic strategy
    #[serde(default)]
    pub panic: Option<String>,

    /// Incremental
    #[serde(default)]
    pub incremental: Option<bool>,

    /// Extra rustflags
    #[serde(default)]
    pub rustflags: Vec<String>,
}

fn default_profile() -> String {
    "fastest".to_string()
}
fn default_true() -> bool {
    true
}
fn default_lto() -> String {
    "thin".to_string()
}

fn default_codegen_backend() -> String {
    "auto".to_string()
}
fn default_cache_size() -> u32 {
    10
}
fn default_linker() -> String {
    "auto".to_string()
}
fn default_icf() -> String {
    "safe".to_string()
}
fn default_build_id() -> String {
    "fast".to_string()
}

impl RustmConfig {
    /// Load config from the project directory
    pub fn load(project_dir: &Path) -> Self {
        let config_path = project_dir.join("rustm.toml");
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("Warning: Failed to parse rustm.toml: {}", e);
                        eprintln!("  Using default configuration.");
                    }
                },
                Err(e) => {
                    eprintln!("Warning: Failed to read rustm.toml: {}", e);
                }
            }
        }

        // Also check global config
        if let Some(home) = dirs::home_dir() {
            let global_path = home.join(".rustm").join("config.toml");
            if global_path.exists()
                && let Ok(content) = fs::read_to_string(&global_path)
                && let Ok(config) = toml::from_str(&content)
            {
                return config;
            }
        }

        Self::default()
    }

    /// Save config to the project directory
    pub fn save(&self, project_dir: &Path) -> Result<(), String> {
        let config_path = project_dir.join("rustm.toml");
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&config_path, content).map_err(|e| format!("Failed to write rustm.toml: {}", e))
    }

    /// Generate default config file with mold optimization documentation
    pub fn generate_default() -> String {
        let config = Self::default();
        let mut doc = String::new();

        doc.push_str("# rustm configuration\n");
        doc.push_str("# https://github.com/rui314/mold — fastest linker\n");
        doc.push_str("#\n");
        doc.push_str("# mold's key optimizations:\n");
        doc.push_str("#   - Parallel symbol resolution (fine-grained mutex)\n");
        doc.push_str("#   - mmap-based I/O (zero-copy file reading)\n");
        doc.push_str("#   - Multi-threaded ICF (Identical Code Folding)\n");
        doc.push_str("#   - TLGP & LD→LE relaxation (eliminates GOT indirection)\n");
        doc.push_str("#   - Compact .dyn section support\n");
        doc.push_str("#   - Fast build-ID (~zero cost)\n\n");

        doc.push_str("# Build optimization settings\n");
        doc.push_str("[build]\n");
        doc.push_str(&format!(
            "default_profile = \"{}\"\n",
            config.build.default_profile
        ));
        doc.push_str(&format!("incremental = {}\n", config.build.incremental));
        doc.push_str(&format!("strip = {}\n", config.build.strip));
        doc.push_str(&format!("lto = \"{}\"\n", config.build.lto));
        doc.push_str("# target = \"x86_64-unknown-linux-gnu\"\n");
        doc.push_str("# codegen_units = 16\n");
        doc.push_str("# rustflags = [\"-C\", \"target-cpu=native\"]\n\n");

        doc.push_str("# Cache settings\n");
        doc.push_str("[cache]\n");
        doc.push_str(&format!("sccache = {}\n", config.cache.sccache));
        doc.push_str(&format!("max_size_gb = {}\n", config.cache.max_size_gb));
        doc.push_str(&format!("local_cache = {}\n\n", config.cache.local_cache));

        doc.push_str("# Linker settings (mold optimization)\n");
        doc.push_str("# mold is 5-10x faster than default ld, 2-3x faster than lld\n");
        doc.push_str("# It achieves this through:\n");
        doc.push_str("#   1. Parallel symbol resolution (no global lock like lld)\n");
        doc.push_str("#   2. mmap I/O (zero-copy, no heap buffering)\n");
        doc.push_str("#   3. Multi-threaded ICF (parallel hash + compare)\n");
        doc.push_str("#   4. Built-in TLGP relaxation\n\n");
        doc.push_str("[linker]\n");
        doc.push_str(&format!("preferred = \"{}\"\n", config.linker.preferred));
        doc.push_str(&format!(
            "# release_mode = {}\n",
            config.linker.release_mode
        ));
        doc.push_str(&format!("icf = \"{}\"\n", config.linker.icf));
        doc.push_str(&format!("relax = {}\n", config.linker.relax));
        doc.push_str(&format!("threads = {}\n", config.linker.threads));
        doc.push_str(&format!("build_id = \"{}\"\n", config.linker.build_id));
        doc.push_str(&format!("compact_dyn = {}\n", config.linker.compact_dyn));
        doc.push_str(&format!("relro = {}\n", config.linker.relro));
        doc.push_str(&format!("bind_now = {}\n\n", config.linker.bind_now));

        doc.push_str("# Parallel build settings\n");
        doc.push_str("[parallel]\n");
        doc.push_str("# jobs = 0  # 0 = auto-detect\n\n");

        // Add profile presets
        doc.push_str("# Profile presets\n");
        doc.push_str("#\n");
        doc.push_str("# fastest:     Maximum optimization stack (DEFAULT) - target-cpu=native + thin LTO + ICF\n");
        doc.push_str("# dev-fast:     Fastest dev iteration (codegen-units=256, no LTO)\n");
        doc.push_str("# balanced:     Good tradeoff (thin LTO, opt-level=2)\n");
        doc.push_str("# dev-check:   Fastest cargo check (no debug info)\n");
        doc.push_str(
            "# release-max:  Maximum performance (fat LTO, codegen-units=1, mold ICF=all)\n",
        );
        doc.push_str("# size-opt:     Minimal binary (opt-level=z, ICF=all)\n\n");

        doc.push_str("[profiles.dev-fast]\n");
        doc.push_str("lto = \"none\"\n");
        doc.push_str("codegen_units = 256\n");
        doc.push_str("opt_level = \"0\"\n");
        doc.push_str("debug = \"2\"\n");
        doc.push_str("incremental = true\n\n");

        doc.push_str("[profiles.release-max]\n");
        doc.push_str("lto = \"fat\"\n");
        doc.push_str("codegen_units = 1\n");
        doc.push_str("opt_level = \"3\"\n");
        doc.push_str("strip = true\n");
        doc.push_str("panic = \"abort\"\n");
        doc.push_str("incremental = false\n");
        doc.push_str("rustflags = [\"-C\", \"target-cpu=native\"]\n\n");

        doc.push_str("[profiles.release-fast]\n");
        doc.push_str("lto = \"thin\"\n");
        doc.push_str("codegen_units = 16\n");
        doc.push_str("opt_level = \"3\"\n");
        doc.push_str("strip = true\n");
        doc.push_str("incremental = false\n\n");

        doc.push_str("[profiles.size-opt]\n");
        doc.push_str("lto = \"thin\"\n");
        doc.push_str("codegen_units = 1\n");
        doc.push_str("opt_level = \"z\"\n");
        doc.push_str("strip = true\n");
        doc.push_str("panic = \"abort\"\n");

        doc
    }

    /// Find the project root directory (where Cargo.toml is)
    pub fn find_project_root() -> Option<PathBuf> {
        let mut current = std::env::current_dir().ok()?;
        loop {
            if current.join("Cargo.toml").exists() {
                return Some(current);
            }
            if !current.pop() {
                return None;
            }
        }
    }
}
