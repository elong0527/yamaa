//! The JSON Schema of each 1.0 file type (#3030 section 2.12): the file
//! `tools/packaging/generate_json_schema.py` commits in `yaml/json/`,
//! which the package embeds byte for byte from its copy in the crate's
//! `schema/json/` when it is built, so nothing is read at run time. The
//! committed file is indented for review; the engine gives the same document
//! as canonical JSON text, the encoding of every JSON text it writes: one
//! line, members in key order, numbers as written.

use yamaa_core::language::FileType;

use crate::json_wire;

/// The JSON Schema of a file of `file_type`, as canonical JSON text.
pub fn json_schema(file_type: FileType) -> String {
    let document = json_wire::decode(committed(file_type))
        .expect("the embedded JSON Schema is the JSON its generator wrote");
    json_wire::encode(&document)
}

/// The embedded copy of `yaml/json/<file_type>.schema.json`.
fn committed(file_type: FileType) -> &'static str {
    super::data::V1_JSON_SCHEMAS
        .iter()
        .find(|(name, _)| *name == file_type.name())
        .map(|(_, text)| *text)
        .expect("every file type has a JSON Schema")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read(file_type: FileType) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../yaml/json")
            .join(format!("{}.schema.json", file_type.name()));
        std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    /// The package embeds each committed JSON Schema byte for byte, one per
    /// file type, so `make check-repository`'s comparison of the generated
    /// copy and this test agree on what the engine gives.
    #[test]
    fn each_file_type_embeds_its_committed_json_schema() {
        let embedded = super::super::data::V1_JSON_SCHEMAS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        assert_eq!(embedded, FileType::ALL.map(FileType::name));
        for file_type in FileType::ALL {
            assert_eq!(
                committed(file_type).as_bytes(),
                read(file_type),
                "{}",
                file_type.name()
            );
        }
    }

    /// The canonical text is the committed document: one ASCII line that
    /// decodes to the same value and encodes to itself.
    #[test]
    fn the_json_schema_is_the_committed_document_as_canonical_text() {
        for file_type in FileType::ALL {
            let text = json_schema(file_type);
            let committed = json_wire::decode_slice(&read(file_type)).unwrap();
            assert_eq!(json_wire::decode(&text).unwrap(), committed);
            assert_eq!(json_wire::encode(&committed), text);
            assert!(
                text.is_ascii() && !text.contains('\n'),
                "{}",
                file_type.name()
            );
            assert!(text.starts_with(r#"{"$defs":{"#), "{}", file_type.name());
            let title = format!(r#""title":"yamaa 1.0 {} file"}}"#, file_type.name());
            assert!(text.ends_with(&title), "{}", file_type.name());
        }
    }
}
