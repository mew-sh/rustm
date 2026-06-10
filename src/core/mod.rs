//! Core modules for the rustm build engine

pub mod config;
pub mod engine;
pub mod cache;
pub mod linker;
pub mod linker_algo;
pub mod profile;
pub mod parallel;
pub mod benchmark;
pub mod llvm;
pub mod cranelift;

// Re-exports for convenience
pub use config::RustmConfig;
pub use config::ProfilePreset;
