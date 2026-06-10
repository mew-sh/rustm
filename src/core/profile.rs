//! Build profile resolution
//!
//! Maps rustm profile names to actual compilation settings.
//! Provides built-in presets and supports custom profiles from config.
//!
//! Default profile is "fastest" — the maximum optimization stack:
//!   target-cpu=native + target-feature auto-detect + thin LTO +
//!   codegen-units=16 + panic=abort + strip + mold ICF=safe + relaxation

use crate::core::config::ProfilePreset;
use std::collections::HashMap;

/// Built-in profile presets
///
/// The "fastest" profile is the default — it stacks all available optimizations:
///   - target-cpu=native (AVX2/FMA/NEON auto-detect)
///   - thin LTO (cross-module optimization)
///   - codegen-units=16 (parallel + good optimization)
///   - panic=abort (smaller binary, no unwind tables)
///   - strip=symbols (smaller binary)
///   - mold ICF=safe (merge identical functions)
///   - mold relaxation (GOT→PC-relative)
pub fn builtin_profiles() -> HashMap<String, ProfilePreset> {
    let mut profiles = HashMap::new();

    // ═══════════════════════════════════════════════════════
    // FASTEST — Maximum optimization stack (DEFAULT)
    // ═══════════════════════════════════════════════════════
    // target-cpu=native:     enables all CPU features (AVX2, FMA, SSE4.2, etc.)
    // target-feature auto:   detected CPU features for max SIMD
    // LTO=thin:              cross-module optimization, fast compile
    // codegen-units=16:      parallel codegen + good optimization
    // panic=abort:           smaller binary, no unwind overhead
    // strip=symbols:         smaller binary
    // ICF=safe:              merge identical functions (mold)
    // relaxation:            GOT→PC-relative (mold)
    profiles.insert(
        "fastest".to_string(),
        ProfilePreset {
            lto: Some("thin".to_string()),
            codegen_units: Some(16),
            opt_level: Some("3".to_string()),
            strip: Some(true),
            debug: None,
            panic: Some("abort".to_string()),
            incremental: Some(false),
            rustflags: vec!["-C target-cpu=native".to_string()],
        },
    );

    // ═══════════════════════════════════════════════════════
    // DEV-FAST — Fastest compile, no optimizations
    // ═══════════════════════════════════════════════════════
    profiles.insert(
        "dev-fast".to_string(),
        ProfilePreset {
            lto: Some("none".to_string()),
            codegen_units: Some(256),
            opt_level: Some("0".to_string()),
            strip: Some(false),
            debug: Some("2".to_string()),
            panic: None,
            incremental: Some(true),
            rustflags: vec![],
        },
    );

    // ═══════════════════════════════════════════════════════════
    // DEV-CRANELIFT — Ultra-fast compile with Cranelift backend
    // ═══════════════════════════════════════════════════════════
    // Uses Cranelift instead of LLVM for 2-5x faster codegen.
    // Ideal for rapid iteration in development.
    // Binary performance: ~80-95% of LLVM, but compiles much faster.
    // Requires: rustup component add rustc_codegen_cranelift --toolchain nightly
    profiles.insert(
        "dev-cranelift".to_string(),
        ProfilePreset {
            lto: Some("none".to_string()),
            codegen_units: Some(256),
            opt_level: Some("0".to_string()),
            strip: Some(false),
            debug: Some("2".to_string()),
            panic: None,
            incremental: Some(true),
            rustflags: vec![], // Cranelift backend is injected by engine
        },
    );

    // DEV-CHECK — Fastest cargo check (no codegen)
    profiles.insert(
        "dev-check".to_string(),
        ProfilePreset {
            lto: Some("none".to_string()),
            codegen_units: Some(256),
            opt_level: Some("0".to_string()),
            strip: Some(false),
            debug: Some("0".to_string()),
            panic: None,
            incremental: Some(true),
            rustflags: vec![],
        },
    );

    // ═══════════════════════════════════════════════════════
    // RELEASE-FAST — Fast release with thin LTO
    // ═══════════════════════════════════════════════════════
    profiles.insert(
        "release-fast".to_string(),
        ProfilePreset {
            lto: Some("thin".to_string()),
            codegen_units: Some(16),
            opt_level: Some("3".to_string()),
            strip: Some(true),
            debug: None,
            panic: Some("abort".to_string()),
            incremental: Some(false),
            rustflags: vec!["-C target-cpu=native".to_string()],
        },
    );

    // ═══════════════════════════════════════════════════════
    // RELEASE-MAX — Maximum runtime performance
    // ═══════════════════════════════════════════════════════
    // Uses fat LTO + codegen-units=1 for absolute best runtime
    // Slowest to compile, but fastest binary
    profiles.insert(
        "release-max".to_string(),
        ProfilePreset {
            lto: Some("fat".to_string()),
            codegen_units: Some(1),
            opt_level: Some("3".to_string()),
            strip: Some(true),
            debug: None,
            panic: Some("abort".to_string()),
            incremental: Some(false),
            rustflags: vec!["-C target-cpu=native".to_string()],
        },
    );

    // ═══════════════════════════════════════════════════════
    // SIZE-OPT — Minimal binary size
    // ═══════════════════════════════════════════════════════
    profiles.insert(
        "size-opt".to_string(),
        ProfilePreset {
            lto: Some("thin".to_string()),
            codegen_units: Some(1),
            opt_level: Some("z".to_string()),
            strip: Some(true),
            debug: None,
            panic: Some("abort".to_string()),
            incremental: Some(false),
            rustflags: vec![],
        },
    );

    // ═══════════════════════════════════════════════════════
    // BALANCED — Good tradeoff between compile time and runtime
    // ═══════════════════════════════════════════════════════
    profiles.insert(
        "balanced".to_string(),
        ProfilePreset {
            lto: Some("thin".to_string()),
            codegen_units: Some(16),
            opt_level: Some("2".to_string()),
            strip: Some(true),
            debug: Some("line-tables-only".to_string()),
            panic: None,
            incremental: Some(true),
            rustflags: vec![],
        },
    );

    profiles
}

