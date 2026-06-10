//! Build timing and benchmarking
//!
//! Tracks build times and provides comparisons between builds.

use chrono::Local;
use std::fs;
use std::path::PathBuf;

/// Build timing record
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BuildRecord {
    pub timestamp: String,
    pub duration_ms: u128,
    pub profile: String,
    pub linker: String,
    pub success: bool,
}

/// Build timer
pub struct BuildTimer {
    start: std::time::Instant,
}

impl BuildTimer {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }
}

impl Default for BuildTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Build history manager
pub struct BuildHistory {
    history_dir: PathBuf,
}

impl BuildHistory {
    pub fn new() -> Self {
        let history_dir = dirs::home_dir()
            .map(|h| h.join(".rustm").join("history"))
            .unwrap_or_else(|| PathBuf::from(".rustm-history"));

        Self { history_dir }
    }

    /// Record a build result
    pub fn record(&self, record: BuildRecord) -> Result<(), String> {
        fs::create_dir_all(&self.history_dir)
            .map_err(|e| format!("Failed to create history dir: {}", e))?;

        let date = Local::now().format("%Y-%m-%d").to_string();
        let file = self.history_dir.join(format!("builds-{}.jsonl", date));

        let mut line = serde_json::to_string(&record)
            .map_err(|e| format!("Failed to serialize record: {}", e))?;
        line.push('\n');

        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|e| format!("Failed to open history file: {}", e))?;
        
        f.write_all(line.as_bytes())
            .map_err(|e| format!("Failed to write history: {}", e))
    }

    /// Get recent build records
    pub fn recent_builds(&self, count: usize) -> Result<Vec<BuildRecord>, String> {
        if !self.history_dir.exists() {
            return Ok(vec![]);
        }

        let mut files: Vec<_> = fs::read_dir(&self.history_dir)
            .map_err(|e| format!("Failed to read history dir: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "jsonl").unwrap_or(false))
            .collect();
        
        files.sort_by_key(|e| e.file_name());
        files.reverse();

        let mut records = Vec::new();
        for file in files {
            if let Ok(content) = fs::read_to_string(file.path()) {
                for line in content.lines().rev() {
                    if let Ok(record) = serde_json::from_str::<BuildRecord>(line) {
                        records.push(record);
                        if records.len() >= count {
                            return Ok(records);
                        }
                    }
                }
            }
        }

        Ok(records)
    }

    /// Show build statistics
    pub fn show_stats(&self, count: usize) -> Result<String, String> {
        let records = self.recent_builds(count)?;
        if records.is_empty() {
            return Ok("No build history found. Run a build first!".to_string());
        }

        let mut output = String::new();
        output.push_str(&format!("Last {} builds:\n\n", records.len().min(count)));
        output.push_str(&format!("{:<5} {:<12} {:<12} {:<8} {:<10} {}\n",
            "#", "Time", "Profile", "Linker", "Duration", "Status"));
        output.push_str(&"─".repeat(70));
        output.push('\n');

        for (i, record) in records.iter().enumerate().take(count) {
            let status = if record.success { "OK" } else { "FAIL" };
            let duration = if record.duration_ms >= 1000 {
                format!("{:.1}s", record.duration_ms as f64 / 1000.0)
            } else {
                format!("{}ms", record.duration_ms)
            };
            let time_str = &record.timestamp[11..16.min(record.timestamp.len())];
            output.push_str(&format!("{:<5} {:<12} {:<12} {:<8} {:<10} {}\n",
                i + 1,
                time_str,
                record.profile,
                record.linker,
                duration,
                status));
        }

        // Calculate averages
        let successful: Vec<_> = records.iter().filter(|r| r.success).collect();
        if !successful.is_empty() {
            let avg_ms: u128 = successful.iter().map(|r| r.duration_ms).sum::<u128>()
                / successful.len() as u128;
            let avg = if avg_ms >= 1000 {
                format!("{:.1}s", avg_ms as f64 / 1000.0)
            } else {
                format!("{}ms", avg_ms)
            };
            output.push_str(&format!("\nAverage build time: {} ({} successful builds)\n",
                avg, successful.len()));
        }

        Ok(output)
    }
}
