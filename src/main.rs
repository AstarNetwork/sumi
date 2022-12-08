mod cli;
mod error;
mod template;

use clap::Parser;
use error::Error;
use std::{
    fs,
    io::{self, BufRead, BufReader, BufWriter, Write},
};

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    let mut reader: Box<dyn BufRead> = match args.input {
        Some(filename) => Box::new(BufReader::new(fs::File::open(&filename).map_err(|e| {
            Error::ReadInput {
                path: filename,
                inner: e,
            }
        })?)),
        None => Box::new(BufReader::new(io::stdin())),
    };

    let mut writer: Box<dyn Write> = match args.output {
        Some(filename) => Box::new(BufWriter::new(fs::File::create(&filename).map_err(
            |e| Error::WriteOutput {
                path: filename,
                inner: e,
            },
        )?)),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let parsed_json = {
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer)?;

        json::parse(&buffer).map_err(Error::from)?
    };

    let rendered = template::render(parsed_json, &args.module_name, &args.evm_id)?;
    write!(writer, "{}\n", rendered)?;

    Ok(())
}
