use ink_metadata::InkProject;
use itertools::Itertools;
use scale_info::{form::PortableForm, Path, PortableRegistry, Type, TypeDef, TypeDefPrimitive};
use serde::{Serialize, Deserialize};
use std::{collections::HashMap, rc::Rc, io::Read, cell::RefCell};
use tinytemplate::{TinyTemplate, error::Error::GenericError};

use crate::error::Error;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct EvmType {
    /// How the type should be defined in the source. For example,
    /// for structs that would be `struct { ... }`. For primitives
    /// and tuples the definition is absent.
    definition: Option<String>,

    /// How the type should be referred to in the source, for example
    /// in function arguments list. Typically that would be just a
    /// type name, but for tuples that would contain full definition.
    reference: String,

    modifier: Option<String>,

    /// How the type should be encoded to Scale format
    encoder: Option<String>,
}

#[derive(Debug, Default)]
pub struct EvmTypeRegistry {
    mapping: HashMap<u32, EvmType>,
}

struct Context<'template> {
    project: Rc<InkProject>,
    templates: TinyTemplate<'template>,
}

impl<'template> Context<'template> {
    fn new(project: Rc<InkProject>) -> Self {
        let mut templates = TinyTemplate::new();
        templates.set_default_formatter(&tinytemplate::format_unescaped);
        templates
            .add_template("struct", include_str!("../templates/solidity-struct.txt"))
            .unwrap();
        templates
            .add_template("enum", include_str!("../templates/solidity-enum.txt"))
            .unwrap();
        templates
            .add_template("encoder", include_str!("../templates/solidity-encoder.txt"))
            .unwrap();

        templates.add_formatter("path", format_path);

        Context { project, templates }
    }
}

fn format_path(value: &serde_json::Value, buffer: &mut String) -> tinytemplate::error::Result<()> {
    let path: String = value
        .as_array()
        .expect("not an array")
        .iter()
        .filter_map(|v| v.as_str())
        .join("_");

    buffer.push_str(&path);
    Ok(())
}

impl EvmTypeRegistry {
    fn new(registry: &PortableRegistry) -> Self {
        Self::default()
    }

    fn lookup(&self, id: u32) -> Option<&EvmType> {
        self.mapping.get(&id)
    }

    fn lookup_mut(&mut self, id: u32) -> Option<&mut EvmType> {
        self.mapping.get_mut(&id)
    }

    fn insert<'r, 'm>(&mut self, id: u32, ty: EvmType) {
        self.mapping.insert(id, ty);
    }

    fn convert_type(
        &mut self,
        id: u32,
        ty: &Type<PortableForm>,
        context: &Context,
    ) -> Option<EvmType> {
        let mut lookup_reference_or_insert = |id| {
            if let Some(ty) = self.lookup(id) {
                Some(ty.reference.clone())
            } else {
                let ty = context
                    .project
                    .registry()
                    .resolve(id)
                    .expect("should exist");
                let new_type = self.convert_type(id, ty, context)?;
                let reference = new_type.reference.clone();
                self.insert(id, new_type);
                Some(reference)
            }
        };

        #[derive(Serialize)]
        struct Struct {
            path: Path<PortableForm>,
            fields: Vec<Field>,
        }

        #[derive(Serialize)]
        struct Field {
            name: String,
            #[serde(rename = "type")]
            ty: String,
        }

        let mut fields_to_struct =
            |path: Path<PortableForm>,
             fields: Box<dyn Iterator<Item = scale_info::Field<PortableForm>>>| {
                let fields = fields
                    .enumerate()
                    .map(|(index, field)| {
                        let id = field.ty().id();

                        Field {
                            name: field
                                .name()
                                .cloned()
                                .unwrap_or_else(|| format!("f{}", index)),
                            ty: lookup_reference_or_insert(id).unwrap_or_default(),
                        }
                    })
                    .collect_vec();

                Struct { path, fields }
            };

        Some(match ty.type_def() {
            TypeDef::Primitive(primitive) => EvmType {
                reference: match primitive {
                    TypeDefPrimitive::Bool => "bool",
                    TypeDefPrimitive::Char => return None, // TODO
                    TypeDefPrimitive::Str => "string",
                    TypeDefPrimitive::U8 => "uint8",
                    TypeDefPrimitive::U16 => "uint16",
                    TypeDefPrimitive::U32 => "uint32",
                    TypeDefPrimitive::U64 => "uint64",
                    TypeDefPrimitive::U128 => "uint128",
                    TypeDefPrimitive::U256 => "uint256",
                    TypeDefPrimitive::I8 => "int8",
                    TypeDefPrimitive::I16 => "int16",
                    TypeDefPrimitive::I32 => "int32",
                    TypeDefPrimitive::I64 => "int64",
                    TypeDefPrimitive::I128 => "int128",
                    TypeDefPrimitive::I256 => "int256",
                }
                .to_owned(),
                ..EvmType::default()
            },

            TypeDef::Array(array) => {
                let id = array.type_param().id();
                let reference = lookup_reference_or_insert(id)?;
                let size = array.len();

                // Special handling of byte arrays
                if reference == "uint8" && size <= 32 {
                    EvmType {
                        reference: format!("bytes{size}"),
                        ..EvmType::default()
                    }
                } else {
                    EvmType {
                        reference: format!("{reference}[{size}]"),
                        ..EvmType::default()
                    }
                }
            }

            TypeDef::Composite(composite) => {
                let st = fields_to_struct(
                    ty.path().clone(),
                    Box::new(composite.fields().iter().cloned()),
                );

                EvmType {
                    // Tuples are not first class citizens of Solidity.
                    // Hence, we are forced to define them as structs.
                    definition: Some(context.templates.render("struct", &st).unwrap()),

                    reference: ty.path().segments().join("_"),

                    // Structures should be declared using `memory` specifier
                    modifier: Some("memory".to_owned()),

                    encoder: Some(context.templates.render("encoder", &st).unwrap()),

                    ..EvmType::default()
                }
            }

            TypeDef::Tuple(tuple) => {
                let st =
                    fields_to_struct(
                        ty.path().clone(),
                        Box::new(tuple.fields().iter().map(|id| {
                            scale_info::Field::<PortableForm>::new(None, *id, None, vec![])
                        })),
                    );

                EvmType {
                    // Tuples are not first class citizens of Solidity.
                    // Hence, we are forced to define them as structs.
                    definition: Some(context.templates.render("struct", &st).unwrap()),

                    // Structures should be referred using `memory` specifier
                    reference: ty.path().segments().join("_") + " memory",

                    ..EvmType::default()
                }
            }

            TypeDef::Variant(variant) => {
                let default_indices = variant
                    .variants()
                    .iter()
                    .enumerate()
                    .all(|(index, variant)| index == variant.index() as usize);

                // Solidity does not support non-default variant discriminants :(
                if !default_indices {
                    return None; // TODO report error
                }

                // Algebraic enums would require complex discriminant and substructure handling :(
                // Currently we just encode them as C-style POD enums completely omitting fields
                EvmType {
                    definition: Some(context.templates.render("enum", &ty).unwrap()),
                    reference: ty.path().segments().join("_"),
                    ..EvmType::default()
                }
            }

            _ => return None, // todo!(),
        })
    }
}