/// Resolves profile names to actual settings
pub struct ProfileResolver {
    custom_profiles: HashMap<String, ProfilePreset>,
}

impl ProfileResolver {
    pub fn new(custom_profiles: &HashMap<String, ProfilePreset>) -> Self {
        Self {
            custom_profiles: custom_profiles.clone(),
        }
    }

    /// Resolve a profile name to settings
    pub fn resolve(&self, name: &str, release: bool) -> Result<ProfilePreset, String> {
        if let Some(profile) = self.custom_profiles.get(name) {
            return Ok(self.apply_release_override(profile, release));
        }

        let builtins = builtin_profiles();
        if let Some(profile) = builtins.get(name) {
            return Ok(self.apply_release_override(profile, release));
        }

        // Aliases: "release" → "fastest"
        if name == "release" {
            return Ok(builtins.get("fastest").unwrap().clone());
        }

        Err(format!(
            "Unknown profile '{}'. Available: {}",
            name,
            self.list_profiles().join(", ")
        ))
    }

    fn apply_release_override(&self, profile: &ProfilePreset, release: bool) -> ProfilePreset {
        if release {
            let mut p = profile.clone();
            if p.opt_level.is_none() {
                p.opt_level = Some("3".to_string());
            }
            if p.lto.is_none() {
                p.lto = Some("thin".to_string());
            }
            p
        } else {
            profile.clone()
        }
    }

    /// List all available profile names
    pub fn list_profiles(&self) -> Vec<String> {
        let builtins = builtin_profiles();
        let mut names: Vec<String> = builtins.keys().cloned().collect();
        for name in self.custom_profiles.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names.sort();
        names
    }

    /// Get details for a specific profile
    pub fn describe(&self, name: &str) -> Option<String> {
        if let Some(profile) = self.custom_profiles.get(name) {
            return Some(format_profile(name, profile));
        }
        let builtins = builtin_profiles();
        builtins
            .get(name)
            .map(|profile| format_profile(name, profile))
    }

    /// Describe all profiles
    pub fn describe_all(&self) -> Vec<String> {
        let builtins = builtin_profiles();
        let mut all: Vec<(String, ProfilePreset)> = builtins.into_iter().collect();
        for (name, profile) in &self.custom_profiles {
            if !all.iter().any(|(n, _)| n == name) {
                all.push((name.clone(), profile.clone()));
            }
        }

        all.sort_by(|(a, _), (b, _)| a.cmp(b));
        all.iter()
            .map(|(name, profile)| format_profile(name, profile))
            .collect()
    }
}

fn format_profile(name: &str, profile: &ProfilePreset) -> String {
    let lto = profile.lto.as_deref().unwrap_or("default");
    let cgu = profile
        .codegen_units
        .map(|u| u.to_string())
        .unwrap_or_else(|| "default".to_string());
    let opt = profile.opt_level.as_deref().unwrap_or("default");
    let strip = profile
        .strip
        .map(|s| if s { "yes" } else { "no" })
        .unwrap_or("default");
    let panic = profile.panic.as_deref().unwrap_or("default");
    let inc = profile
        .incremental
        .map(|s| if s { "yes" } else { "no" })
        .unwrap_or("default");
    let flags = if profile.rustflags.is_empty() {
        "none".to_string()
    } else {
        profile.rustflags.join(" ")
    };

    let badge = if name == "fastest" { " ⚡DEFAULT" } else { "" };

    format!(
        "  {}{:20}| LTO: {:6}| Codegen: {:4}| Opt: {:4}| Strip: {:5}| Panic: {:7}| Incr: {:5}| Flags: {}",
        name, badge, lto, cgu, opt, strip, panic, inc, flags
    )
}
