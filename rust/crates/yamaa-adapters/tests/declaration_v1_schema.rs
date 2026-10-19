#![cfg(feature = "yaml")]
//! The 1.0 schema files in `yaml/v1/`, read with the engine's schema model.
//!
//! Nothing in the engine reads the 1.0 files yet. Its bundle rules accept the
//! notation the 1.0 files are written in except one keyword, the `kind` a
//! named type declares (`yaml/README.md`), so each named type's `kind` is
//! removed before admission and nothing else changes. Each file type's bundle
//! is then admitted with valid defaults, and every converted example of the
//! 1.0 interface RFC validates against its file type's root class.
mod declaration {
    mod v1_schema {
        use std::collections::BTreeSet;
        use std::path::{Path, PathBuf};
        use yamaa_adapters::yaml_decode::decode_yaml;
        use yamaa_core::schema::{
            Document, DocumentNode as N, SchemaDiagnostic, SchemaModule, SchemaStructure,
        };

        /// Each 1.0 file type, its entry module and its root class.
        const FILE_TYPES: [(&str, &str, &str); 4] = [
            ("spec", "schema_spec.yaml", "root_class"),
            ("project", "schema_project.yaml", "project_class"),
            ("codelists", "schema_codelists.yaml", "codelist_file_class"),
            (
                "function",
                "schema_function.yaml",
                "function_definition_class",
            ),
        ];
        /// Root collections whose members defer requiredness in a layer (LAYER-4).
        const COLLECTIONS: [&str; 5] = ["input", "windows", "intermediates", "columns", "rows"];

        fn repository() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
        }

        fn decode_text(label: &str, bytes: &[u8]) -> Document {
            decode_yaml(bytes)
                .unwrap_or_else(|error| panic!("{label}: {error:?}"))
                .document
        }

        fn decode(path: &Path) -> Document {
            let bytes =
                std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            decode_text(&path.display().to_string(), &bytes)
        }

