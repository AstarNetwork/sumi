use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Mode {
    EvmToInk,
    InkToEvm,
}

#[derive(Parser, Debug)]
pub struct Args {
    /// Input filename or stdin if empty
    #[arg(long, short)]
    pub input: Option<PathBuf>,

    /// Output filename or stdout if empty
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Ink module name to generate
    #[arg(long)]
    pub module_name: Option<String>,

    /// EVM ID to use in module
    #[arg(long, short, default_value = "0x0F")]
    pub evm_id: String,

    #[arg(long, short, default_value = "evm-to-ink")]
    pub mode: Mode,
}
