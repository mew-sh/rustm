//! Config command implementation

use crate::commands::cli;
use crate::core::config::RustmConfig;
use colored::*;

pub fn execute(args: cli::ConfigArgs) -> Result<(), String> {
    // If generate, create a default config file
    if args.generate {
        let project_dir = RustmConfig::find_project_root()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
        
        let config_path = project_dir.join("rustm.toml");
        if config_path.exists() && !args.edit {
            println!("{} rustm.toml already exists. Use --edit to overwrite.", "⚠️".to_string());
            return Ok(());
        }

        let content = RustmConfig::generate_default();
        std::fs::write(&config_path, content)
            .map_err(|e| format!("Failed to write rustm.toml: {}", e))?;
        
        println!("{} Generated default config at {}", "✅".to_string(), 
            config_path.display().to_string().cyan());
        return Ok(());
    }

    // If set, update a config value
    if let Some(ref set_value) = args.set {
        let project_dir = RustmConfig::find_project_root()
            .ok_or("Not in a Rust project directory.".to_string())?;
        
        let parts: Vec<&str> = set_value.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err("Invalid format. Use: rustm config --set key=value".to_string());
        }

        let key = parts[0];
        let value = parts[1];
        
        let mut config = RustmConfig::load(&project_dir);
        
        match key {
            "build.default_profile" => config.build.default_profile = value.to_string(),
            "build.lto" => config.build.lto = value.to_string(),
            "build.strip" => config.build.strip = value == "true",
            "build.incremental" => config.build.incremental = value == "true",
            "build.target" => config.build.target = if value.is_empty() { None } else { Some(value.to_string()) },
            "cache.sccache" => config.cache.sccache = value == "true",
            "cache.max_size_gb" => config.cache.max_size_gb = value.parse()
                .map_err(|_| "Invalid number for cache.max_size_gb".to_string())?,
            "linker.preferred" => config.linker.preferred = value.to_string(),
            "parallel.jobs" => config.parallel.jobs = value.parse()
                .map_err(|_| "Invalid number for parallel.jobs".to_string())?,
            _ => return Err(format!("Unknown config key: '{}'", key)),
        }

        config.save(&project_dir)?;
        println!("{} Set {} = {}", "✅".to_string(), key.cyan(), value.green());
        return Ok(());
    }

    // Show current config
    let project_dir = RustmConfig::find_project_root()
        .ok_or("Not in a Rust project directory. Run 'rustm config --generate' to create one.".to_string())?;
    
    let config = RustmConfig::load(&project_dir);
    let content = toml::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    println!("{} Current configuration (rustm.toml):\n", "📋".to_string());
    println!("{}", content);
    Ok(())
}
