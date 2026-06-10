//! Core modules for the rustm build engine

pub mod benchmark;
pub mod cache;
pub mod config;
pub mod cranelift;
pub mod engine;
pub mod linker;
pub mod linker_algo;
pub mod llvm;
pub mod parallel;
pub mod profile;

// Re-exports for convenience
pub use config::ProfilePreset;
pub use config::RustmConfig;
