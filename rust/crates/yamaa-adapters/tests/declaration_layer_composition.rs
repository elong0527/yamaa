#![cfg(feature = "yaml")]
//! Composition of 1.0 specification layers (LAYER-1 to LAYER-10, LAYER-12,
//! P-CLR-1, P-CLR-2, P-NULL-1).
//!
//! A layer's `clear` removes values it inherits before its own fields
//! compose, YAML null is the missing value and never removes anything, and
//! the resolved specification keeps the provenance of every value. A layer's
//! fragments compose after its parent chain and before its own fields, each
//! fragment read from its own directory.
//!
//! What the public check or save of a layered study reports is a case of
//! `fixtures/diagnostics/layer_composition.json`: a refused null, `clear` or
//! `include`, a keyed member without its identity, findings named by `id`, a
//! resolved specification without `output`, the converted
//! `negative-property-clear`, and the output of a window composed by name.
//! The tests left here compare the resolved specification, the layer that
//! declares each value, or the resolved artifact, which no case states.
mod declaration {
    mod layer_composition {
        use crate::support::Edit as _;
        use std::fs;
        use std::path::{Component, Path, PathBuf};

        use yamaa_adapters::document::{self, Closures, PreparedSpecification, SpecificationError};
        use yamaa_adapters::resolved_artifact::{self, model::specification_value};

