//! Independent resolution controls over original entry preparation and schema links.
//!
//! The seam is the `ResolvedArtifact` that `resolved_artifact::resolve`
//! returns, its locations, origins and resource roles compared with direct
//! preparation, and its wire validation: no case operation returns it.
#![cfg(feature = "yaml")]
use crate::support::Edit as _;
use std::fs;
use yamaa_application::resolved_artifact::model::{
    specification_value, Location, OriginKind, RelativeTo, ResolvedArtifact, ResourceRole,
};
use yamaa_application::{file_workflow, resolved_artifact};
use yamaa_core::{build::Request, json::Value, project_function::Language};

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::canonical("resolved");
        Self(root)
    }
    fn path(&self, path: &str) -> String {
        self.0.join(path).to_str().unwrap().replace('\\', "/")
    }
    fn write(&self, path: &str, text: &str) {
        let file = self.0.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    fn resolve(&self, path: &str) -> ResolvedArtifact {
        resolved_artifact::resolve(&self.path(path), None, Some(Language::Python), None).unwrap()
    }
}

const PRODUCER: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: PRODUCER\nkeys: [ID]\ninput: {RAW: {path: input/raw.csv, relative_to: entry, types: {ID: int}}}\ncolumns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\noutput: {path: output/produced.csv, relative_to: entry, columns: [ID]}\n";
const CONSUMER: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: CONSUMER\nkeys: [ID]\ninput: {P: {path: output/produced.csv, schema: producer.yaml, relative_to: entry}}\ncolumns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]\noutput: {path: output/final.csv, relative_to: entry, columns: [ID], report: output/report.csv}\n";

#[test]
fn two_studies_and_their_schema_links_match_the_actual_direct_preparation() {
    let study = Study::new();
    study.write("shared/consumer.yaml", CONSUMER);
    study.write("shared/producer.yaml", PRODUCER);
    for wrapper in ["A", "B"] {
        study.write(
            &format!("{wrapper}/spec.yaml"),
            "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/consumer.yaml\n",
        );
        study.write(
            &format!("{wrapper}/producer.yaml"),
            "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/producer.yaml\n",
        );
        let path = study.path(&format!("{wrapper}/spec.yaml"));
        let prepared = file_workflow::select_request(
            Request {
                specification: &path,
                environment: None,
            },
            Some(Language::Python),
        )
        .unwrap();
        let artifact = study.resolve(&format!("{wrapper}/spec.yaml"));
        assert_eq!(artifact, prepared.resolved_specification().unwrap());
        // A linked producer is type metadata only: it is not an entry.
        assert_eq!(artifact.entries().len(), 1);
        let consumer = &artifact.entries()[0];
        let (specification, order) =
            specification_value(prepared.run().document().document()).unwrap();
        assert_eq!(consumer.specification, specification);
        assert_eq!(consumer.mapping_order, order);
        for resource in &consumer.resources {
            assert_eq!(resource.owning_entry, consumer.entry);
        }
        let schema = consumer
            .resources
            .iter()
            .find(|resource| resource.role == ResourceRole::ProducerSchema)
            .unwrap();
        assert_eq!(schema.spec_path, "input.P.schema");
        assert_eq!(schema.declaring_source, study.path("shared/consumer.yaml"));
        assert_eq!(schema.base_source, consumer.entry);
        assert_eq!(schema.relative_to, RelativeTo::Entry);
        assert_eq!(
            schema.location,
            Location::Local {
                path: study.path(&format!("{wrapper}/producer.yaml"))
            }
        );
        let input = consumer
            .resources
            .iter()
            .find(|resource| resource.role == ResourceRole::InputData)
            .unwrap();
        assert_eq!(
            input.location,
            Location::Local {
                path: study.path(&format!("{wrapper}/output/produced.csv"))
            }
        );
        let output = consumer
            .resources
            .iter()
            .find(|resource| resource.role == ResourceRole::OutputData)
            .unwrap();
        assert_eq!(
            output.location,
            Location::Local {
                path: study.path(&format!("{wrapper}/output/final.csv"))
            }
        );
        // Neither absent data nor nonexistent output directories are accessed.
        assert!(!study.0.join(wrapper).join("input").exists());
        assert!(!study.0.join(wrapper).join("output").exists());
        assert_eq!(
            consumer.origins["/columns/0/label"].declaring_source,
            Some(study.path("shared/consumer.yaml"))
        );
        assert!(consumer
            .origins
            .values()
            .any(|origin| origin.source_kind == OriginKind::SchemaDefault));
        assert_eq!(
            artifact.to_json().unwrap(),
            study
                .resolve(&format!("{wrapper}/spec.yaml"))
                .to_json()
                .unwrap()
        );
    }
}

