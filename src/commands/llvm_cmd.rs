//! LLVM optimization info command

use crate::commands::cli;
use crate::core::llvm;

pub fn execute(args: cli::LlvmArgs) -> Result<(), String> {
    if args.info || args.detect {
        println!("{}", llvm::print_llvm_info());
    }

    if args.pgo {
        let optimizer = llvm::LlvmOptimizer::new(&Default::default());
        println!("{}", optimizer.pgo_instructions());
    }

    if args.bolt {
        let optimizer = llvm::LlvmOptimizer::new(&Default::default());
        println!("{}", optimizer.bolt_instructions());
    }

    // Default: show everything
    if !args.info && !args.detect && !args.pgo && !args.bolt {
        println!("{}", llvm::print_llvm_info());
    }

    Ok(())
}