        fn text<'a>(document: &'a Document, name: &str) -> Option<&'a str> {
            match &document.nodes()[document.field(document.root(), name)?] {
                N::Text(value) => Some(value),
                _ => None,
            }
        }

        /// Copy the tree under `id`, children first, leaving out dropped entries.
        fn copy(
            document: &Document,
            id: usize,
            dropped: &BTreeSet<(usize, usize)>,
            nodes: &mut Vec<N>,
        ) -> usize {
            let node = match &document.nodes()[id] {
                N::Sequence(items) => N::Sequence(
                    items
                        .iter()
                        .map(|&item| copy(document, item, dropped, nodes))
                        .collect(),
                ),
                N::Mapping(entries) => N::Mapping(
                    entries
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| !dropped.contains(&(id, *index)))
                        .map(|(_, &(key, value))| {
                            (
                                copy(document, key, dropped, nodes),
                                copy(document, value, dropped, nodes),
                            )
                        })
                        .collect(),
                ),
                scalar => scalar.clone(),
            };
            nodes.push(node);
            nodes.len() - 1
        }

        /// A schema module without the `kind` of its named value types.
        fn without_kind(document: &Document) -> Document {
            let nodes = document.nodes();
            let N::Mapping(declarations) = &nodes[document.root()] else {
                panic!("a schema module is a mapping");
            };
            let mut dropped = BTreeSet::new();
            for &(_, value) in declarations {
                let N::Mapping(entries) = &nodes[value] else {
                    continue;
                };
                let named_type = ["type", "registry"]
                    .iter()
                    .any(|keyword| document.field(value, keyword).is_some());
                for (index, &(key, _)) in entries.iter().enumerate() {
                    if named_type && matches!(&nodes[key], N::Text(name) if name == "kind") {
                        dropped.insert((value, index));
                    }
                }
            }
            let mut copied = Vec::with_capacity(nodes.len());
            let root = copy(document, document.root(), &dropped, &mut copied);
            Document::new(copied, root).expect("a copied module stays a valid document")
        }

        /// One file type's bundle: its entry module first, then what it includes.
        fn closure(entry: &str) -> Vec<SchemaModule> {
            let directory = repository().join("yaml/v1");
            let mut modules: Vec<SchemaModule> = Vec::new();
            let mut pending = vec![entry.to_string()];
            while let Some(name) = pending.pop() {
                if modules.iter().any(|module| module.name == name) {
                    continue;
                }
                let document = without_kind(&decode(&directory.join(&name)));
                if let Some(includes) = document.field(document.root(), "includes") {
                    let N::Sequence(items) = &document.nodes()[includes] else {
                        panic!("{name}: includes is a list");
                    };
                    for &item in items.iter().rev() {
                        let N::Text(included) = &document.nodes()[item] else {
                            panic!("{name}: an include is a file name");
                        };
                        pending.push(included.clone());
                    }
                }
                modules.push(SchemaModule { name, document });
            }
            modules
        }

        fn structure(entry: &str, root: &str) -> SchemaStructure {
            SchemaStructure::admit(closure(entry), 0, root)
                .unwrap_or_else(|error| panic!("{entry}: {error:?}"))
        }

        fn root_class(file_type: &str) -> (SchemaStructure, &'static str) {
            let (_, entry, root) = FILE_TYPES
                .into_iter()
                .find(|(name, _, _)| *name == file_type)
                .unwrap_or_else(|| panic!("no file type {file_type}"));
            (structure(entry, root), root)
        }

        /// A complete document of the root class, requiredness included.
        fn complete(
            structure: &SchemaStructure,
            root: &str,
            document: &Document,
        ) -> Vec<SchemaDiagnostic> {
            let types = structure.parse_query_types(&[root.to_string()]).unwrap();
            structure
                .validate_types(&types, document, document.root(), "$", false)
                .unwrap()
        }

        /// One specification layer: the header is written, an unknown root
        /// field fails, a keyed collection defers requiredness at every depth,
        /// and every other root field is complete (LAYER-4, LAYER-7).
        fn layer(
            structure: &SchemaStructure,
            root: &str,
            document: &Document,
        ) -> Vec<SchemaDiagnostic> {
            let class = structure.class_named(root).unwrap();
            let N::Mapping(entries) = &document.nodes()[document.root()] else {
                panic!("a layer is a mapping");
            };
            for header in ["yamaa", "type"] {
                assert!(
                    text(document, header).is_some(),
                    "a layer declares {header}"
                );
            }
            let mut findings = Vec::new();
            for &(key, value) in entries {
                let N::Text(name) = &document.nodes()[key] else {
                    panic!("a root field name is text");
                };
                let field = class
                    .fields
                    .iter()
                    .find(|field| &field.name == name)
                    .unwrap_or_else(|| panic!("unknown root field {name}"));
                let fragment = COLLECTIONS.contains(&name.as_str());
                findings.extend(
                    structure
                        .validate_descriptor(field.descriptor, document, value, name, fragment)
                        .unwrap(),
                );
            }
            findings
        }

        fn examples() -> Vec<(PathBuf, Document)> {
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
                        paths.push(path);
                    }
                }
            }
            paths.sort();
            paths
                .into_iter()
                .map(|path| {
                    let document = decode(&path);
                    (path, document)
                })
                .collect()
        }

        #[test]
        fn every_file_type_bundle_is_admitted_with_valid_defaults() {
            for (file_type, entry, root) in FILE_TYPES {
                let structure = structure(entry, root);
                assert_eq!(structure.version(), "1.0", "{file_type}");
                let defaults = structure.validate_defaults().unwrap();
                assert!(
                    defaults
                        .iter()
                        .all(|default| default.diagnostics.is_empty()),
                    "{file_type}: {defaults:?}"
                );
            }
        }

        #[test]
        fn the_six_converted_examples_validate() {
            let examples = examples();
            let parents: BTreeSet<PathBuf> = examples
                .iter()
                .filter_map(|(path, document)| {
                    let parent = text(document, "parent")?;
                    Some(path.parent()?.join(parent).canonicalize().unwrap())
                })
                .collect();
            let mut directories = BTreeSet::new();
            let mut layers = 0;
            for (path, document) in &examples {
                let directory = path
                    .parent()
                    .unwrap()
                    .strip_prefix(repository().join("docs/design/interface-1.0/examples"))
                    .unwrap();
                directories.insert(
                    directory
                        .components()
                        .next()
                        .unwrap()
                        .as_os_str()
                        .to_string_lossy()
                        .into_owned(),
                );
                let file_type = text(document, "type").expect("every example declares type");
                let (structure, root) = root_class(file_type);
                let is_layer = text(document, "parent").is_some()
                    || parents.contains(&path.canonicalize().unwrap());
                let findings = if is_layer {
                    layers += 1;
                    // A layer is no complete specification on its own.
                    assert!(!complete(&structure, root, document).is_empty());
                    layer(&structure, root, document)
                } else {
                    complete(&structure, root, document)
                };
                assert!(findings.is_empty(), "{}: {findings:?}", path.display());
            }
            assert_eq!(layers, 5);
            assert_eq!(
                directories.into_iter().collect::<Vec<_>>(),
                [
                    "adam-adsl-bmi",
                    "adam-adtte-pfs",
                    "schema-inheritance",
                    "schema-inheritance-clear",
                    "sdtm-dm-metadata",
                    "sdtm-lb-multiform",
                ]
            );
        }

        #[test]
        fn a_0_5_spelling_in_a_1_0_file_fails() {
            let (structure, root) = root_class("spec");
            let path =
                repository().join("docs/design/interface-1.0/examples/adam-adsl-bmi/spec.yaml");
            let original = std::fs::read_to_string(&path).unwrap();
            for (old, new) in [
                ("type: spec\n", "type: spec\nschema_version: '1.0'\n"),
                ("derivation: SOURCE.STUDYID", "derivation: {literal: X}"),
                (
                    "derivation: SOURCE.USUBJID",
                    "derivation: {source: SOURCE.USUBJID}",
                ),
                ("cm_per_m: 100", "cm_per_m: {date: 2024-01-01}"),
            ] {
                assert!(original.contains(old), "{old}");
                let edited = original.replacen(old, new, 1);
                let document = decode_text("edited example", edited.as_bytes());
                assert!(
                    !complete(&structure, root, &document).is_empty(),
                    "{new} should fail"
                );
            }
        }
    }
}
