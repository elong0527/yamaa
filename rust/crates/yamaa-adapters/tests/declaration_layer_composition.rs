#![cfg(feature = "yaml")]
//! Composition of 1.0 specification layers (LAYER-1 to LAYER-10, P-CLR-1,
//! P-CLR-2, P-NULL-1), which only tests reach until the engine reads 1.0
//! files (#3030 D76).
//!
//! A layer's `clear` removes values it inherits before its own fields
//! compose, YAML null is the missing value and never removes anything, a
//! finding names row templates and intermediates by `id`, and the resolved
//! specification keeps the provenance of every value. The converted
//! `schema-inheritance-clear` and `negative-property-clear` resolve as their
//! 0.5 sources do.
mod declaration {
    mod layer_composition {
        use std::fs;
        use std::path::{Component, Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use yamaa_adapters::document::{
            self, Closures, Decoded, PreparedSpecification, SpecificationError,
        };
        use yamaa_adapters::resolved_artifact::{self, model::specification_value};
        use yamaa_adapters::specification_report::Identity;
        use yamaa_adapters::specification_source::{Error, InheritancePort, Source};
        use yamaa_core::diagnostic::ConditionCode;
        use yamaa_core::domain_entry::Request;
        use yamaa_core::inheritance::{self as graph, Source as GraphSource, SourceError};
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::project_function::Language;
        use yamaa_core::schema::{NormalizationError, SchemaContext as C, SchemaDiagnostic};

        static NEXT: AtomicUsize = AtomicUsize::new(0);

        fn repository() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
        }

        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-layer-composition-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&path).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn write(&self, name: &str, text: &str) {
                let path = self.0.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            fn path(&self, name: &str) -> String {
                identity(&self.0.join(name))
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn identity(path: &Path) -> String {
            path.to_str().unwrap().replace('\\', "/")
        }

        /// A path without `.` and `..` steps, without reading the filesystem.
        fn lexical(path: &Path) -> PathBuf {
            let mut result = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        result.pop();
                    }
                    other => result.push(other),
                }
            }
            result
        }

        /// Files resolved beside their declaring file. A relative path is
        /// respelled from the entry's directory, so a resolved specification
        /// shows the base each path took (LAYER-10).
        struct Files;
        impl InheritancePort for Files {
            type Error = std::io::Error;
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<GraphSource, SourceError<Self::Error>> {
                let candidate = Path::new(declaring).parent().unwrap().join(written);
                let canonical = yamaa_adapters::canonical_path::canonicalize(&candidate)
                    .map_err(|_| SourceError::Unavailable)?;
                let identity = identity(&canonical);
                Ok(GraphSource {
                    display_path: identity.clone(),
                    identity,
                })
            }
            fn capture(
                &mut self,
                source: &GraphSource,
            ) -> Result<Vec<u8>, SourceError<Self::Error>> {
                fs::read(&source.identity).map_err(SourceError::Raised)
            }
            fn rebase(
                &mut self,
                layer: &GraphSource,
                entry: &GraphSource,
                written: &str,
            ) -> Result<String, Self::Error> {
                let target = lexical(&Path::new(&layer.identity).parent().unwrap().join(written));
                let base = Path::new(&entry.identity).parent().unwrap();
                let target = target.components().collect::<Vec<_>>();
                let base = base.components().collect::<Vec<_>>();
                let shared = target
                    .iter()
                    .zip(&base)
                    .take_while(|(left, right)| left == right)
                    .count();
                let mut steps = vec!["..".to_string(); base.len() - shared];
                steps.extend(
                    target[shared..]
                        .iter()
                        .map(|step| step.as_os_str().to_string_lossy().into_owned()),
                );
                Ok(steps.join("/"))
            }
        }

        /// The 1.0 specification at `identity`, prepared by an engine that
        /// reads 1.0.
        fn prepare(
            identity: &str,
        ) -> Result<PreparedSpecification, SpecificationError<std::io::Error>> {
            let source = Source {
                identity: identity.into(),
                bytes: fs::read(identity).unwrap(),
            };
            let closures = Closures::shipped().unwrap();
            match document::decode(source, FileType::Spec, Some(LANGUAGE_VERSION), &closures) {
                Ok(Decoded::Versioned(document)) => {
                    document.prepare_specification(identity.into(), &mut Files)
                }
                other => panic!("{identity}: {other:?}"),
            }
        }

        /// The resolved specification as JSON.
        fn resolved(prepared: &PreparedSpecification) -> Value {
            specification_value(prepared.resolved()).unwrap().0
        }

        fn at<'a>(value: &'a Value, pointer: &str) -> &'a Value {
            pointer
                .split('/')
                .skip(1)
                .fold(value, |value, step| match value {
                    Value::Array(items) => &items[step.parse::<usize>().unwrap()],
                    Value::Object(fields) => fields
                        .get(step)
                        .unwrap_or_else(|| panic!("{pointer}: no {step}")),
                    _ => panic!("{pointer}: {step} under a scalar"),
                })
        }

        fn json(text: &str) -> Value {
            yamaa_adapters::json_wire::decode(text).unwrap()
        }

        /// One finding as a test compares it: its cause, path and context.
        type Finding = (ConditionCode, String, Vec<(&'static str, C)>);

        fn finding(finding: &SchemaDiagnostic) -> Finding {
            (finding.code, finding.path.clone(), finding.context.clone())
        }

        fn invalid_clear(index: usize, field: &str) -> Finding {
            (
                ConditionCode::SchemaLayerInvalidClear,
                format!("clear[{index}]"),
                vec![("field", C::Text(field.into()))],
            )
        }

        /// The findings in the layer a refused chain names, and that layer.
        fn layer_findings(error: SpecificationError<std::io::Error>) -> (String, Vec<Finding>) {
            match error {
                SpecificationError::Clear {
                    source, findings, ..
                } => (source.identity, findings.iter().map(finding).collect()),
                SpecificationError::Traversal(graph::Error::Layer {
                    source,
                    error: NormalizationError::Invalid(findings),
                    ..
                }) => (source, findings.iter().map(finding).collect()),
                other => panic!("findings in a layer: {other:?}"),
            }
        }

        /// The findings in a resolved specification that is not valid.
        fn resolved_findings(error: SpecificationError<std::io::Error>) -> Vec<Finding> {
            let SpecificationError::Entry(Error::Findings(captured)) = error else {
                panic!("findings in the resolved specification: {error:?}");
            };
            captured.findings().iter().map(finding).collect()
        }

        const BASE: &str = r#"yamaa: "1.0"
type: spec
input:
  DS: {path: ds.csv, types: {CODE: str}}
columns:
  - name: CODE
    type: str
    label: Code
    derivation: DS.CODE
    metadata: {origin: collected, owner: base}
  - name: REASON
    type: str
    label: Reason
    derivation:
      mapping:
        source: DS.CODE
        dict: {AE: ADVERSE EVENT, PD: PROGRESSIVE DISEASE, 'NOT DONE': NOT DONE}
        unmapped: OTHER
metadata: {scope: organization, owner: base}
"#;

        const ENTRY: &str = r#"yamaa: "1.0"
type: spec
parent: base.yaml
domain: TEST
keys: [CODE]
output: {path: out.csv, columns: [CODE, REASON]}
"#;

        /// The entry with `clear` and further fields after its header.
        fn entry(clear: &[&str], rest: &str) -> String {
            let mut text = ENTRY.to_string();
            if !clear.is_empty() {
                text = text.replace(
                    "parent: base.yaml\n",
                    &format!(
                        "parent: base.yaml\nclear:\n{}",
                        clear
                            .iter()
                            .map(|path| format!("  - \"{path}\"\n"))
                            .collect::<String>()
                    ),
                );
            }
            text + rest
        }

        #[test]
        fn a_clear_removes_what_the_layer_inherits_before_its_own_fields_compose() {
            let study = Study::new();
            study.write("base.yaml", BASE);
            let written = "columns:\n  - name: CODE\n    metadata: {owner: study}\n";
            // Without a clear, a member's mapping merges key by key (LAYER-6).
            study.write("spec.yaml", &entry(&[], written));
            let merged = resolved(&prepare(&study.path("spec.yaml")).unwrap());
            assert_eq!(
                at(&merged, "/columns/0/metadata"),
                &json(r#"{"origin":"collected","owner":"study"}"#)
            );
            study.write(
                "spec.yaml",
                &entry(
                    &[
                        "columns.REASON.derivation.mapping.dict.PD",
                        "columns.REASON.derivation.mapping.dict['NOT DONE']",
                        "columns.CODE.label",
                        "columns.CODE.metadata",
                        "metadata.owner",
                    ],
                    written,
                ),
            );
            let prepared = prepare(&study.path("spec.yaml")).unwrap();
            let spec = resolved(&prepared);
            // An entry of a mapping derivation written bare is named through
            // its shorthand; the other entry and the answer for a code
            // without an entry stay.
            let mapping = at(&spec, "/columns/1/derivation/value/mapping");
            assert_eq!(at(mapping, "/dict"), &json(r#"{"AE":"ADVERSE EVENT"}"#));
            assert_eq!(at(mapping, "/unmapped"), &json(r#""OTHER""#));
            // A cleared optional field is gone, and a mapping cleared and
            // written in one layer is replaced instead of merged.
            let code = at(&spec, "/columns/0").as_object().unwrap();
            assert!(!code.contains_key("label"));
            assert_eq!(code["metadata"], json(r#"{"owner":"study"}"#));
            // A key of a root mapping no later layer writes is cleared, and
            // the rest of the mapping is inherited.
            assert_eq!(at(&spec, "/metadata"), &json(r#"{"scope":"organization"}"#));
            // Composition consumes `parent` and `clear`.
            let root = spec.as_object().unwrap();
            assert!(!root.contains_key("parent") && !root.contains_key("clear"));
            assert_eq!(root["yamaa"], json(r#""1.0""#));
            // Each value keeps the layer that wrote it.
            let base = study.path("base.yaml");
            let entry = study.path("spec.yaml");
            for (path, layer) in [
                ("columns.REASON.derivation.mapping.dict.AE", &base),
                ("columns.REASON.derivation.mapping.unmapped", &base),
                ("columns.CODE.metadata.owner", &entry),
                ("columns.CODE.type", &base),
                ("metadata.scope", &base),
                ("domain", &entry),
            ] {
                assert_eq!(
                    prepared.declaring_source(path),
                    Some(layer.as_str()),
                    "{path}"
                );
            }
        }

        #[test]
        fn a_cleared_field_takes_its_default_again_and_its_path_its_base() {
            let study = Study::new();
            study.write(
                "shared/base.yaml",
                &BASE.replace(
                    "DS: {path: ds.csv,",
                    "DS: {path: ds.csv, relative_to: entry,",
                ),
            );
            let entry = |clear: &[&str]| {
                entry(clear, "").replace("parent: base.yaml", "parent: ../shared/base.yaml")
            };
            // The inherited `relative_to: entry` reads the study's own file.
            study.write("study/spec.yaml", &entry(&[]));
            let spec = resolved(&prepare(&study.path("study/spec.yaml")).unwrap());
            assert_eq!(at(&spec, "/input/DS/relative_to"), &json(r#""entry""#));
            assert_eq!(at(&spec, "/input/DS/path"), &json(r#""ds.csv""#));
            // Cleared, the field takes its default, and the path its
            // declaring layer's directory (LAYER-7, LAYER-10).
            study.write("study/spec.yaml", &entry(&["input.DS.relative_to"]));
            let spec = resolved(&prepare(&study.path("study/spec.yaml")).unwrap());
            assert_eq!(at(&spec, "/input/DS/relative_to"), &json(r#""declaring""#));
            assert_eq!(at(&spec, "/input/DS/path"), &json(r#""../shared/ds.csv""#));
        }

        #[test]
        fn yaml_null_is_the_missing_value_and_never_removes_anything() {
            let study = Study::new();
            study.write("base.yaml", BASE);
            study.write(
                "spec.yaml",
                &entry(
                    &[],
                    "columns:\n  - name: REASON\n    derivation: {mapping: {unmapped: null}}\n  - name: CODE\n    derivation: null\n",
                ),
            );
            let spec = resolved(&prepare(&study.path("spec.yaml")).unwrap());
            // `unmapped: null` answers missing; the inherited dictionary and
            // source stay (COLUMN-10).
            let mapping = at(&spec, "/columns/1/derivation/value/mapping");
            assert_eq!(at(mapping, "/unmapped"), &Value::Null);
            assert_eq!(at(mapping, "/source/variable"), &json(r#""DS.CODE""#));
            assert_eq!(at(mapping, "/dict").as_object().unwrap().len(), 3);
            // `derivation: null` derives missing instead of removing the
            // inherited derivation.
            assert_eq!(at(&spec, "/columns/0/derivation/value"), &Value::Null);
        }

        #[test]
        fn yaml_null_is_refused_outside_value_and_expression_fields() {
            let study = Study::new();
            const ALONE: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n";
            for (old, new, path) in [
                ("label: Identifier", "label: null", "columns.ID.label"),
                ("type: int", "type: null", "columns.ID.type"),
                ("name: ID,", "name: null,", "columns[0].name"),
                ("keys: [ID]", "keys: null", "keys"),
                ("keys: [ID]", "keys: [null]", "keys[0]"),
                ("domain: TEST", "domain: null", "domain"),
                ("domain: TEST", "domain: TEST\nparent: null", "parent"),
                ("domain: TEST", "domain: TEST\nmetadata: null", "metadata"),
                (
                    "domain: TEST",
                    "domain: TEST\nmetadata: {scope: null}",
                    "metadata.scope",
                ),
                ("input: {SRC: input.csv}", "input: {SRC: null}", "input.SRC"),
                ("input: {SRC: input.csv}", "input: null", "input"),
                ("path: output.csv", "path: null", "output.path"),
            ] {
                assert!(ALONE.contains(old), "{old}");
                study.write("spec.yaml", &ALONE.replacen(old, new, 1));
                let (layer, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
                assert_eq!(layer, study.path("spec.yaml"));
                assert!(
                    found.iter().any(|(code, at, _)| *code
                        == ConditionCode::SchemaInvalidFieldType
                        && at == path),
                    "{new}: {found:?}"
                );
            }
        }

        #[test]
        fn a_clear_that_names_what_it_cannot_clear_fails_at_its_entry() {
            let study = Study::new();
            study.write("base.yaml", BASE);
            // Every entry is checked, and each fault is reported at its own
            // `clear[i]` with the field the path ends at; the valid entry
            // among them is not reported.
            let paths = [
                "columns.MISSING.label",
                "columns.CODE.type",
                "columns.CODE.name",
                "input.DS",
                "yamaa",
                "type",
                "columns.REASON.derivation.mapping.dict.XX",
                "columns.REASON.derivation.mapping.source",
                "metadata.label",
                "parent",
                "columns.CODE.label",
            ];
            study.write("spec.yaml", &entry(&paths, ""));
            let (layer, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            assert_eq!(layer, study.path("spec.yaml"));
            let fields = [
                "label", "type", "name", "DS", "yamaa", "type", "XX", "source", "label", "parent",
            ];
            assert_eq!(
                found,
                fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| invalid_clear(index, field))
                    .collect::<Vec<_>>()
            );
            // A keyed member itself, and an operation keyword without the
            // field that holds it, stay.
            for (path, field) in [
                ("columns.CODE", "CODE"),
                ("columns.REASON.derivation.mapping", "mapping"),
            ] {
                study.write("spec.yaml", &entry(&[path], ""));
                let (_, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
                assert_eq!(found, [invalid_clear(0, field)], "{path}");
            }
            // A value that came from a schema default is not inherited.
            study.write(
                "base.yaml",
                &format!("{BASE}output: {{path: base.csv, columns: [CODE]}}\n"),
            );
            study.write(
                "spec.yaml",
                &entry(&["output.relative_to"], "")
                    .replace("output: {path: out.csv, columns: [CODE, REASON]}\n", ""),
            );
            let (_, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            assert_eq!(found, [invalid_clear(0, "relative_to")]);
            // A layer without a parent inherits nothing to clear.
            study.write("base.yaml", &format!("{BASE}clear: [metadata.scope]\n"));
            study.write("spec.yaml", &entry(&[], ""));
            let (layer, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            assert_eq!(layer, study.path("base.yaml"));
            assert_eq!(found, [invalid_clear(0, "scope")]);
        }

        #[test]
        fn a_clear_list_is_spec_paths_none_covering_another() {
            let study = Study::new();
            study.write("base.yaml", BASE);
            // A path outside the notation, and a path beside one of its
            // prefixes, fail when the layer is admitted, before composition.
            study.write(
                "spec.yaml",
                &entry(
                    &[
                        "columns[0].label",
                        "columns.CODE.metadata",
                        "$",
                        "columns.CODE.metadata.owner",
                        "metadata..scope",
                        "columns.CODE.metadata",
                    ],
                    "",
                ),
            );
            let (layer, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            assert_eq!(layer, study.path("spec.yaml"));
            assert_eq!(
                found,
                [
                    invalid_clear(0, "columns[0].label"),
                    invalid_clear(2, "$"),
                    invalid_clear(3, "owner"),
                    invalid_clear(4, "metadata..scope"),
                    invalid_clear(5, "metadata"),
                ]
            );
        }

        #[test]
        fn findings_name_row_templates_and_intermediates_by_id() {
            let study = Study::new();
            const ROWS: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, key: [ID]}}
output: {path: output.csv, columns: [ID]}
intermediates:
  - {id: FIRST, dataset: SRC, key: [ID], other: 1}
rows:
  - {id: week2, dataset: SRC, other: 1}
  - {id: 'week 3', dataset: SRC, other: 1}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
"#;
            study.write("spec.yaml", ROWS);
            let (_, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            let paths = found
                .iter()
                .map(|(code, path, _)| (*code, path.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(
                paths,
                [
                    (
                        ConditionCode::SchemaUnknownField,
                        "intermediates.FIRST.other"
                    ),
                    (ConditionCode::SchemaUnknownField, "rows.week2.other"),
                    (ConditionCode::SchemaUnknownField, "rows['week 3'].other"),
                ]
            );
            // A member a layer leaves incomplete is completed by no layer:
            // the resolved specification names it by identity, and a value
            // written bare by its shorthand (P-CLR-2).
            study.write(
                "spec.yaml",
                &ROWS
                    .replace("dataset: SRC, key: [ID], other: 1", "key: [ID]")
                    .replace(", other: 1}", "}")
                    .replace(
                        "derivation: SRC.ID}",
                        "derivation: {mapping: {dict: {A: B}}}}",
                    ),
            );
            let found = resolved_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            let paths = found
                .iter()
                .map(|(code, path, _)| (*code, path.as_str()))
                .collect::<Vec<_>>();
            assert!(
                paths.contains(&(
                    ConditionCode::SchemaMissingRequiredField,
                    "intermediates.FIRST.dataset"
                )),
                "{paths:?}"
            );
            assert!(
                paths.contains(&(
                    ConditionCode::SchemaMissingRequiredField,
                    "columns.ID.derivation.mapping.source"
                )),
                "{paths:?}"
            );
        }

        #[test]
        fn resolve_writes_out_a_1_0_chain_with_the_origin_of_every_value() {
            use yamaa_adapters::resolved_artifact::model::{
                Location, OriginKind, RelativeTo, ResolvedArtifact, ResourceRole,
            };
            let study = Study::new();
            study.write(
                "shared/base.yaml",
                &format!(
                    "{}output: {{path: out.csv, columns: [CODE]}}\n",
                    BASE.replace(
                        "DS: {path: ds.csv,",
                        "DS: {path: ds.csv, relative_to: entry,"
                    )
                    .replace(
                        "dict: {AE: ADVERSE EVENT, PD: PROGRESSIVE DISEASE, 'NOT DONE': NOT DONE}",
                        "dict: codes.yaml",
                    )
                ),
            );
            study.write(
                "study/spec.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/base.yaml\nclear: [columns.CODE.label]\ndomain: TEST\nkeys: [CODE]\n",
            );
            let prepared = prepare(&study.path("study/spec.yaml")).unwrap();
            // Each location is the written path read from its base's
            // directory.
            let artifact = resolved_artifact::versioned(&prepared, None, |base, written| {
                Ok::<_, yamaa_adapters::file_resources::Error>(identity(&lexical(
                    &Path::new(base).parent().unwrap().join(written),
                )))
            })
            .unwrap();
            let [entry] = artifact.entries() else {
                panic!("one entry");
            };
            let base = study.path("shared/base.yaml");
            let spec = study.path("study/spec.yaml");
            // The entry records the chain's version, and its model is the
            // resolved specification, without `parent` or `clear`.
            assert_eq!(entry.language_version.as_deref(), Some("1.0"));
            assert_eq!(entry.schema_version, None);
            assert_eq!(entry.layers, [base.clone(), spec.clone()]);
            assert_eq!(entry.specification, resolved(&prepared));
            let origin = |pointer: &str| {
                let origin = &entry.origins[pointer];
                (origin.declaring_source.clone(), origin.source_kind.clone())
            };
            assert_eq!(
                origin("/columns/1/derivation/value/mapping/unmapped"),
                (Some(base.clone()), OriginKind::Authored)
            );
            assert_eq!(
                origin("/domain"),
                (Some(spec.clone()), OriginKind::Authored)
            );
            assert_eq!(origin("/output/relative_to").1, OriginKind::SchemaDefault);
            assert_eq!(origin(""), (None, OriginKind::Composed));
            // Each resource keeps its layer's spelling and the base its
            // declaration selects (FILE-2).
            let resources = entry
                .resources
                .iter()
                .map(|resource| {
                    let Location::Local { path } = &resource.location;
                    (
                        resource.spec_path.as_str(),
                        resource.role.clone(),
                        resource.declaring_source.as_str(),
                        resource.relative_to.clone(),
                        resource.written.as_str(),
                        path.as_str(),
                    )
                })
                .collect::<Vec<_>>();
            let located = |name: &str| study.path(name);
            assert_eq!(
                resources,
                [
                    (
                        "columns.REASON.derivation.mapping.dict",
                        ResourceRole::MappingDictionary,
                        base.as_str(),
                        RelativeTo::Declaring,
                        "codes.yaml",
                        located("shared/codes.yaml").as_str(),
                    ),
                    (
                        "input.DS.path",
                        ResourceRole::InputData,
                        base.as_str(),
                        RelativeTo::Entry,
                        "ds.csv",
                        located("study/ds.csv").as_str(),
                    ),
                    (
                        "output.path",
                        ResourceRole::OutputData,
                        base.as_str(),
                        RelativeTo::Declaring,
                        "out.csv",
                        located("shared/out.csv").as_str(),
                    ),
                    (
                        "parent",
                        ResourceRole::ParentSpecification,
                        spec.as_str(),
                        RelativeTo::Declaring,
                        "../shared/base.yaml",
                        located("shared/base.yaml").as_str(),
                    ),
                ]
            );
            // The artifact reads back as written, naming the language
            // version and no bundle version.
            let json = artifact.to_json().unwrap();
            assert!(json.contains("\"language_version\":\"1.0\""));
            assert!(!json.contains("schema_version"));
            assert_eq!(ResolvedArtifact::from_json(&json).unwrap(), artifact);
        }

        /// The converted `negative-property-clear`, as `tools/migrate` writes it.
        const CONVERTED_PARENT: &str = "yamaa: \"1.0\"\ntype: spec\ninput:\n  DM: ../input/dm.csv\nbase: DM\n\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n";
        const CONVERTED_ENTRY: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\nparent: layers/parent.yaml\nclear:\n  # null clears the type inherited from layers/parent.yaml, but a column\n  # must have a type, so it cannot be cleared.\n  - columns.USUBJID.type\ndomain: ADSL\nkeys: [USUBJID]\n\noutput:\n  path: adsl.csv\n  columns: [USUBJID]\n";

        #[test]
        fn the_converted_negative_property_clear_fails_as_its_source_at_clear_0() {
            // The 0.5 source, checked as a public call checks it.
            let source = identity(
                &repository()
                    .join("benchmarks/negative-property-clear/spec.yaml")
                    .canonicalize()
                    .unwrap(),
            );
            let issues = yamaa_adapters::file_application::check(
                Language::Python,
                Request {
                    specification: &source,
                    environment: None,
                },
                Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "test",
                    specification: &source,
                    base_directory: ".",
                },
            );
            let [legacy] = issues.as_slice() else {
                panic!("one issue: {issues:?}");
            };
            assert_eq!(legacy.spec_paths, ["columns.USUBJID.type"]);
            // The conversion fails with the same condition, rule and context,
            // at the clear entry the null became.
            let study = Study::new();
            study.write("input/dm.csv", "USUBJID\n01\n");
            study.write("layers/parent.yaml", CONVERTED_PARENT);
            study.write("spec.yaml", CONVERTED_ENTRY);
            let (layer, found) = layer_findings(prepare(&study.path("spec.yaml")).unwrap_err());
            assert_eq!(layer, study.path("spec.yaml"));
            assert_eq!(found, [invalid_clear(0, "type")]);
            let definition = ConditionCode::SchemaLayerInvalidClear.definition();
            assert_eq!(
                (
                    definition.phase,
                    definition.condition,
                    definition.requirement
                ),
                (legacy.phase(), legacy.condition(), legacy.requirement())
            );
            // The 0.5 issue names the same field; the public projection adds
            // the layer it is in, which is the entry in both.
            assert_eq!(legacy.context["field"], json(r#""type""#));
            assert_eq!(legacy.context["source"], Value::String(source.clone()));
        }

        #[test]
        fn the_converted_schema_inheritance_clear_resolves_as_its_source() {
            let benchmark = repository().join("benchmarks/schema-inheritance-clear");
            let source = identity(&benchmark.join("spec_study.yaml").canonicalize().unwrap());
            let artifact =
                resolved_artifact::resolve(&source, None, Language::Python, None).unwrap();
            let legacy = &artifact.entries()[0].specification;
            // The converter's output for the benchmark: its YAML values are
            // the example's (`tools/tests/test_migrate.py`).
            let converted =
                repository().join("docs/design/interface-1.0/examples/schema-inheritance-clear");
            let entry = identity(&converted.join("spec_study.yaml").canonicalize().unwrap());
            let prepared = prepare(&entry).unwrap();
            let spec = resolved(&prepared);
            // Every field but the header resolves to the same value, the
            // cleared PD entry gone and the other three translations kept.
            for pointer in [
                "/base",
                "/domain",
                "/input",
                "/keys",
                "/metadata",
                "/output",
                "/columns/1",
                "/columns/0/name",
                "/columns/0/type",
                "/columns/0/label",
            ] {
                assert_eq!(at(&spec, pointer), at(legacy, pointer), "{pointer}");
            }
            assert_eq!(
                at(&spec, "/columns/1/derivation/value/mapping/dict")
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<_>>(),
                ["AE", "LTFU", "WBS"]
            );
            // The bare reference stays text until the 1.0 front end lowers
            // it to the source binding 0.5 writes.
            assert_eq!(
                at(legacy, "/columns/0/derivation/value/source/variable"),
                &json(r#""DS.USUBJID""#)
            );
            assert_eq!(
                at(&spec, "/columns/0/derivation/value"),
                &json(r#""DS.USUBJID""#)
            );
            // The header: `schema_version` becomes `yamaa` and `type`, and
            // the converter writes the entry's `model`.
            assert_eq!(at(legacy, "/schema_version"), &json(r#""1.0""#));
            let header = spec.as_object().unwrap();
            assert_eq!(
                (&header["yamaa"], &header["type"], &header["model"]),
                (&json(r#""1.0""#), &json(r#""spec""#), &json(r#""ADaM""#))
            );
            assert_eq!(
                spec.as_object().unwrap().len(),
                legacy.as_object().unwrap().len() + 2
            );
            // Each value keeps the layer that wrote it, as in 0.5.
            let file = |identity: &str| identity.rsplit('/').next().unwrap().to_string();
            let legacy_origins = &artifact.entries()[0].origins;
            for (path, pointer, layer) in [
                (
                    "columns.DCSREAS.derivation.mapping.dict.AE",
                    "/columns/1/derivation/value/mapping/dict/AE",
                    "spec_organization.yaml",
                ),
                ("metadata.scope", "/metadata/scope", "spec_study.yaml"),
                ("domain", "/domain", "spec_study.yaml"),
            ] {
                assert_eq!(
                    prepared.declaring_source(path).map(file).as_deref(),
                    Some(layer),
                    "{path}"
                );
                assert_eq!(
                    legacy_origins[pointer]
                        .declaring_source
                        .as_deref()
                        .map(file)
                        .as_deref(),
                    Some(layer),
                    "{pointer}"
                );
            }
        }
    }
}
