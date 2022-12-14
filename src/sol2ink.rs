use crate::error::Error;
use convert_case::{Case, Casing};
use ethabi::ParamType;
use hex::ToHex;
use itertools::Itertools;
use serde::Serialize;
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use tinytemplate::{format_unescaped, TinyTemplate};

static MODULE_TEMPLATE: &'static str = include_str!("../templates/ink-module.txt");

#[derive(Serialize)]
struct Input {
    name: String,

    // Type came from metadata
    evm_type: String,

    // Equivalent type to use in ink! code
    rust_type: String,
}

#[derive(Serialize)]
pub struct Function {
    name: String,
    inputs: Vec<Input>,
    output: String,
    selector: String,
    selector_hash: String,
}

#[derive(Serialize)]
struct Variant {
    inputs: Vec<Input>,
    output: String,
    selector: String,
    selector_hash: String,
}

#[derive(Serialize)]
struct OverloadedFunction {
    name: String,
    variants: Vec<Variant>,
}

#[derive(Serialize)]
struct Module {
    #[serde(rename = "module_name")]
    name: String,
    evm_id: String,
    functions: Vec<Function>,
    overloaded_functions: Vec<OverloadedFunction>,
}

fn convert_type(ty: &ParamType) -> String {
    match ty {
        ParamType::Bool => "bool".to_owned(),
        ParamType::Address => "H160".to_owned(),
        ParamType::Array(inner) => format!("Vec<{}>", convert_type(inner)),
        ParamType::FixedArray(inner, size) => format!("[{}; {}]", convert_type(inner), size),
        ParamType::Tuple(inner) => format!("({})", inner.iter().map(convert_type).join(", ")),
        ParamType::FixedBytes(size) => format!("FixedBytes<{}>", size),
        ParamType::Bytes => "Vec<u8>".to_owned(),
        ParamType::String => "String".to_owned(),

        ParamType::Int(size) => match size {
            8 => "i8",
            16 => "i16",
            32 => "i32",
            64 => "i64",
            128 => "i128",

            _ => "I256",
        }
        .to_owned(),

        ParamType::Uint(size) => match size {
            8 => "u8",
            16 => "u16",
            32 => "u32",
            64 => "u64",
            128 => "u128",

            _ => "U256",
        }
        .to_owned(),
    }
}

pub fn render(json: json::JsonValue, module_name: &str, evm_id: &str) -> Result<String, Error> {
    let mut template = TinyTemplate::new();

    template.set_default_formatter(&format_unescaped);
    template.add_template("module", MODULE_TEMPLATE)?;

    template.add_formatter("snake", |value, buffer| match value {
        serde_json::Value::String(s) => {
            buffer.push_str(&s.to_case(Case::Snake));
            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("upper_snake", |value, buffer| match value {
        serde_json::Value::String(s) => {
            buffer.push_str(&s.to_case(Case::UpperSnake));
            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("upper_camel", |value, buffer| match value {
        serde_json::Value::String(s) => {
            buffer.push_str(&s.to_case(Case::UpperCamel));
            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("capitalize", |value, buffer| match value {
        serde_json::Value::String(s) => {
            let (head, tail) = s.split_at(1);

            buffer.push_str(&head.to_uppercase());
            buffer.push_str(tail);

            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    let mut is_overloaded = HashMap::new();
    for (index, function) in json
        .members()
        .enumerate()
        .filter(|(_, item)| item["type"] == "function")
        .filter(|(_, item)| item["stateMutability"] != "view")
        .filter(|(_, item)| {
            item["outputs"]
                .members()
                .all(|output| output["type"] == "bool")
        })
    {
        let function_name = function["name"].as_str().ok_or_else(|| {
            Error::Metadata(format!("'name' for ABI item {index} not exists or is not a string"))
        })?;

        is_overloaded
            .entry(function_name)
            .and_modify(|v| *v = true)
            .or_insert(false);
    }

    let mut overloaded_functions = Vec::<OverloadedFunction>::new();
    let mut functions = Vec::new();

    for (index, function) in json
        .members()
        .enumerate()
        .filter(|(_, item)| item["type"] == "function")
        .filter(|(_, item)| item["stateMutability"] != "view")
        .filter(|(_, item)| {
            item["outputs"]
                .members()
                .all(|output| output["type"] == "bool")
        })
    {
        let function_name = function["name"].as_str().ok_or_else(|| {
            Error::Metadata(format!("'name' for ABI item {index} not exists or is not a string"))
        })?;

        let inputs = function["inputs"]
            .members()
            .enumerate()
            .map(|(index, input)| {
                let name = input["name"].as_str().ok_or_else(|| {
                    Error::Metadata(format!("invalid 'name' input parameter {index} of function {function_name}"))
                })?;

                let raw_type = input["type"].as_str().ok_or_else(|| {
                    Error::Metadata(format!("invalid 'type' in input parameter item {name} ({index}) of function {function_name}"))
                })?;

                let param_type = ethabi::param_type::Reader::read(raw_type)?;
                let converted = convert_type(&param_type);

                Ok(Input {
                    name: name.to_owned(),
                    evm_type: raw_type.to_owned(),
                    rust_type: converted,
                })
            })
            .collect::<Result<Vec<Input>, Error>>()?;

        // let outputs: String = function["outputs"].members().map(|m| format!("{}: {}, ", m["name"], m["type"])).collect();

        let selector = format!(
            "{function_name}({args})",
            args = inputs.iter().map(|input| input.evm_type.as_str()).join(","),
        );

        let mut hasher = Keccak256::new();
        hasher.update(selector.as_bytes());
        let selector_hash: &[u8] = &hasher.finalize();
        let selector_hash: [u8; 4] = selector_hash[0..=3]
            .try_into()
            .expect("Keccac256 hash should contain at least 4 bytes");

        if is_overloaded[function_name] {
            let function = {
                if let Some(function) = overloaded_functions
                    .iter_mut()
                    .find(|f| f.name == function_name)
                {
                    function
                } else {
                    overloaded_functions.push(OverloadedFunction {
                        name: function_name.to_owned(),
                        variants: Vec::new(),
                    });

                    overloaded_functions
                        .last_mut()
                        .expect("we've just pushed an item; cannot fail")
                }
            };

            function.variants.push(Variant {
                inputs,
                output: "bool".to_owned(), // TODO
                selector,
                selector_hash: selector_hash.encode_hex(),
            })
        } else {
            functions.push(Function {
                name: function_name.to_owned(),
                inputs,
                output: "bool".to_owned(), // TODO
                selector,
                selector_hash: selector_hash.encode_hex(),
            });
        }
    }

    let module = Module {
        name: module_name.to_owned(),
        evm_id: evm_id.to_owned(),
        overloaded_functions,
        functions,
    };

    Ok(template.render("module", &module)?)
}
