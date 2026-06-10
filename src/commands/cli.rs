//! CLI definition using clap derive

use clap::{Parser, Subcommand, Args};

#[derive(Parser, Debug)]
#[command(
    name = "rustm",
    version,
    about = "rustm - Blazing-fast Rust/Cargo build compiler & optimizer",
    long_about = "rustm is a smart Cargo wrapper that automatically applies performance\n\
                  optimizations to make your Rust builds significantly faster.\n\n\
                  It auto-detects the best linker (mold/lld), configures sccache,\n\
                  optimizes codegen-units, applies mold linker optimizations,\n\
                  and supports PGO (Profile-Guided Optimization) and BOLT.",
    after_help = "EXAMPLES:\n  \
                  rustm build                    Build with default (dev-fast) profile\n  \
                  rustm build --release          Build with release-fast profile\n  \
                  rustm build --profile release-max   Maximum optimization\n  \
                  rustm pgo generate             Step 1: Build instrumented binary for PGO\n  \
                  rustm pgo merge                Step 3: Merge PGO profile data\n  \
                  rustm pgo use                  Step 4: Rebuild with PGO data\n  \
                  rustm doctor                   Check system for optimization opportunities"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build the project with optimizations
    #[command(visible_alias = "b")]
    Build(BuildArgs),

    /// Build and run the project
    #[command(visible_alias = "r")]
    Run(RunArgs),

    /// Check the project (fast, no binary output)
    #[command(visible_alias = "c")]
    Check(CheckArgs),

    /// Run tests with optimizations
    #[command(visible_alias = "t")]
    Test(TestArgs),

    /// Run clippy with optimizations
    #[command(visible_alias = "l")]
    Clippy(ClippyArgs),

    /// Clean build artifacts and caches
    Clean(CleanArgs),

    /// Manage rustm configuration
    #[command(visible_alias = "cfg")]
    Config(ConfigArgs),

    /// Show build time benchmarks and history
    #[command(visible_alias = "bm")]
    Bench(BenchArgs),

    /// Manage build profiles
    #[command(visible_alias = "pf")]
    Profile(ProfileArgs),

    /// Check system for optimization opportunities
    #[command(visible_alias = "dr")]
    Doctor,

    /// Initialize rustm config in current project
    #[command(visible_alias = "i")]
    Init(InitArgs),

    /// PGO (Profile-Guided Optimization) workflow
    #[command(visible_alias = "p")]
    Pgo(PgoArgs),

    /// LLVM optimization info and diagnostics
    #[command(visible_alias = "ll")]
    Llvm(LlvmArgs),

    /// Apply BOLT post-link optimization
    Bolt(BoltArgs),
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    #[arg(short, long)]
    pub release: bool,

    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    #[arg(long)]
    pub target: Option<String>,

    #[arg(short, long, value_delimiter = ',')]
    pub features: Vec<String>,

    #[arg(long)]
    pub all_features: bool,

    #[arg(long)]
    pub no_default_features: bool,

    #[arg(short, long)]
    pub jobs: Option<u32>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long)]
    pub quiet: bool,

    #[arg(long)]
    pub package: Option<String>,

    #[arg(long)]
    pub workspace: bool,

    /// Enable PGO instrumented build (Step 1)
    #[arg(long)]
    pub pgo_generate: bool,

    /// Enable PGO optimized build (Step 3)
    #[arg(long)]
    pub pgo_use: bool,

    /// PGO profile path
    #[arg(long)]
    pub pgo_path: Option<String>,

    /// Auto-detect and enable CPU features (AVX2, NEON, etc.)
    #[arg(long)]
    pub target_features: bool,

    /// Enable LLVM optimization remarks
    #[arg(long)]
    pub llvm_remarks: bool,

    /// Codegen backend: auto, cranelift, llvm
    /// auto = Cranelift for dev, LLVM for release
    /// cranelift = Fast codegen (2-5x faster), ~80-95% binary performance
    /// llvm = Maximum optimization, slower codegen
    #[arg(long, value_name = "BACKEND")]
    pub codegen_backend: Option<String>,

    #[arg(last = true)]
    pub args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(short, long)]
    pub release: bool,

    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    #[arg(long)]
    pub target: Option<String>,

    #[arg(short, long, value_delimiter = ',')]
    pub features: Vec<String>,

    #[arg(long)]
    pub all_features: bool,

    #[arg(long)]
    pub no_default_features: bool,

    #[arg(short, long)]
    pub jobs: Option<u32>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long)]
    pub quiet: bool,

    #[arg(long)]
    pub package: Option<String>,

    #[arg(last = true)]
    pub args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    #[arg(short, long)]
    pub release: bool,

    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    #[arg(long)]
    pub target: Option<String>,

    #[arg(short, long, value_delimiter = ',')]
    pub features: Vec<String>,

    #[arg(long)]
    pub all_features: bool,

    #[arg(long)]
    pub no_default_features: bool,

    #[arg(short, long)]
    pub jobs: Option<u32>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long)]
    pub quiet: bool,

    #[arg(long)]
    pub package: Option<String>,
}

