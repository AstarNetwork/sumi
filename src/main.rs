mod cli;
mod error;
mod ink2sol;
mod sol2ink;

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

    let rendered = match args.mode {
        cli::Mode::EvmToInk => {
            let parsed_json = {
                let mut buffer = String::new();
                reader.read_to_string(&mut buffer)?;

                json::parse(&buffer).map_err(Error::from)?
            };

            sol2ink::render(parsed_json, &args.module_name.unwrap(), &args.evm_id)?
        }

        cli::Mode::InkToEvm => {
            ink2sol::render(&mut reader, &args.module_name)?
        },
    };

    write!(writer, "{}\n", rendered)?;

    Ok(())
}
