use convert_case::{Case, Casing};
use ethabi::ParamType;
use hex::ToHex;
use itertools::Itertools;
use serde::Serialize;
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use tinytemplate::{format_unescaped, TinyTemplate};

static MODULE_TEMPLATE: &'static str = include_str!("../templates/module.txt");

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

pub fn render(
    json: json::JsonValue,
    module_name: &str,
    evm_id: &str,
) -> Result<String, tinytemplate::error::Error> {
    let mut template = TinyTemplate::new();

    template.set_default_formatter(&format_unescaped);
    template.add_template("module", MODULE_TEMPLATE)?;

    template.add_formatter("snake", |value, buf| match value {
        serde_json::Value::String(s) => {
            *buf += &s.to_case(Case::Snake);
            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("upper_snake", |value, buf| match value {
        serde_json::Value::String(s) => {
            *buf += &s.to_case(Case::UpperSnake);
            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("upper_camel", |value, buf| match value {
        serde_json::Value::String(s) => {
            *buf += &s.to_case(Case::UpperCamel);
            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("capitalize", |value, buf| match value {
        serde_json::Value::String(s) => {
            let (head, tail) = s.split_at(1);

            *buf += &head.to_uppercase();
            *buf += tail;

            Ok(())
        }
        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    template.add_formatter("convert_type", |value, buf| match value {
        serde_json::Value::String(raw_type) => {
            let param_type = ethabi::param_type::Reader::read(raw_type).unwrap();
            let converted = convert_type(&param_type);

            buf.push_str(&converted);
            Ok(())
        }

        _ => Err(tinytemplate::error::Error::GenericError {
            msg: "string value expected".to_owned(),
        }),
    });

    let mut is_overloaded = HashMap::new();
    for function in json
        .members()
        .filter(|item| item["type"] == "function")
        .filter(|item| item["stateMutability"] != "view")
        .filter(|item| {
            item["outputs"]
                .members()
                .all(|output| output["type"] == "bool")
        })
    {
        let function_name = function["name"].as_str().unwrap();

        is_overloaded
            .entry(function_name)
            .and_modify(|v| *v = true)
            .or_insert(false);
    }

    let mut overloaded_functions = Vec::<OverloadedFunction>::new();
    let mut functions = Vec::new();

    for function in json
        .members()
        .filter(|item| item["type"] == "function")
        .filter(|item| item["stateMutability"] != "view")
        .filter(|item| {
            item["outputs"]
                .members()
                .all(|output| output["type"] == "bool")
        })
    {
        let function_name = function["name"].to_string();

        let inputs: Vec<_> = function["inputs"]
            .members()
            .map(|m| {
                let raw_type = m["type"].as_str().unwrap();
                let param_type = ethabi::param_type::Reader::read(raw_type).unwrap();
                let converted = convert_type(&param_type);

                Input {
                    name: m["name"].to_string(),
                    evm_type: raw_type.to_string(),
                    rust_type: converted,
                }
            })
            .collect();

        // let outputs: String = function["outputs"].members().map(|m| format!("{}: {}, ", m["name"], m["type"])).collect();

        let selector = format!(
            "{name}({args})",
            name = function_name,
            args = inputs.iter().map(|input| input.evm_type.as_str()).join(","),
        );

        let mut hasher = Keccak256::new();
        hasher.update(selector.as_bytes());
        let selector_hash: &[u8] = &hasher.finalize();
        let selector_hash: [u8; 4] = selector_hash[0..=3].try_into().unwrap();

        if is_overloaded[function_name.as_str()] {
            let function = {
                if let Some(function) = overloaded_functions
                    .iter_mut()
                    .find(|f| f.name == function_name)
                {
                    function
                } else {
                    overloaded_functions.push(OverloadedFunction {
                        name: function_name.clone(),
                        variants: Vec::new(),
                    });

                    overloaded_functions.last_mut().unwrap()
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
                name: function_name,
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