#[test]
fn selector_only_overrides_and_atomic_output_keep_original_paths_and_selected_roots() {
    let study = Study::new();
    study.write("shared/base.yaml", "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: {path: ../data/raw.csv, types: {ID: int}}}\ncolumns: [{name: ID, type: int, label: Original, derivation: RAW.ID}]\noutput: {path: ../data/base.csv, columns: [ID]}\n");
    study.write("study/spec.yaml", "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/base.yaml\ninput: {RAW: {relative_to: entry}}\ncolumns: [{name: ID, label: Override}]\noutput: {path: ../result/final.csv, columns: [ID], relative_to: entry, report: ../result/report.csv}\n");
    let artifact = study.resolve("study/spec.yaml");
    let entry = &artifact.entries()[0];
    let input = entry
        .resources
        .iter()
        .find(|resource| resource.role == ResourceRole::InputData)
        .unwrap();
    assert_eq!(input.written, "../data/raw.csv");
    assert_eq!(input.declaring_source, study.path("shared/base.yaml"));
    assert_eq!(input.base_source, study.path("study/spec.yaml"));
    assert_eq!(
        input.location,
        Location::Local {
            path: study.path("data/raw.csv")
        }
    );
    assert_eq!(
        entry.origins["/columns/0/label"].declaring_source,
        Some(study.path("study/spec.yaml"))
    );
    assert_eq!(
        entry.origins["/columns/0/type"].declaring_source,
        Some(study.path("shared/base.yaml"))
    );
    for role in [ResourceRole::OutputData, ResourceRole::Report] {
        let resource = entry
            .resources
            .iter()
            .find(|resource| resource.role == role)
            .unwrap();
        assert_eq!(resource.relative_to, RelativeTo::Entry);
        assert_eq!(resource.declaring_source, study.path("study/spec.yaml"));
        assert_eq!(resource.base_source, study.path("study/spec.yaml"));
    }
    // Default declaring resolution is different from the selector-only override.
    study.write(
        "study/default.yaml",
        "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/base.yaml\n",
    );
    let default = study.resolve("study/default.yaml");
    let resource = default.entries()[0]
        .resources
        .iter()
        .find(|resource| resource.role == ResourceRole::InputData)
        .unwrap();
    assert_eq!(resource.relative_to, RelativeTo::Declaring);
    assert_eq!(resource.base_source, study.path("shared/base.yaml"));
}

#[test]
fn absolute_and_parent_relative_paths_are_facts_and_never_fallback_searches() {
    let study = Study::new();
    study.write("elsewhere/raw.csv", "ID\n1\n");
    let source = PRODUCER.edit("input/raw.csv", &study.path("absent/raw.csv"));
    study.write("study/spec.yaml", &source);
    let artifact = study.resolve("study/spec.yaml");
    let input = artifact.entries()[0]
        .resources
        .iter()
        .find(|resource| resource.role == ResourceRole::InputData)
        .unwrap();
    assert_eq!(input.written, study.path("absent/raw.csv"));
    assert_eq!(
        input.location,
        Location::Local {
            path: study.path("absent/raw.csv")
        }
    );
    study.write(
        "study/spec.yaml",
        &PRODUCER.edit("input/raw.csv", "../../parent/raw.csv"),
    );
    let artifact = study.resolve("study/spec.yaml");
    let input = artifact.entries()[0]
        .resources
        .iter()
        .find(|resource| resource.role == ResourceRole::InputData)
        .unwrap();
    let parent = study
        .0
        .parent()
        .unwrap()
        .join("parent/raw.csv")
        .to_str()
        .unwrap()
        .replace('\\', "/");
    assert_eq!(input.location, Location::Local { path: parent });
    // A missing linked specification still fails original metadata preparation.
    study.write("study/missing.yaml", CONSUMER);
    assert!(resolved_artifact::resolve(
        &study.path("study/missing.yaml"),
        None,
        Some(Language::Python),
        None
    )
    .is_err());
}

#[test]
fn held_facts_round_trip_after_sources_are_deleted_without_recompilation() {
    let study = Study::new();
    study.write("spec.yaml", PRODUCER);
    let artifact = study.resolve("spec.yaml");
    let json = artifact.to_json().unwrap();
    fs::remove_file(study.0.join("spec.yaml")).unwrap();
    assert_eq!(artifact, ResolvedArtifact::from_json(&json).unwrap());
    assert_eq!(
        json,
        ResolvedArtifact::from_json(&json)
            .unwrap()
            .to_json()
            .unwrap()
    );
    let mut value: Value = yamaa_adapters::json_wire::decode(&json).unwrap();
    value["artifact_version"] = Value::from(99);
    assert!(ResolvedArtifact::from_json(&yamaa_adapters::json_wire::encode(&value)).is_err());
    assert!(ResolvedArtifact::from_json(PRODUCER).is_err());
}

