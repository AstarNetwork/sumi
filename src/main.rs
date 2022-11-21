use std::{io::{BufReader, self, BufRead, BufWriter, Write}, fs};

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    /// Input filename or stdin if empty
    #[arg(long, short)]
    input: Option<String>,

    /// Output filename or stdout if empty
    #[arg(long, short)]
    output: Option<String>,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let mut reader: Box<dyn BufRead> = match args.input {
        Some(filename) => Box::new(BufReader::new(fs::File::open(filename).map_err(|e| e.to_string())?)),
        None => Box::new(BufReader::new(io::stdin())),
    };

    let mut _writer: Box<dyn Write> = match args.output {
        Some(filename) => Box::new(BufWriter::new(fs::File::create(filename).map_err(|e| e.to_string())?)),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let mut buf = String::new();
    reader.read_to_string(&mut buf).map_err(|e| e.to_string())?;

    let parsed = json::parse(&buf).map_err(|e| e.to_string())?;

    // println!("{:#?}", parsed);

    for function in parsed
        .members()
        .filter(|item| item["type"] == "function" && item["stateMutability"] != "view" ) 
    {
        let inputs: String = function["inputs"].members().map(|m| format!("{}: {}, ", m["name"], m["type"])).collect();
        let outputs: String = function["outputs"].members().map(|m| format!("{}: {}, ", m["name"], m["type"])).collect();

        println!("{}({}) -> {}", function["name"], inputs, outputs);
    }

    Ok(())

    // while let Ok(len) = reader.read_line(&mut buf) {
    //     if len == 0 { break; }

    //     writer.write_all(buf.as_bytes()).unwrap();
    //     writer.flush().unwrap();
    // }
}

