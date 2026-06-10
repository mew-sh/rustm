//! Cache management for rustm
//!
//! Handles sccache integration, local artifact caching,
//! and cache statistics tracking.

use crate::core::config::CacheConfig;
use std::fs;
use std::path::PathBuf;

/// Cache status information
#[derive(Debug)]
pub struct CacheStatus {
    pub sccache_active: bool,
    #[allow(dead_code)]
    pub local_cache_active: bool,
    #[allow(dead_code)]
    pub cache_dir: PathBuf,
    pub hits: u64,
    pub misses: u64,
}

/// Manages build caches
pub struct CacheManager {
    config: CacheConfig,
    sccache_path: Option<PathBuf>,
    cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new(config: &CacheConfig) -> Self {
        let sccache_path = which::which("sccache").ok();
        let cache_dir = config.dir.as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".rustm").join("cache"))
                    .unwrap_or_else(|| PathBuf::from(".rustm-cache"))
            });

        Self {
            config: config.clone(),
            sccache_path,
            cache_dir,
        }
    }

    pub fn is_sccache_active(&self) -> bool {
        self.config.sccache && self.sccache_path.is_some()
    }

    /// Set up the cache environment
    pub fn setup(&self) -> Result<CacheStatus, String> {
        if self.config.local_cache {
            fs::create_dir_all(&self.cache_dir)
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;
        }

        let sccache_active = self.is_sccache_active();
        if sccache_active {
            if let Some(ref path) = self.sccache_path {
                let _ = std::process::Command::new(path)
                    .arg("--start-server")
                    .output();
            }
        }

        let (hits, misses) = self.get_sccache_stats();

        Ok(CacheStatus {
            sccache_active,
            local_cache_active: self.config.local_cache,
            cache_dir: self.cache_dir.clone(),
            hits,
            misses,
        })
    }

    /// Get sccache statistics
    fn get_sccache_stats(&self) -> (u64, u64) {
        if let Some(ref path) = self.sccache_path {
            if let Ok(output) = std::process::Command::new(path)
                .arg("--show-stats")
                .arg("--stats-format=json")
                .output()
            {
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                    let hits = json.get("cache_hits")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let misses = json.get("cache_misses")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    return (hits, misses);
                }
            }
        }
        (0, 0)
    }

    /// Clear all caches
    pub fn clear(&self) -> Result<(), String> {
        if let Some(ref path) = self.sccache_path {
            let _ = std::process::Command::new(path)
                .arg("--zero-stats")
                .output();
        }

        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)
                .map_err(|e| format!("Failed to clear cache: {}", e))?;
        }

        Ok(())
    }
}