#[test]
fn named_dictionaries_and_mapping_order_are_retained_without_changing_the_compilation_model() {
    let study = Study::new();
    study.write("dict.yaml", "A: Alpha\nB: Beta\n");
    study.write("spec.yaml", "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\nbase: Z\ninput: {Z: z.csv, A: a.csv}\ncolumns: [{name: ID, type: str, derivation: Z.ID}, {name: X, type: str, derivation: {mapping: {source: Z.CODE, dict: dict.yaml}}}]\noutput: {path: result.csv, columns: [ID, X]}\n");
    let artifact = study.resolve("spec.yaml");
    let entry = &artifact.entries()[0];
    // Input A is read by nothing, so pruning (LAYER-11) discards it from
    // the resolved model exactly as composition would; Z's order is kept.
    assert_eq!(entry.mapping_order["/input"], ["Z"]);
    let dictionary = entry
        .resources
        .iter()
        .find(|resource| resource.role == ResourceRole::MappingDictionary)
        .unwrap();
    assert_eq!(dictionary.written, "dict.yaml");
    assert_eq!(dictionary.base_source, study.path("spec.yaml"));
    assert_eq!(
        dictionary.location,
        Location::Local {
            path: study.path("dict.yaml")
        }
    );
    let path = study.path("spec.yaml");
    let direct = file_workflow::select_request(
        Request {
            specification: &path,
            environment: None,
        },
        Some(Language::Python),
    )
    .unwrap();
    assert_eq!(artifact, direct.resolved_specification().unwrap());
    fs::remove_file(study.0.join("dict.yaml")).unwrap();
    assert!(resolved_artifact::resolve(&path, None, Some(Language::Python), None).is_err());
    assert!(ResolvedArtifact::from_json(&artifact.to_json().unwrap()).is_ok());
}

#[test]
fn wire_validation_rejects_incomplete_or_invented_ledgers_and_preserves_large_numbers() {
    let study = Study::new();
    study.write("spec.yaml", PRODUCER);
    let initial = study.resolve("spec.yaml");
    // Projection retains exact numbers independently of executable literal limits.
    use yamaa_core::schema::{Document, DocumentNode as N};
    let numbers = Document::new(
        vec![
            N::Text("large".into()),
            N::Integer("123456789012345678901234567890".into()),
            N::Text("decimal".into()),
            N::Float(1.0),
            N::Mapping(vec![(0, 1), (2, 3)]),
        ],
        4,
    )
    .unwrap();
    let (numbers, order) = specification_value(&numbers).unwrap();
    let mut entries = initial.entries().to_vec();
    let entry = &mut entries[0];
    entry.specification["examples"] = numbers;
    entry
        .mapping_order
        .get_mut("")
        .unwrap()
        .push("examples".into());
    entry
        .mapping_order
        .insert("/examples".into(), order[""].clone());
    let origin = entry.origins["/domain"].clone();
    for pointer in ["/examples", "/examples/large", "/examples/decimal"] {
        entry.origins.insert(pointer.into(), origin.clone());
    }
    let artifact = ResolvedArtifact::new(initial.entry().into(), None, entries).unwrap();
    let json = artifact.to_json().unwrap();
    assert!(json.contains("123456789012345678901234567890"));
    assert!(json.contains("\"decimal\":1.0"));
    assert_eq!(ResolvedArtifact::from_json(&json).unwrap(), artifact);
    for modification in [
        "missing_origin",
        "extra_origin",
        "missing_order",
        "extra_order",
        "duplicate_key",
        "invented_key",
        "version",
        "unknown_field",
    ] {
        let mut value: Value = yamaa_adapters::json_wire::decode(&json).unwrap();
        let entry = &mut value["entries"][0];
        match modification {
            "missing_origin" => {
                entry["origins"].as_object_mut().unwrap().remove("/domain");
            }
            "extra_origin" => {
                entry["origins"]["/not-a-field"] = entry["origins"]["/domain"].clone();
            }
            "missing_order" => {
                entry["mapping_order"]
                    .as_object_mut()
                    .unwrap()
                    .remove("/input");
            }
            "extra_order" => {
                entry["mapping_order"]["/not-a-mapping"] = Value::Array(Vec::new());
            }
            "duplicate_key" => {
                entry["mapping_order"]["/input"] = yamaa_adapters::json!(["RAW", "RAW"]);
            }
            "invented_key" => {
                entry["mapping_order"]["/input"] = yamaa_adapters::json!(["INVENTED"]);
            }
            "version" => {
                entry["language_version"] = Value::String("99".into());
            }
            "unknown_field" => {
                entry["unknown"] = Value::Null;
            }
            _ => unreachable!(),
        }
        assert!(
            ResolvedArtifact::from_json(&yamaa_adapters::json_wire::encode(&value)).is_err(),
            "{modification}"
        );
    }
    // A linked producing specification is a resource, never a second entry,
    // even when that entry is otherwise complete and self-consistent.
    let mut value: Value = yamaa_adapters::json_wire::decode(&json).unwrap();
    let mut second = value["entries"][0].clone();
    second["entry"] = Value::String("/other.yaml".into());
    second["layers"] = yamaa_adapters::json!(["/other.yaml"]);
    second["resources"] = Value::Array(Vec::new());
    let mut alone = value.clone();
    alone["entry"] = Value::String("/other.yaml".into());
    alone["entries"] = Value::Array(vec![second.clone()]);
    assert!(ResolvedArtifact::from_json(&yamaa_adapters::json_wire::encode(&alone)).is_ok());
    value["entries"].as_array_mut().unwrap().push(second);
    assert!(ResolvedArtifact::from_json(&yamaa_adapters::json_wire::encode(&value)).is_err());
}

