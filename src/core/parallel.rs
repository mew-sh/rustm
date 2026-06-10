//! Parallel build optimization
//!
//! Determines optimal parallelism based on CPU cores,
//! available memory, and build characteristics.

use crate::core::config::ParallelConfig;
use std::num::NonZeroU32;

/// Optimizes parallel build settings
pub struct ParallelOptimizer {
    _config: ParallelConfig,
}

impl ParallelOptimizer {
    pub fn new(config: &ParallelConfig) -> Self {
        Self {
            _config: config.clone(),
        }
    }

    /// Calculate optimal number of parallel jobs
    pub fn optimal_jobs(&self) -> u32 {
        let cpu_count = self.cpu_count();

        // Use more jobs than CPUs for I/O-bound compilation
        // Rust compilation is both CPU and I/O intensive
        let io_multiplier = 1.5;
        let memory_factor = self.memory_factor();

        let optimal = ((cpu_count as f64 * io_multiplier * memory_factor) as u32)
            .max(1)
            .min(cpu_count * 2); // Don't go above 2x CPU count

        optimal
    }

    /// Get CPU core count
    fn cpu_count(&self) -> u32 {
        std::thread::available_parallelism()
            .ok()
            .and_then(|n| NonZeroU32::new(n.get() as u32))
            .map(|n| n.get())
            .unwrap_or(4)
    }

    /// Memory availability factor (reduce parallelism if low memory)
    fn memory_factor(&self) -> f64 {
        #[cfg(target_os = "linux")]
        {
            let cpu_count = self.cpu_count() as u64;
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemAvailable:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                let gb = kb / 1_048_576; // Convert KB to GB
                                let max_by_memory = gb / 2;
                                return (max_by_memory as f64 / cpu_count as f64).min(1.0);
                            }
                        }
                    }
                }
            }
        }

        1.0
    }

    /// Get system info for display
    pub fn system_info(&self) -> String {
        let cpus = self.cpu_count();
        let jobs = self.optimal_jobs();
        format!("CPUs: {} | Optimal jobs: {}", cpus, jobs)
    }
}
