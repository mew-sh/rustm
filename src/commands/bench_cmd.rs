//! Benchmark command implementation

use crate::commands::cli;
use crate::core::benchmark::BuildHistory;
use crate::core::config::RustmConfig;
use crate::core::engine::{BuildEngine, BuildRequest, BuildType};

pub fn execute(args: cli::BenchArgs) -> Result<(), String> {
    let history = BuildHistory::new();

    if args.run {
        let project_dir = RustmConfig::find_project_root()
            .ok_or("Not in a Rust project directory.".to_string())?;

        let config = RustmConfig::load(&project_dir);
        let engine = BuildEngine::new(config);

        println!("Running benchmark build...");

        let request = BuildRequest {
            build_type: BuildType::Build,
            profile_name: None,
            release: false,
            target: None,
            features: vec![],
            all_features: false,
            no_default_features: false,
            jobs: None,
            verbose: false,
            quiet: true,
            args: vec![],
            project_dir,
            codegen_backend: None,
            pgo_generate: false,
            pgo_use: false,
            pgo_path: None,
            target_features: false,
            llvm_remarks: false,
        };

        let result = engine.build(&request)?;
        println!("  Build time: {}", format_duration(result.duration_ms));
    }

    if args.compare {
        let project_dir = RustmConfig::find_project_root()
            .ok_or("Not in a Rust project directory.".to_string())?;

        let profiles = vec![
            ("dev-fast", false),
            ("dev-check", false),
            ("dev-cranelift", false),
            ("balanced", false),
            ("release-fast", true),
            ("release-max", true),
            ("fastest", true),
        ];

        println!("\nProfile Build Time Comparison\n");
        println!(
            "{:<15} {:<15} {}",
            "Profile", "Duration", "Speedup vs dev-fast"
        );
        println!("{}", "─".repeat(55));

        let mut baseline: Option<u128> = None;

        for (profile_name, release) in &profiles {
            let config = RustmConfig::load(&project_dir);
            let engine = BuildEngine::new(config);
            let request = BuildRequest {
                build_type: BuildType::Build,
                profile_name: Some(profile_name.to_string()),
                release: *release,
                target: None,
                features: vec![],
                all_features: false,
                no_default_features: false,
                jobs: None,
                verbose: false,
                quiet: true,
                args: vec![],
                project_dir: project_dir.clone(),
                codegen_backend: None,
                pgo_generate: false,
                pgo_use: false,
                pgo_path: None,
                target_features: false,
                llvm_remarks: false,
            };

            match engine.build(&request) {
                Ok(result) => {
                    let duration = result.duration_ms;
                    if baseline.is_none() && *profile_name == "dev-fast" {
                        baseline = Some(duration);
                    }
                    let speedup = baseline
                        .map(|b| format!("{:.1}x", b as f64 / duration as f64))
                        .unwrap_or_else(|| "—".to_string());

                    println!(
                        "{:<15} {:<15} {}",
                        profile_name,
                        format_duration(duration),
                        speedup
                    );
                }
                Err(_) => {
                    println!("{:<15} {:<15} {}", profile_name, "failed", "—");
                }
            }
        }
    }

    // Show history
    let stats = history.show_stats(args.count)?;
    println!("{}", stats);

    Ok(())
}

fn format_duration(ms: u128) -> String {
    if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1000 {
        format!("{:.2}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}
