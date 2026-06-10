//! PGO command implementation

use crate::commands::cli;
use crate::core::config::RustmConfig;
use crate::core::engine::{BuildEngine, BuildRequest, BuildType, print_build_summary};
use crate::core::benchmark::{BuildRecord, BuildHistory};
use crate::core::llvm::LlvmOptimizer;

pub fn execute(args: cli::PgoArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory. No Cargo.toml found.".to_string())?;

    if args.info {
        let optimizer = LlvmOptimizer::new(&Default::default());
        println!("{}", optimizer.pgo_instructions());
        return Ok(());
    }

    if args.generate {
        // Step 1: Build instrumented binary
        println!("\n{} PGO Step 1: Building instrumented binary...", "🎯".to_string());
        println!("{} This binary will collect profile data during execution.\n", "→".to_string());

        let mut config = RustmConfig::load(&project_dir);
        let pgo_path = args.path.clone()
            .unwrap_or_else(|| "./target/pgo-profiles".to_string());

        // Inject PGO generate flags via config
        config.env.insert("RUSTM_PGO_STAGE".to_string(), "generate".to_string());
        config.env.insert("RUSTM_PGO_PATH".to_string(), pgo_path.clone());
        config.build.rustflags.push("-C profile-generate=".to_string() + &pgo_path);

        let engine = BuildEngine::new(config);

        let request = BuildRequest {
            build_type: BuildType::Build,
            profile_name: args.profile.clone().or_else(|| Some("release-fast".to_string())),
            release: true,
            target: None,
            features: vec![],
            all_features: false,
            no_default_features: false,
            jobs: None,
            verbose: true,
            quiet: false,
            args: vec![],
            project_dir: project_dir.clone(),
            codegen_backend: None,
            pgo_generate: false,
            pgo_use: false,
            pgo_path: None,
            target_features: false,
            llvm_remarks: false,
        };

        let result = engine.build(&request)?;

        if result.success {
            println!("\n{} Instrumented binary built successfully!", "✅".to_string());
            println!("\n{} Next steps:", "→".to_string());
            println!("  1. Run your binary with representative workload:");
            println!("     ./target/release/your-binary <args>");
            println!("     Profile data will be written to: {}", pgo_path);
            println!("\n  2. Merge profile data:");
            println!("     rustm pgo merge");
            println!("\n  3. Rebuild with profile data:");
            println!("     rustm pgo use --profile release-max");
        } else {
            return Err("Instrumented build failed".to_string());
        }

        // Record to history
        let history = BuildHistory::new();
        let record = BuildRecord {
            timestamp: chrono::Local::now().to_rfc3339(),
            duration_ms: result.duration_ms,
            profile: format!("pgo-generate-{}", result.profile_used),
            linker: result.linker_used,
            success: result.success,
        };
        let _ = history.record(record);

        return Ok(());
    }

    if args.merge {
        // Step 3: Merge profile data
        println!("\n{} PGO Step 3: Merging profile data...", "🎯".to_string());

        let optimizer = LlvmOptimizer::new(&Default::default());
        let profile_dir = std::path::PathBuf::from(
            args.path.clone().unwrap_or_else(|| "./target/pgo-profiles".to_string())
        );
        let output_path = std::path::PathBuf::from("./target/pgo-profiles/merged.profdata");

        match optimizer.merge_pgo_profiles(&profile_dir, &output_path) {
            Ok(()) => {
                println!("{} Profile data merged successfully!", "✅".to_string());
                println!("  Output: {}", output_path.display());
                println!("\n{} Next: Rebuild with profile data:", "→".to_string());
                println!("  rustm pgo use --profile release-max");
            }
            Err(e) => {
                return Err(format!("Failed to merge profiles: {}", e));
            }
        }

        return Ok(());
    }

    if args.use_profile {
        // Step 4: Rebuild with profile data
        println!("\n{} PGO Step 4: Rebuilding with profile data...", "🎯".to_string());
        println!("{} Using PGO data to optimize hot code paths.\n", "→".to_string());

        let mut config = RustmConfig::load(&project_dir);
        let pgo_path = args.path.clone()
            .unwrap_or_else(|| "./target/pgo-profiles/merged.profdata".to_string());

        // Inject PGO use flags
        config.env.insert("RUSTM_PGO_STAGE".to_string(), "use".to_string());
        config.env.insert("RUSTM_PGO_PATH".to_string(), pgo_path.clone());
        config.build.rustflags.push("-C profile-use=".to_string() + &pgo_path);

        let engine = BuildEngine::new(config);

        let request = BuildRequest {
            build_type: BuildType::Build,
            profile_name: args.profile.clone().or_else(|| Some("release-max".to_string())),
            release: true,
            target: None,
            features: vec![],
            all_features: false,
            no_default_features: false,
            jobs: None,
            verbose: true,
            quiet: false,
            args: vec![],
            project_dir: project_dir.clone(),
            codegen_backend: None,
            pgo_generate: false,
            pgo_use: false,
            pgo_path: None,
            target_features: false,
            llvm_remarks: false,
        };

        let result = engine.build(&request)?;
        print_build_summary(&result);

        // Record to history
        let history = BuildHistory::new();
        let record = BuildRecord {
            timestamp: chrono::Local::now().to_rfc3339(),
            duration_ms: result.duration_ms,
            profile: format!("pgo-use-{}", result.profile_used),
            linker: result.linker_used,
            success: result.success,
        };
        let _ = history.record(record);

        if !result.success {
            return Err("PGO-optimized build failed".to_string());
        }

        return Ok(());
    }

    if args.run {
        println!("\n{} PGO Step 2: Run your binary with representative workload", "🎯".to_string());
        println!("\n{} Run:", "→".to_string());
        println!("  ./target/release/your-binary <your-benchmark-args>");
        println!("\n{} Profile data will be written to: ./target/pgo-profiles/", "→".to_string());
        println!("{} After running, merge profiles:", "→".to_string());
        println!("  rustm pgo merge");

        return Ok(());
    }

    // Default: show PGO instructions
    let optimizer = LlvmOptimizer::new(&Default::default());
    println!("{}", optimizer.pgo_instructions());

    Ok(())
}