        use yamaa_adapters::specification_source::Source;
        use yamaa_core::inheritance::Source as GraphSource;
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::resolver::{Captured, Failure, Name, Resolver};
        use yamaa_core::resource::ResourceFailure;

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("layer-composition");
                Self(path)
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
        impl Resolver for Files {
            type Error = std::io::Error;
            fn inspect(&mut self, name: Name<'_>) -> Result<GraphSource, Self::Error> {
                let Name::Written {
                    declaring, written, ..
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
                let candidate = Path::new(declaring).parent().unwrap().join(written);
                let canonical = yamaa_adapters::canonical_path::canonicalize(&candidate)?;
                let identity = identity(&canonical);
                Ok(GraphSource {
                    display_path: identity.clone(),
                    identity,
                })
            }
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, Self::Error> {
                let Name::Located { source, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                Ok(Captured::resource(fs::read(&source.identity)?.into(), true))
            }
            /// A file that cannot be located is missing; a read error stays
            /// the host's own.
            fn failure(&self, name: Name<'_>, _: &Self::Error) -> Option<Failure> {
                matches!(name, Name::Written { .. })
                    .then_some(Failure::Resource(ResourceFailure::Missing))
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
                Ok(document) => document.prepare_specification(identity.into(), &mut Files),
                Err(error) => panic!("{identity}: {error:?}"),
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
                text = text.edit(
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

        /// Seam: the resolved specification's values and the layer that
        /// declares each one (`declaring_source`), which no case states.
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

        /// Seam: the resolved specification's `relative_to` and the path it
        /// respells from the entry, which no case states.
        #[test]
        fn a_cleared_field_takes_its_default_again_and_its_path_its_base() {
            let study = Study::new();
            study.write(
                "shared/base.yaml",
                &BASE.edit(
                    "DS: {path: ds.csv,",
                    "DS: {path: ds.csv, relative_to: entry,",
                ),
            );
            let entry = |clear: &[&str]| {
                entry(clear, "").edit("parent: base.yaml", "parent: ../shared/base.yaml")
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

        /// Seam: the resolved specification's mapping and derivation values,
        /// which no case states.
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

        /// Seam: the resolved artifact of a chain: its entry, the origin of
        /// each value, its resources and its JSON round trip, which no case
        /// states.
        #[test]
        fn resolve_writes_out_a_1_0_chain_with_the_origin_of_every_value() {
            use yamaa_adapters::resolved_artifact::model::{
                Location, OriginKind, RelativeTo, ResolvedArtifact, ResourceRole,
            };
            let study = Study::new();
            study.write(
                "shared/base.yaml",
                &format!(
                    "{}output: {{path: out.csv, columns: [CODE, REASON]}}\n",
                    BASE.edit(
                        "DS: {path: ds.csv,",
                        "DS: {path: ds.csv, relative_to: entry,"
                    )
                    .edit(
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
            assert_eq!(entry.language_version, "1.0");
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

        /// A parent whose `PREV` reads the previous `VAL` in the window
        /// `VISITS`, and whose `RANK` numbers the rows in the window `OTHER`.
        const WINDOWED: &str = r#"yamaa: "1.0"
type: spec
domain: OUT
keys: [ID]
input: {SRC: {path: input.csv, types: {SEQ: int, VAL: int}}}
windows:
  VISITS: {group_by: [G], order_by: [SEQ]}
  OTHER: {group_by: [G], order_by: [VAL]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: G, type: str, derivation: SRC.G}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: VAL, type: int, derivation: SRC.VAL}
  - {name: PREV, type: int, derivation: {row_value: {source: VAL, offset: -1, window: VISITS}}}
  - {name: RANK, type: int, derivation: {row_number: {window: OTHER}}}
output: {path: out.csv, columns: [ID, PREV, RANK]}
"#;

        /// The entry of a layered study: `base.yaml` as its parent, then
        /// `header`, then its own fields.
        fn including(header: &str, rest: &str) -> String {
            ENTRY.edit(
                "parent: base.yaml\n",
                &format!("parent: base.yaml\n{header}"),
            ) + rest
        }

        /// Seam: the layers a prepared chain composes, in order, the resolved
        /// specification's values and the layer that declares each one,
        /// which no case states.
        #[test]
        fn fragments_compose_after_the_parent_chain_and_before_their_layer() {
            let study = Study::new();
            study.write("base.yaml", BASE);
            // The first fragment adds a column and reads the source from its
            // own directory; the second changes what the first wrote.
            study.write(
                "shared/first.yaml",
                "yamaa: \"1.0\"\ntype: spec\ninput:\n  DS: data/ds.csv\ncolumns:\n  \
                 - {name: CODE, metadata: {owner: first}}\n  \
                 - {name: EXTRA, type: str, label: Extra, derivation: \"'A'\"}\n",
            );
            study.write(
                "shared/second.yaml",
                "yamaa: \"1.0\"\ntype: spec\ncolumns:\n  - {name: CODE, metadata: {owner: second}}\n",
            );
            study.write(
                "spec.yaml",
                &including(
                    "include:\n  - shared/first.yaml\n  - shared/second.yaml\n",
                    "columns:\n  - {name: EXTRA, label: Entry Extra}\n",
                )
                .edit("columns: [CODE, REASON]", "columns: [CODE, REASON, EXTRA]"),
            );
            let prepared = prepare(&study.path("spec.yaml")).unwrap();
            let (base, first, second, entry) = (
                study.path("base.yaml"),
                study.path("shared/first.yaml"),
                study.path("shared/second.yaml"),
                study.path("spec.yaml"),
            );
            assert_eq!(
                prepared
                    .layers()
                    .iter()
                    .map(|layer| layer.source.identity.as_str())
                    .collect::<Vec<_>>(),
                [
                    base.as_str(),
                    first.as_str(),
                    second.as_str(),
                    entry.as_str()
                ]
            );
            let spec = resolved(&prepared);
            // The later fragment wins over the earlier, and the entry over
            // both; a mapping still merges key by key (LAYER-6).
            assert_eq!(
                at(&spec, "/columns/0/metadata"),
                &json(r#"{"origin":"collected","owner":"second"}"#)
            );
            assert_eq!(at(&spec, "/columns/2/label"), &json(r#""Entry Extra""#));
            // The fragment's path is read beside the fragment and respelled
            // from the entry (LAYER-10).
            assert_eq!(
                at(&spec, "/input/DS/path"),
                &json(r#""shared/data/ds.csv""#)
            );
            let root = spec.as_object().unwrap();
            assert!(!root.contains_key("include") && !root.contains_key("parent"));
            for (path, layer) in [
                ("columns.CODE.metadata.owner", &second),
                ("columns.CODE.metadata.origin", &base),
                ("columns.EXTRA.derivation", &first),
                ("columns.EXTRA.label", &entry),
                ("input.DS.path", &first),
            ] {
                assert_eq!(
                    prepared.declaring_source(path),
                    Some(layer.as_str()),
                    "{path}"
                );
            }
        }

        /// Seam: the resolved specification's `metadata` once a fragment's
        /// `clear` applies, which no case states. The same fragment on a base
        /// without the scope fails in the fragment, the case
        /// `layer_composition.json::a_fragment_clear_naming_nothing_inherited_fails_in_the_fragment`.
        #[test]
        fn a_fragment_clear_applies_to_what_the_layers_before_it_compose() {
            let study = Study::new();
            study.write(
                "shared/unscoped.yaml",
                "yamaa: \"1.0\"\ntype: spec\nclear: [metadata.scope]\n",
            );
            study.write(
                "spec.yaml",
                &including("include: [shared/unscoped.yaml]\n", ""),
            );
            // On a base that writes the scope, the fragment clears it.
            study.write("base.yaml", BASE);
            let spec = resolved(&prepare(&study.path("spec.yaml")).unwrap());
            assert_eq!(at(&spec, "/metadata"), &json(r#"{"owner":"base"}"#));
        }

        /// Seam: the resolved artifact's layers, its parent and include
        /// resources and the origin of an included value, which no case
        /// states.
        #[test]
        fn resolve_lists_each_layer_and_each_include_entry_in_contribution_order() {
            use yamaa_adapters::resolved_artifact::model::{Location, RelativeTo, ResourceRole};
            let study = Study::new();
            study.write("base.yaml", BASE);
            study.write(
                "shared/labels.yaml",
                "yamaa: \"1.0\"\ntype: spec\ncolumns:\n  - {name: CODE, label: Collected Code}\n",
            );
            study.write(
                "spec.yaml",
                &including("include: [shared/labels.yaml]\n", ""),
            );
            let prepared = prepare(&study.path("spec.yaml")).unwrap();
            let artifact = resolved_artifact::versioned(&prepared, None, |base, written| {
                Ok::<_, yamaa_adapters::file_resources::Error>(identity(&lexical(
                    &Path::new(base).parent().unwrap().join(written),
                )))
            })
            .unwrap();
            let [entry] = artifact.entries() else {
                panic!("one entry");
            };
            let (base, labels, spec) = (
                study.path("base.yaml"),
                study.path("shared/labels.yaml"),
                study.path("spec.yaml"),
            );
            assert_eq!(entry.layers, [base.clone(), labels.clone(), spec.clone()]);
            let layered = entry
                .resources
                .iter()
                .filter(|resource| {
                    matches!(
                        resource.role,
                        ResourceRole::ParentSpecification | ResourceRole::IncludedSpecification
                    )
                })
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
            assert_eq!(
                layered,
                [
                    (
                        "include[0]",
                        ResourceRole::IncludedSpecification,
                        spec.as_str(),
                        RelativeTo::Declaring,
                        "shared/labels.yaml",
                        labels.as_str(),
                    ),
                    (
                        "parent",
                        ResourceRole::ParentSpecification,
                        spec.as_str(),
                        RelativeTo::Declaring,
                        "base.yaml",
                        base.as_str(),
                    ),
                ]
            );
            let origin = &entry.origins["/columns/0/label"];
            assert_eq!(origin.declaring_source.as_deref(), Some(labels.as_str()));
        }

        /// Seam: the composed window definitions the resolved specification
        /// holds inline, which no case states. What the composed windows
        /// save is the case
        /// `layer_composition.json::a_named_window_merged_by_name_saves_every_reference_by_the_final_definition`.
        /// A later definition of an inherited window merges with it by name,
        /// keeping the settings it does not write and the other inherited
        /// windows, and the inherited reference reads the composed definition
        /// (LAYER-8, LAYER-6).
        #[test]
        fn a_named_window_composes_by_name_and_every_reference_uses_the_final_definition() {
            let study = Study::new();
            study.write("base.yaml", WINDOWED);
            study.write(
                "spec.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\nwindows:\n  VISITS:\n    order_by: [{variable: SEQ, nulls: first}]\n",
            );
            // The resolved specification holds each reference's composed
            // definition inline (WINDOW-2).
            let spec = resolved(&prepare(&study.path("spec.yaml")).unwrap());
            let previous = "/columns/4/derivation/value/row_value/window";
            let rank = "/columns/5/derivation/value/row_number/window";
            for (pointer, expected) in [
                (format!("{previous}/group_by"), r#"["G"]"#),
                (format!("{previous}/order_by/0/variable"), r#""SEQ""#),
                (format!("{previous}/order_by/0/nulls"), r#""first""#),
                (format!("{rank}/order_by/0/variable"), r#""VAL""#),
            ] {
                assert_eq!(at(&spec, &pointer), &json(expected), "{pointer}");
            }
        }

        /// Seam: the resolved specification's `output`, which the parent
        /// contributes, and which no case states.
        /// Requiredness applies to the pruned resolved specification, never to
        /// one layer (LAYER-12): an entry that omits `output` resolves when its
        /// parent contributes it. When no layer contributes `output`, the
        /// resolved specification fails at `output`, the case
        /// `layer_composition.json::a_resolved_specification_no_layer_gives_output_misses_it_at_the_root`.
        #[test]
        fn an_entry_that_omits_output_resolves_with_the_output_its_parent_contributes() {
            let study = Study::new();
            let output = "output: {path: out.csv, columns: [CODE, REASON]}\n";
            study.write("base.yaml", &format!("{BASE}{output}"));
            study.write("spec.yaml", &ENTRY.edit(output, ""));
            let prepared = prepare(&study.path("spec.yaml")).unwrap();
            assert_eq!(
                at(&resolved(&prepared), "/output/path"),
                &json(r#""out.csv""#)
            );
        }
    }
}