#[test]
fn inherited_defaults_and_shorthand_keep_the_original_normalization_origins() {
    let study = Study::new();
    study.write("mapping.yaml", "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: missing.csv}\ncolumns: [{name: ID, type: str, derivation: {mapping: {source: RAW.ID, dict: {A: Alpha}}}}]\noutput: {path: result.csv, columns: [ID]}\n");
    study.write(
        "mapping-child.yaml",
        "yamaa: \"1.0\"\ntype: spec\nparent: mapping.yaml\n",
    );
    let direct = study.resolve("mapping.yaml");
    let inherited = study.resolve("mapping-child.yaml");
    let pointer = "/columns/0/derivation/value/mapping/case_sensitive";
    let expected = &direct.entries()[0].origins[pointer];
    assert_eq!(expected.source_kind, OriginKind::SchemaDefault);
    assert!(expected
        .declaring_source
        .as_ref()
        .unwrap()
        .contains("schema_expression_mapping"));
    assert_eq!(&inherited.entries()[0].origins[pointer], expected);

    study.write("shorthand.yaml", "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: missing.csv}\ncolumns: [{name: ID, type: str, derivation: RAW.ID}]\noutput: {path: result.csv, columns: [ID]}\n");
    study.write(
        "shorthand-child.yaml",
        "yamaa: \"1.0\"\ntype: spec\nparent: shorthand.yaml\n",
    );
    let direct = study.resolve("shorthand.yaml");
    let inherited = study.resolve("shorthand-child.yaml");
    // A 1.0 name is an expression's text, so the shorthands that expand are
    // the dataset's path and the derivation's long form (D18).
    for pointer in [
        "/input/RAW",
        "/input/RAW/path",
        "/columns/0/derivation",
        "/columns/0/derivation/value",
    ] {
        assert_eq!(
            inherited.entries()[0].origins[pointer],
            direct.entries()[0].origins[pointer],
            "{pointer}"
        );
    }
    assert!(inherited.entries()[0].origins["/input/RAW"].generated);
    assert!(inherited.entries()[0].origins["/columns/0/derivation"].generated);
}

#[test]
fn named_window_descendants_keep_the_definition_origin_and_root_keeps_the_use_site() {
    let study = Study::new();
    study.write("base.yaml", "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: {path: missing.csv, types: {ID: int}}}\nwindows: {W: {order_by: [RAW.ID]}}\ncolumns: [{name: ID, type: int, derivation: RAW.ID}]\noutput: {path: result.csv, columns: [ID]}\n");
    study.write("child.yaml", "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\ncolumns: [{name: N, type: int, derivation: {row_number: {window: W}}}]\noutput: {path: result.csv, columns: [ID, N]}\n");
    let artifact = study.resolve("child.yaml");
    let origins = &artifact.entries()[0].origins;
    let prefix = "/columns/1/derivation/value/row_number/window";
    assert_eq!(
        origins[prefix].declaring_source,
        Some(study.path("child.yaml"))
    );
    assert_eq!(
        origins[&format!("{prefix}/order_by/0/variable")].declaring_source,
        Some(study.path("base.yaml"))
    );
    assert_eq!(
        origins[&format!("{prefix}/order_by/0/direction")].source_kind,
        OriginKind::SchemaDefault
    );
    // An omitted placement has no schema default: missing sorts lowest, so its
    // position follows direction and the resolved term leaves it absent.
    assert!(!origins.contains_key(&format!("{prefix}/order_by/0/nulls")));
}
