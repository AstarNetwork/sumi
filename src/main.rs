mod cli;
mod error;
mod template;

use clap::Parser;
use snafu::prelude::*;
use std::{
    fs,
    io::{self, BufRead, BufReader, BufWriter, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    let mut reader: Box<dyn BufRead> = match &args.input {
        Some(filename) => Box::new(BufReader::new(
            fs::File::open(&filename).context(error::ReadInputSnafu { path: filename })?,
        )),
        None => Box::new(BufReader::new(io::stdin())),
    };

    let mut writer: Box<dyn Write> = match &args.output {
        Some(filename) => Box::new(BufWriter::new(
            fs::File::create(&filename).context(error::WriteOutputSnafu { path: filename })?,
        )),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let parsed_json = {
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer)?;

        json::parse(&buffer)?
    };

    let rendered = template::render(parsed_json, &args.module_name, &args.evm_id)?;
    write!(writer, "{}\n", rendered)?;

    Ok(())
}
