//! BOLT command implementation

use crate::commands::cli;
use crate::core::llvm::LlvmOptimizer;
use colored::Colorize;
use std::path::PathBuf;

pub fn execute(args: cli::BoltArgs) -> Result<(), String> {
    let optimizer = LlvmOptimizer::new(&Default::default());

    // Check if BOLT is available
    if optimizer.detect_bolt().is_none() {
        println!("{} BOLT is not installed.", "⚠️".to_string());
        println!("  Install from: https://github.com/llvm/llvm-project/tree/main/bolt");
        println!();
        println!("{}", optimizer.bolt_instructions());
        return Ok(());
    }

    match (args.binary, args.profile, args.output) {
        (Some(binary), Some(profile), Some(output)) => {
            let binary_path = PathBuf::from(&binary);
            let profile_path = PathBuf::from(&profile);
            let output_path = PathBuf::from(&output);

            if !binary_path.exists() {
                return Err(format!("Binary not found: {}", binary));
            }
            if !profile_path.exists() {
                return Err(format!("Profile not found: {}", profile));
            }

            println!("{} Applying BOLT optimization...", "🔨".to_string());
            println!("  Binary: {}", binary);
            println!("  Profile: {}", profile);
            println!("  Output: {}", output);
            println!();

            optimizer.run_bolt(&binary_path, Some(&profile_path), &output_path)?;

            println!("{} BOLT optimization complete!", "✅".to_string());
            println!("  Optimized binary: {}", output.green());
            println!("  Expected improvement: 5-15% on top of PGO");
        }
        (Some(binary), None, Some(output)) => {
            let binary_path = PathBuf::from(&binary);
            let output_path = PathBuf::from(&output);

            if !binary_path.exists() {
                return Err(format!("Binary not found: {}", binary));
            }

            println!("{} Applying BOLT optimization (no profile)...", "🔨".to_string());
            println!("  Binary: {}", binary);
            println!("  Output: {}", output);
            println!();

            optimizer.run_bolt(&binary_path, None, &output_path)?;

            println!("{} BOLT optimization complete!", "✅".to_string());
            println!("  Optimized binary: {}", output.green());
            println!("  For better results, provide profile data with --profile");
        }
        _ => {
            println!("{}", optimizer.bolt_instructions());
        }
    }

    Ok(())
}
