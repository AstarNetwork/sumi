use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct Args {
    /// Input filename or stdin if empty
    #[arg(long, short)]
    pub input: Option<PathBuf>,

    /// Output filename or stdout if empty
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Ink module name to generate
    #[arg(long, short)]
    pub module_name: String,

    /// EVM ID to use in module
    #[arg(long, short, default_value = "0x0F")]
    pub evm_id: String,
}
