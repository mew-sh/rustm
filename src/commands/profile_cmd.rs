//! Profile command implementation — includes mold optimization info

use crate::commands::cli;
use crate::core::config::RustmConfig;
use crate::core::profile::ProfileResolver;
use colored::*;

pub fn execute(args: cli::ProfileArgs) -> Result<(), String> {
    let project_dir = RustmConfig::find_project_root().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    let config = RustmConfig::load(&project_dir);
    let resolver = ProfileResolver::new(&config.profiles);

    if let Some(ref profile_name) = args.show {
        match resolver.describe(profile_name) {
            Some(desc) => {
                println!("📋 Profile: {}\n", profile_name.cyan().bold());
                println!("{}", desc);

                // Show mold optimization tips for release profiles
                if profile_name.contains("release") || profile_name.contains("size") {
                    println!("\n⚡ mold optimization recommendations:");
                    println!(
                        "  {} ICF=all — merges identical functions, reduces size 5-15%",
                        "→".green()
                    );
                    println!(
                        "  {} Relaxation — eliminates GOT indirection for faster runtime",
                        "→".green()
                    );
                    println!(
                        "  {} Build-ID=fast — nearly zero cost vs SHA-1",
                        "→".green()
                    );
                    println!(
                        "  {} Compact .dyn — reduces dynamic section size",
                        "→".green()
                    );
                }
            }
            None => {
                return Err(format!("Unknown profile: '{}'", profile_name));
            }
        }
        return Ok(());
    }

    if args.compare.is_some() {
        let descriptions = resolver.describe_all();
        println!("📊 Comparing all profiles\n");
        println!("⚡ mold optimizations per profile:");
        println!("  dev-fast:     ICF=none (fastest compile, no folding overhead)");
        println!("  dev-check:    ICF=none (minimal overhead for type checking)");
        println!("  balanced:     ICF=safe (safe folding, slight overhead)");
        println!("  release-fast: ICF=safe + relaxation (good size/runtime tradeoff)");
        println!("  release-max:  ICF=all + relaxation + strip (maximum optimization)");
        println!("  size-opt:     ICF=all + relaxation + strip (minimum binary)\n");
        for desc in &descriptions {
            println!("{}", desc);
        }
        return Ok(());
    }

    // List all profiles (default action)
    let descriptions = resolver.describe_all();

    println!("⚡ Available Build Profiles\n");
    println!("⚡ mold's optimizations are applied per-profile:");
    println!("  dev profiles:  ICF=none (fastest compile)");
    println!("  release profiles: ICF=safe/all + relaxation (smallest/fastest binary)\n");

    for desc in &descriptions {
        println!("{}", desc);
    }

    println!(
        "\n💡 Use: {} to see details of a specific profile",
        "rustm profile --show <name>".cyan()
    );
    println!(
        "💡 Use: {} to see mold optimization comparison",
        "rustm profile --compare".cyan()
    );

    Ok(())
}
