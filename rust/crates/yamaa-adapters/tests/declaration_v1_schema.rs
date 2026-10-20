#![cfg(feature = "yaml")]
//! The 1.0 schema files in `yaml/v1/`, read with the engine's schema model.
//!
//! The package embeds the closure of each 1.0 file type beside the 0.5 ones,
//! and the engine's bundle rules accept the notation the 1.0 files are written
//! in, `kind` included (`yaml/README.md`). Each file type's closure is admitted
//! with valid defaults, and every converted example of the 1.0 interface RFC
//! passes the document decoder: its header selects its closure, and it is
//! admitted at the version it declares, a specification layer by layer.
mod declaration {
    mod v1_schema {
        use std::collections::BTreeSet;
        use std::path::{Path, PathBuf};
        use yamaa_adapters::document::{self, check_references, Closures, Decoded};
        use yamaa_adapters::shipped_schema;
        use yamaa_adapters::specification_source::{Error, InheritancePort, Source};
        use yamaa_adapters::yaml_decode::decode_yaml;
        use yamaa_core::diagnostic::ConditionCode;
        use yamaa_core::inheritance::{Source as GraphSource, SourceError};
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::schema::{Document, DocumentNode as N, SchemaAliasKind};

        fn repository() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
        }

        fn text<'a>(document: &'a Document, name: &str) -> Option<&'a str> {
            match &document.nodes()[document.field(document.root(), name)?] {
                N::Text(value) => Some(value),
                _ => None,
            }
        }

        /// Files resolved beside their declaring file.
        struct Files;
        impl InheritancePort for Files {
            type Error = std::io::Error;
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<GraphSource, SourceError<Self::Error>> {
                let candidate = Path::new(declaring).parent().unwrap().join(written);
                let identity = candidate
                    .canonicalize()
                    .map_err(|_| SourceError::Unavailable)?;
                Ok(GraphSource {
                    identity: identity.to_str().unwrap().into(),
                    display_path: candidate.to_str().unwrap().into(),
                })
            }
            fn capture(
                &mut self,
                source: &GraphSource,
            ) -> Result<Vec<u8>, SourceError<Self::Error>> {
                std::fs::read(&source.identity).map_err(SourceError::Raised)
            }
            fn rebase(
                &mut self,
                _: &GraphSource,
                _: &GraphSource,
                written: &str,
            ) -> Result<String, Self::Error> {
                Ok(written.into())
            }
        }

        fn examples() -> Vec<PathBuf> {
            let mut paths = Vec::new();
            let mut pending = vec![repository().join("docs/design/interface-1.0/examples")];
            while let Some(directory) = pending.pop() {
                for entry in std::fs::read_dir(&directory).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        pending.push(path);
                    } else if path
                        .extension()
                        .is_some_and(|extension| extension == "yaml")
                    {
                        paths.push(path.canonicalize().unwrap());
                    }
                }
            }
            paths.sort();
            paths
        }

        fn decode(path: &Path) -> Document {
            decode_yaml(&std::fs::read(path).unwrap()).unwrap().document
        }

        #[test]
        fn every_file_type_closure_is_admitted_with_valid_defaults_and_kinds() {
            for file_type in FileType::ALL {
                let schema = shipped_schema::capture_versioned(file_type).unwrap();
                let structure = schema.structure();
                assert_eq!(structure.version(), "1.0", "{file_type:?}");
                assert_eq!(structure.root_class().name, file_type.root_class());
                let defaults = structure.validate_defaults().unwrap();
                assert!(
                    defaults
                        .iter()
                        .all(|default| default.diagnostics.is_empty()),
                    "{file_type:?}: {defaults:?}"
                );
                // A named value type keeps the kind it declares (#3030 D31).
                let (_, identifier) = structure.alias_named("identifier").unwrap();
                let SchemaAliasKind::Descriptor(descriptor) = identifier.kind else {
                    panic!("identifier is a value type");
                };
                assert_eq!(
                    structure.descriptors()[descriptor].descriptor.kind(),
                    Some("identifier")
                );
            }
        }

        #[test]
        fn the_six_converted_examples_pass_the_document_decoder() {
            let closures = Closures::shipped().unwrap();
            let examples = examples();
            let parents: BTreeSet<PathBuf> = examples
                .iter()
                .filter_map(|path| {
                    let parent = text(&decode(path), "parent")?.to_string();
                    Some(path.parent()?.join(parent).canonicalize().unwrap())
                })
                .collect();
            let mut chains = Vec::new();
            let mut directories = BTreeSet::new();
            for path in &examples {
                let directory = path
                    .strip_prefix(
                        repository()
                            .join("docs/design/interface-1.0/examples")
                            .canonicalize()
                            .unwrap(),
                    )
                    .unwrap()
                    .components()
                    .next()
                    .unwrap()
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned();
                directories.insert(directory.clone());
                let identity = path.to_str().unwrap().to_string();
                let declared = decode(path);
                let file_type = FileType::parse(text(&declared, "type").unwrap()).unwrap();
                let source = Source {
                    identity: identity.clone(),
                    bytes: std::fs::read(path).unwrap(),
                };
                let Decoded::Versioned(document) =
                    document::decode(source, file_type, Some(LANGUAGE_VERSION), &closures).unwrap()
                else {
                    panic!("{identity}: every example declares the 1.x header");
                };
                assert_eq!(document.header().version, LANGUAGE_VERSION);
                // Every file an example's referencing fields name declares the
                // type its field reads. No example selects `relative_to: entry`.
                check_references(
                    &document.raw().document,
                    file_type,
                    &identity,
                    &identity,
                    LANGUAGE_VERSION,
                    &mut Files,
                )
                .unwrap_or_else(|error| panic!("{identity}: {error:?}"));
                match file_type {
                    // A parent is a layer, prepared with the entry naming it.
                    FileType::Spec if parents.contains(path) => {}
                    FileType::Spec => {
                        let prepared = document
                            .prepare_specification(identity.clone(), &mut Files)
                            .unwrap_or_else(|error| panic!("{identity}: {error:?}"));
                        assert_eq!(prepared.version(), LANGUAGE_VERSION);
                        chains.push((directory, prepared.layers().len()));
                    }
                    _ => {
                        document
                            .admit()
                            .unwrap_or_else(|error| panic!("{identity}: {error:?}"));
                    }
                }
            }
            assert_eq!(
                chains,
                [
                    ("adam-adsl-bmi".to_string(), 1),
                    ("adam-adtte-pfs".to_string(), 1),
                    ("schema-inheritance".to_string(), 3),
                    ("schema-inheritance-clear".to_string(), 2),
                    ("sdtm-dm-metadata".to_string(), 1),
                    ("sdtm-lb-multiform".to_string(), 1),
                ]
            );
            assert_eq!(directories.len(), 6);
        }

        #[test]
        fn a_0_5_spelling_in_a_1_0_file_fails() {
            let closures = Closures::shipped().unwrap();
            let path =
                repository().join("docs/design/interface-1.0/examples/adam-adsl-bmi/spec.yaml");
            let original = std::fs::read_to_string(&path).unwrap();
            let edit = |old: &str, new: &str| {
                assert!(original.contains(old), "{old}");
                Source {
                    identity: "spec.yaml".into(),
                    bytes: original.replacen(old, new, 1).into_bytes(),
                }
            };
            // `schema_version` beside `yamaa` routes the file to the 0.5 path,
            // which reports `yamaa` as an unknown field.
            let both = edit("type: spec\n", "type: spec\nschema_version: '1.0'\n");
            let decoded =
                document::decode(both, FileType::Spec, Some(LANGUAGE_VERSION), &closures).unwrap();
            let Decoded::Legacy { source, .. } = decoded else {
                panic!("schema_version keeps the 0.5 path: {decoded:?}");
            };
            let error = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(source)
                .unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!("0.5 findings: {error:?}");
            };
            let unknown = captured
                .findings()
                .iter()
                .find(|finding| finding.code == ConditionCode::SchemaUnknownField)
                .unwrap_or_else(|| panic!("an unknown field: {error:?}"));
            assert_eq!(unknown.path, "yamaa");
            // A 0.5 spelling under the 1.x header fails 1.0 admission.
            for (old, new) in [
                ("derivation: SOURCE.STUDYID", "derivation: {literal: X}"),
                (
                    "derivation: SOURCE.USUBJID",
                    "derivation: {source: SOURCE.USUBJID}",
                ),
                ("cm_per_m: 100", "cm_per_m: {date: 2024-01-01}"),
            ] {
                let decoded = document::decode(
                    edit(old, new),
                    FileType::Spec,
                    Some(LANGUAGE_VERSION),
                    &closures,
                )
                .unwrap();
                let Decoded::Versioned(document) = decoded else {
                    panic!("{new}: the 1.x header selects the 1.0 path: {decoded:?}");
                };
                assert!(
                    document
                        .prepare_specification("spec.yaml".into(), &mut Files)
                        .is_err(),
                    "{new} should fail"
                );
            }
        }
    }
}
