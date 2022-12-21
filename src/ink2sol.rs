use itertools::Itertools;
use scale_info::{form::PortableForm, Path, PortableRegistry, Type, TypeDef, TypeDefPrimitive};
use serde::Serialize;
use std::collections::HashMap;
use tinytemplate::TinyTemplate;

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
}

#[derive(Debug)]
pub struct EvmTypeRegistry {
    mapping: HashMap<u32, EvmType>,
    // registry: &'a PortableRegistry,
}

struct Context<'a, 'template> {
    registry: &'a PortableRegistry,
    templates: TinyTemplate<'template>,
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
        let mut instance = Self {
            mapping: HashMap::new(),
        };

        let mut templates = TinyTemplate::new();
        templates.set_default_formatter(&tinytemplate::format_unescaped);
        templates
            .add_template("struct", include_str!("../templates/solidity-struct.txt"))
            .unwrap();
        templates
            .add_template("enum", include_str!("../templates/solidity-enum.txt"))
            .unwrap();

        templates.add_formatter("path", format_path);

        let context = Context {
            registry,
            templates,
        };

        for ink_type in registry.types() {
            if !instance.mapping.contains_key(&ink_type.id()) {
                if let Some(evm_type) =
                    instance.convert_type(ink_type.id(), ink_type.ty(), &context)
                {
                    instance.insert(ink_type.id(), evm_type);
                }
            }
        }

        instance
    }

    fn lookup(&self, id: u32) -> Option<&EvmType> {
        self.mapping.get(&id)
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
                let ty = context.registry.resolve(id).expect("should exist");
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
                // Primivite types are trivial and do not need definition
                definition: None,

                reference: match primitive {
                    TypeDefPrimitive::Bool => "bool",
                    TypeDefPrimitive::Char => return None, // todo!(), // ?
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
            },

            TypeDef::Array(array) => {
                let id = array.type_param().id();
                let reference = lookup_reference_or_insert(id)?;

                EvmType {
                    // Arrays are defined in place
                    definition: None,

                    reference: format!("{ty}[{size}]", size = array.len(), ty = reference),
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

                    // Structures should be referred using `memory` specifier
                    reference: ty.path().segments().join("_") + " memory",
                }
            }

            TypeDef::Tuple(tuple) => {
                let st = fields_to_struct(
                    ty.path().clone(),
                    Box::new(tuple.fields().iter().map(|id| {
                        scale_info::Field::<PortableForm>::new(
                            None,
                            *id,
                            None,
                            vec![],
                        )
                    })),
                );

                EvmType {
                    // Tuples are not first class citizens of Solidity.
                    // Hence, we are forced to define them as structs.
                    definition: Some(context.templates.render("struct", &st).unwrap()),

                    // Structures should be referred using `memory` specifier
                    reference: ty.path().segments().join("_") + " memory",
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
                }
            }

            _ => return None, // todo!(),
        })
    }
}

#[test]
fn type_conversion() {}

#[cfg(test)]
mod tests {
    use super::*;
    use scale_info::{meta_type, PortableRegistry, Registry};
    use tinytemplate::error::Error::GenericError;

    #[test]
    fn type_registry() {
        let mut ink_registry = Registry::new();
        let array_type_id = ink_registry.register_type(&meta_type::<[u8; 20]>()).id();

        let ink_registry: PortableRegistry = ink_registry.into();
        let evm_registry = EvmTypeRegistry::new(&ink_registry);

        assert_eq!(
            evm_registry.lookup(array_type_id),
            Some(&EvmType {
                definition: None,
                reference: "uint8[20]".to_owned(),
            })
        );

        assert_eq!(
            evm_registry.lookup(1),
            Some(&EvmType {
                definition: None,
                reference: "uint8".to_owned(),
            })
        );
    }

    #[test]
    fn load() {
        use ink_metadata::InkProject;
        use itertools::Itertools;
        use std::{
            fs::File,
            io::{BufReader, Read},
        };

        let mut file = BufReader::new(File::open("samples/ink-erc20.json").unwrap());
        let mut buffer = String::new();
        file.read_to_string(&mut buffer).unwrap();

        let metadata: serde_json::Value = serde_json::from_str(&buffer).unwrap();
        let project: InkProject = serde_json::from_value(metadata["V3"].clone()).unwrap();

        static MODULE_TEMPLATE: &'static str = include_str!("../templates/solidity-module.txt");
        let mut template = tinytemplate::TinyTemplate::new();

        template.set_default_formatter(&tinytemplate::format_unescaped);
        template.add_template("module", MODULE_TEMPLATE).unwrap();

        template.add_formatter("debug", |value, buffer| {
            buffer.push_str(&format!("{:?}", value));
            Ok(())
        });

        template.add_formatter("path", format_path);

        let registry = std::rc::Rc::new(EvmTypeRegistry::new(&project.registry()));

        let evm_registry = registry.clone();
        template.add_formatter("reference", move |value, buffer| {
            if let serde_json::Value::Number(id) = value {
                let id = id
                    .as_u64()
                    .and_then(|id| id.try_into().ok())
                    .expect("id should be valid");

                let reference = &evm_registry
                    .lookup(id)
                    .ok_or_else(|| GenericError {
                        msg: format!("unknown or unsupported type id {}", id),
                    })?
                    .reference;

                buffer.push_str(&reference);
                Ok(())
            } else {
                return Err(GenericError {
                    msg: format!("invalid type id {:?}", value),
                });
            }
        });

        let evm_registry = registry.clone();
        template.add_formatter("definition", move |value, buffer| {
            if let serde_json::Value::Number(id) = value {
                let id = id
                    .as_u64()
                    .and_then(|id| id.try_into().ok())
                    .expect("id should be valid");

                if let Some(definition) = &evm_registry
                    .lookup(id)
                    .and_then(|ty| ty.definition.as_ref())
                // .ok_or_else(|| GenericError {
                //     msg: format!("unknown or unsupported type id {}", id),
                // })?
                // .definition
                {
                    buffer.push_str(&definition);
                }
                Ok(())
            } else {
                return Err(GenericError {
                    msg: format!("invalid type id {:?}", value),
                });
            }
        });

        let rendered = template.render("module", &project).unwrap();
        println!("{}", rendered);
    }
}