pub fn render(reader: &mut dyn Read) -> Result<String, Error> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;

    let metadata: serde_json::Value = serde_json::from_str(&buffer)?;
    let project: Rc<InkProject> = Rc::new(serde_json::from_value(metadata["V3"].clone())?);

    static MODULE_TEMPLATE: &'static str = include_str!("../templates/solidity-module.txt");
    let mut template = tinytemplate::TinyTemplate::new();

    template.set_default_formatter(&tinytemplate::format_unescaped);
    template.add_template("module", MODULE_TEMPLATE)?;

    template.add_formatter("debug", |value, buffer| {
        buffer.push_str(&format!("{:?}", value));
        Ok(())
    });

    template.add_formatter("path", format_path);

    let evm_registry = Rc::new(RefCell::new(EvmTypeRegistry::new(&project.registry())));
    let context = Context::new(project.clone());

    let registry = evm_registry.clone();
    template.add_predicate("mapped", move |id| {
        let id = id
            .as_u64()
            .and_then(|id| id.try_into().ok())
            .ok_or_else(|| GenericError {
                msg: format!("invalid id {id:?}"),
            })?;

        Ok(registry.borrow().lookup(id).is_some())
    });

    template.add_formatter_with_args("type", move |value, arg, buffer| {
        if let serde_json::Value::Number(id) = value {
            let id = id
                .as_u64()
                .and_then(|id| id.try_into().ok())
                .ok_or_else(|| GenericError {
                    msg: format!("invalid id {id:?}"),
                })?;

            let write_buffer = |ty: &EvmType, buffer: &mut String| {
                let empty = String::default();
                buffer.push_str(match arg {
                    Some("reference") => ty.reference.as_ref(),
                    Some("definition") => ty.definition.as_ref().unwrap_or(&empty),
                    Some("modifier") => ty.modifier.as_ref().unwrap_or(&empty),
                    Some("encoder") => ty.encoder.as_ref().unwrap_or(&empty),
                    _ => panic!("type formatter must come with an argument"),
                });
            };

            let mut registry = evm_registry.borrow_mut();
            match registry.lookup_mut(id) {
                Some(ty) => write_buffer(ty, buffer),
                None => {
                    let ty =
                        context
                            .project
                            .registry()
                            .resolve(id)
                            .ok_or_else(|| GenericError {
                                msg: format!("invalid id {id:?}"),
                            })?;
                    let mut new_type = registry.convert_type(id, ty, &context).unwrap();
                    write_buffer(&mut new_type, buffer);
                    registry.insert(id, new_type);
                }
            }

            Ok(())
        } else {
            return Err(GenericError {
                msg: format!("invalid type id {:?}", value),
            });
        }
    });

    Ok(template.render("module", &*project)?)
}

#[test]
fn type_conversion() {}

#[cfg(test)]
mod tests {
    use super::*;
    use scale_info::{meta_type, PortableRegistry, Registry};

    #[test]
    fn type_registry() {
        let mut ink_registry = Registry::new();
        let array_type_id = ink_registry.register_type(&meta_type::<[u8; 20]>()).id();

        let ink_registry: PortableRegistry = ink_registry.into();
        let evm_registry = EvmTypeRegistry::new(&ink_registry);

        assert_eq!(
            evm_registry.lookup(array_type_id),
            Some(&EvmType {
                reference: "uint8[20]".to_owned(),
                ..EvmType::default()
            })
        );

        assert_eq!(
            evm_registry.lookup(1),
            Some(&EvmType {
                reference: "uint8".to_owned(),
                ..EvmType::default()
            })
        );
    }

    #[test]
    fn encode() {
        use parity_scale_codec::Encode;
        dbg!([(1u8, 2u8), (3u8, 4u8)].encode().bytes());
        dbg!(vec![1u8, 2, 3, 4, 5].encode().bytes());
    }
}
