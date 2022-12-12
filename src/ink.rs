
#[test]
fn load() {
    use ink_metadata::InkProject;
    use std::{fs::File, io::{BufReader, Read}};

    let mut file = BufReader::new(File::open("samples/ink-erc20.json").unwrap());
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).unwrap();

    let metadata: serde_json::Value = serde_json::from_str(&buffer).unwrap();
    let project: InkProject = serde_json::from_value(metadata["V3"].clone()).unwrap();

    for message in project.spec().messages() {
        println!("{}", message.label());
    }
}
