//! Build command implementation — includes LLVM/PGO + Cranelift integration

use crate::commands::cli;
use crate::core::config::RustmConfig;
use crate::core::engine::{BuildEngine, BuildRequest, BuildType, print_build_summary};
use crate::core::benchmark::{BuildRecord, BuildHistory};
use crate::core::llvm::detect_cpu_features;

pub fn execute(args: cli::BuildArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory. No Cargo.toml found.")?;
    
    let mut config = RustmConfig::load(&project_dir);

    // Handle PGO flags
    if args.pgo_generate {
        let pgo_path = args.pgo_path.clone()
            .unwrap_or_else(|| "./target/pgo-profiles".to_string());
        config.env.insert("RUSTM_PGO_STAGE".to_string(), "generate".to_string());
        config.env.insert("RUSTM_PGO_PATH".to_string(), pgo_path);
    } else if args.pgo_use {
        let pgo_path = args.pgo_path.clone()
            .unwrap_or_else(|| "./target/pgo-profiles/merged.profdata".to_string());
        config.env.insert("RUSTM_PGO_STAGE".to_string(), "use".to_string());
        config.env.insert("RUSTM_PGO_PATH".to_string(), pgo_path);
    }

    // Auto-detect CPU features if requested
    if args.target_features {
        let features = detect_cpu_features();
        if !features.is_empty() {
            let feature_str = features.join(",");
            config.build.rustflags.push(format!("-C target-feature={}", feature_str));
        }
    }

    let engine = BuildEngine::new(config);

    let request = BuildRequest {
        build_type: BuildType::Build,
        profile_name: args.profile,
        release: args.release,
        target: args.target,
        features: args.features,
        all_features: args.all_features,
        no_default_features: args.no_default_features,
        jobs: args.jobs,
        verbose: args.verbose,
        quiet: args.quiet,
        args: args.args,
        project_dir,
        codegen_backend: args.codegen_backend,
        pgo_generate: args.pgo_generate,
        pgo_use: args.pgo_use,
        pgo_path: args.pgo_path,
        target_features: args.target_features,
        llvm_remarks: args.llvm_remarks,
    };

    let result = engine.build(&request)?;
    print_build_summary(&result);

    // Record to history
    let history = BuildHistory::new();
    let record = BuildRecord {
        timestamp: chrono::Local::now().to_rfc3339(),
        duration_ms: result.duration_ms,
        profile: result.profile_used.clone(),
        linker: result.linker_used.clone(),
        success: result.success,
    };
    let _ = history.record(record);

    if !result.success {
        return Err("Build failed".to_string());
    }

    Ok(())
}

pub fn execute_run(args: cli::RunArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory.")?;
    
    let config = RustmConfig::load(&project_dir);
    let engine = BuildEngine::new(config);

    let request = BuildRequest {
        build_type: BuildType::Run,
        profile_name: args.profile,
        release: args.release,
        target: args.target,
        features: args.features,
        all_features: args.all_features,
        no_default_features: args.no_default_features,
        jobs: args.jobs,
        verbose: args.verbose,
        quiet: args.quiet,
        args: args.args,
        project_dir,
        codegen_backend: None,
        pgo_generate: false,
        pgo_use: false,
        pgo_path: None,
        target_features: false,
        llvm_remarks: false,
    };

    let result = engine.build(&request)?;
    print_build_summary(&result);

    if !result.success {
        return Err("Run failed".to_string());
    }

    Ok(())
}

pub fn execute_check(args: cli::CheckArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory.")?;
    
    let config = RustmConfig::load(&project_dir);
    let engine = BuildEngine::new(config);

    let request = BuildRequest {
        build_type: BuildType::Check,
        profile_name: args.profile.or_else(|| Some("dev-check".to_string())),
        release: args.release,
        target: args.target,
        features: args.features,
        all_features: args.all_features,
        no_default_features: args.no_default_features,
        jobs: args.jobs,
        verbose: args.verbose,
        quiet: args.quiet,
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
    print_build_summary(&result);

    if !result.success {
        return Err("Check failed".to_string());
    }

    Ok(())
}

pub fn execute_test(args: cli::TestArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory.")?;
    
    let config = RustmConfig::load(&project_dir);
    let engine = BuildEngine::new(config);

    let request = BuildRequest {
        build_type: BuildType::Test,
        profile_name: args.profile,
        release: args.release,
        target: args.target,
        features: args.features,
        all_features: args.all_features,
        no_default_features: args.no_default_features,
        jobs: args.jobs,
        verbose: args.verbose,
        quiet: args.quiet,
        args: args.args,
        project_dir,
        codegen_backend: None,
        pgo_generate: false,
        pgo_use: false,
        pgo_path: None,
        target_features: false,
        llvm_remarks: false,
    };

    let result = engine.build(&request)?;
    print_build_summary(&result);

    if !result.success {
        return Err("Tests failed".to_string());
    }

    Ok(())
}

pub fn execute_clippy(args: cli::ClippyArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory.")?;
    
    let config = RustmConfig::load(&project_dir);
    let engine = BuildEngine::new(config);

    let request = BuildRequest {
        build_type: BuildType::Clippy,
        profile_name: args.profile,
        release: args.release,
        target: None,
        features: args.features,
        all_features: false,
        no_default_features: false,
        jobs: None,
        verbose: args.verbose,
        quiet: args.quiet,
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
    print_build_summary(&result);

    if !result.success {
        return Err("Clippy failed".to_string());
    }

    Ok(())
}