#[derive(Args, Debug)]
pub struct TestArgs {
    #[arg(short, long)]
    pub release: bool,

    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    #[arg(long)]
    pub target: Option<String>,

    #[arg(short, long, value_delimiter = ',')]
    pub features: Vec<String>,

    #[arg(long)]
    pub all_features: bool,

    #[arg(long)]
    pub no_default_features: bool,

    #[arg(short, long)]
    pub jobs: Option<u32>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long)]
    pub quiet: bool,

    #[arg(last = true)]
    pub args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ClippyArgs {
    #[arg(short, long)]
    pub release: bool,

    #[arg(long, value_name = "PROFILE")]
    pub profile: Option<String>,

    #[arg(short, long, value_delimiter = ',')]
    pub features: Vec<String>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long)]
    pub quiet: bool,
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    #[arg(long)]
    pub cache: bool,

    #[arg(long)]
    pub sccache: bool,

    #[arg(long)]
    pub full: bool,

    #[arg(long)]
    pub package: Option<String>,

    #[arg(long)]
    pub release: bool,
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[arg(short, long)]
    pub show: bool,

    #[arg(short, long)]
    pub generate: bool,

    #[arg(short, long)]
    pub set: Option<String>,

    #[arg(short, long)]
    pub edit: bool,
}

#[derive(Args, Debug)]
pub struct BenchArgs {
    #[arg(short, long, default_value = "10")]
    pub count: usize,

    #[arg(short, long)]
    pub compare: bool,

    #[arg(short, long)]
    pub run: bool,
}

#[derive(Args, Debug)]
pub struct ProfileArgs {
    #[arg(short, long)]
    pub list: bool,

    #[arg(long)]
    pub show: Option<String>,

    #[arg(short, long)]
    pub compare: Option<String>,
}

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(short, long)]
    pub force: bool,

    #[arg(short, long)]
    pub preset: Option<String>,
}

/// PGO workflow commands
#[derive(Args, Debug)]
pub struct PgoArgs {
    /// Step 1: Build instrumented binary (-C profile-generate)
    #[arg(short, long)]
    pub generate: bool,

    /// Step 2: Run workload (just prints instructions)
    #[arg(short, long)]
    pub run: bool,

    /// Step 3: Merge profile data with llvm-profdata
    #[arg(short, long)]
    pub merge: bool,

    /// Step 4: Rebuild with profile data (-C profile-use)
    #[arg(short, long)]
    pub use_profile: bool,

    /// Show PGO workflow instructions
    #[arg(short, long)]
    pub info: bool,

    /// Profile directory (for generate) or .profdata file (for use)
    #[arg(short, long)]
    pub path: Option<String>,

    /// Build profile to use
    #[arg(long)]
    pub profile: Option<String>,
}

/// LLVM optimization info
#[derive(Args, Debug)]
pub struct LlvmArgs {
    /// Show available LLVM optimizations
    #[arg(short, long)]
    pub info: bool,

    /// Detect CPU features
    #[arg(short, long)]
    pub detect: bool,

    /// Show PGO instructions
    #[arg(short, long)]
    pub pgo: bool,

    /// Show BOLT instructions
    #[arg(short, long)]
    pub bolt: bool,
}

/// BOLT post-link optimization
#[derive(Args, Debug)]
pub struct BoltArgs {
    /// Path to the binary to optimize
    #[arg(short, long)]
    pub binary: Option<String>,

    /// Path to BOLT profile data (fdata format)
    #[arg(short, long)]
    pub profile: Option<String>,

    /// Output path for optimized binary
    #[arg(short, long)]
    pub output: Option<String>,
}
