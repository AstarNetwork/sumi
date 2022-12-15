use itertools::Itertools;
use scale_info::{form::PortableForm, PortableRegistry, TypeDef, TypeDefPrimitive};
use std::collections::HashMap;

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

impl EvmTypeRegistry {
    fn new(registry: &PortableRegistry) -> Self {
        let mut instance = Self {
            mapping: HashMap::new(),
            // registry,
        };

        for ink_type in registry.types() {
            if !instance.mapping.contains_key(&ink_type.id()) {
                if let Some(evm_type) = instance.convert_type(ink_type.ty().type_def(), registry) {
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

    // fn lookup_reference_or_insert(&mut self, id: u32) -> &str {
    //     if let Some(evm_type) = self.lookup(id) {
    //         return &evm_type.reference;
    //     }

    //     let ty = self.registry.resolve(id).expect("should exist");
    //     let new_type = self.convert_type(ty.type_def());

    //     self.insert(id, new_type);

    //     return &self
    //         .lookup(id)
    //         .expect("should be inserted by now")
    //         .reference;
    // }

    fn convert_type(
        &mut self,
        ty: &TypeDef<PortableForm>,
        registry: &PortableRegistry,
    ) -> Option<EvmType> {
        let mut lookup_reference_or_insert = |id| {
            if let Some(ty) = self.lookup(id) {
                Some(ty.reference.clone())
            } else {
                let ty = registry.resolve(id).expect("should exist");
                let new_type = self.convert_type(ty.type_def(), registry)?;
                let reference = new_type.reference.clone();
                self.insert(id, new_type);
                Some(reference)
            }
        };

        Some(match ty {
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
                let is_tuple = composite
                    .fields()
                    .iter()
                    .any(|field| field.name().is_none());

                EvmType {
                    // Tuples are defined in place
                    definition: None,

                    reference: format!(
                        "({})",
                        composite
                            .fields()
                            .iter()
                            .map(|field| {
                                let id = field.ty().id();
                                lookup_reference_or_insert(id).unwrap_or_default()
                            })
                            .join(",")
                    ),
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

        template.add_formatter("path", |value, buffer| {
            let path: String = value
                .as_array()
                .expect("not an array")
                .iter()
                .filter_map(|v| v.as_str())
                .join("::");

            buffer.push_str(&path);
            Ok(())
        });

        let evm_registry = EvmTypeRegistry::new(&project.registry());

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

        let rendered = template.render("module", &project).unwrap();
        println!("{}", rendered);
    }
}
