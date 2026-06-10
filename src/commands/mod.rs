//! CLI commands module

pub mod bench_cmd;
pub mod bolt_cmd;
pub mod build_cmd;
pub mod clean_cmd;
pub mod cli;
pub mod config_cmd;
pub mod doctor_cmd;
pub mod init_cmd;
pub mod llvm_cmd;
pub mod pgo_cmd;
pub mod profile_cmd;

use cli::Cli;

/// Execute the CLI command
pub fn execute(cli: Cli) -> Result<(), String> {
    match cli.command {
        cli::Commands::Build(args) => build_cmd::execute(args),
        cli::Commands::Run(args) => build_cmd::execute_run(args),
        cli::Commands::Check(args) => build_cmd::execute_check(args),
        cli::Commands::Test(args) => build_cmd::execute_test(args),
        cli::Commands::Clippy(args) => build_cmd::execute_clippy(args),
        cli::Commands::Clean(args) => clean_cmd::execute(args),
        cli::Commands::Config(args) => config_cmd::execute(args),
        cli::Commands::Bench(args) => bench_cmd::execute(args),
        cli::Commands::Profile(args) => profile_cmd::execute(args),
        cli::Commands::Doctor => doctor_cmd::execute(),
        cli::Commands::Init(args) => init_cmd::execute(args),
        cli::Commands::Pgo(args) => pgo_cmd::execute(args),
        cli::Commands::Llvm(args) => llvm_cmd::execute(args),
        cli::Commands::Bolt(args) => bolt_cmd::execute(args),
    }
}
