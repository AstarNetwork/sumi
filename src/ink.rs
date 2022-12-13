use itertools::Itertools;

#[test]
fn load() {
    use ink_metadata::InkProject;
    use std::{
        fs::File,
        io::{BufReader, Read},
    };

    let mut file = BufReader::new(File::open("samples/ink-erc20.json").unwrap());
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).unwrap();

    let metadata: serde_json::Value = serde_json::from_str(&buffer).unwrap();
    let project: InkProject = serde_json::from_value(metadata["V3"].clone()).unwrap();

    static MODULE_TEMPLATE: &'static str = include_str!("../templates/solidity.txt");
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

    let rendered = template.render("module", &project).unwrap();
    println!("{}", rendered);
}
