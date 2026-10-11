//! Original YAML reaches core compilation; expectations are authored from the
//! six unchanged documents, not emitted by a compiler or the legacy plan codec.
use std::{fs, path::PathBuf};
use yamaa_adapters::{
    shipped_schema,
    specification_source::{InheritancePort, PreparedDocument, Source},
};
use yamaa_core::{
    bound_expression::{Binding, Read},
    dataset::{DatasetPlan, Expression, RowMode, WindowKind},
    schema::DocumentNode as N,
    specification::{PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::ColumnType as T,
};
use yamaa_engine::inheritance::{Source as Identity, SourceError};

struct Parents {
    root: PathBuf,
    reads: Vec<String>,
    unsupported: bool,
}
impl InheritancePort for Parents {
    type Error = ();
    fn canonicalize(&mut self, _: &str, written: &str) -> Result<Identity, SourceError<()>> {
        assert!(
            written.ends_with(".yaml"),
            "preparation acquired study authority"
        );
        Ok(Identity {
            identity: written.into(),
            display_path: written.into(),
        })
    }
    fn capture(&mut self, source: &Identity, maximum: usize) -> Result<Vec<u8>, SourceError<()>> {
        assert!(
            source.identity.ends_with(".yaml"),
            "compiler read study data"
        );
        self.reads.push(source.identity.clone());
        let mut bytes = fs::read(self.root.join(&source.identity)).unwrap();
        if self.unsupported && source.identity == "spec_organization.yaml" {
            bytes = String::from_utf8(bytes)
                .unwrap()
                .replace("input/lb.csv", "input/lb.xlsx")
                .into_bytes();
        }
        assert!(bytes.len() <= maximum);
        Ok(bytes)
    }
    fn rebase(
        &mut self,
        _: &Identity,
        _: &Identity,
        written: &str,
        _: usize,
    ) -> Result<String, ()> {
        Ok(written.into())
    }
}
fn prepare(name: &str, unsupported: bool) -> (PreparedDocument, Parents) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../benchmarks")
        .join(name);
    let entry = if name == "schema-inheritance" {
        "spec_study.yaml"
    } else {
        "spec.yaml"
    };
    let mut parents = Parents {
        root,
        reads: vec![],
        unsupported,
    };
    let document = shipped_schema::prepare(
        Source {
            identity: entry.into(),
            bytes: fs::read(parents.root.join(entry)).unwrap(),
        },
        entry.into(),
        &mut parents,
    )
    .unwrap();
    (document, parents)
}
fn schema(columns: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
fn names(plan: &DatasetPlan) -> Vec<&str> {
    plan.output()
        .columns()
        .iter()
        .map(|column| column.name.as_str())
        .collect()
}
fn provenance(document: &PreparedDocument, plan: &DatasetPlan, inherited: bool) {
    for assignment in plan
        .templates()
        .iter()
        .flat_map(|template| &template.assignments)
        .chain(plan.columns())
    {
        let declaring = document.declaring_source(&assignment.path).unwrap();
        assert_eq!(
            declaring,
            if inherited {
                "spec_organization.yaml"
            } else {
                "spec.yaml"
            }
        );
    }
    assert_eq!(
        document.raw().locations.len(),
        document.raw().document.nodes().len()
    );
    for parent in document.parents() {
        assert_eq!(
            parent.raw().locations.len(),
            parent.raw().document.nodes().len()
        );
        assert_eq!(
            parent.source().bytes,
            fs::read(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks/schema-inheritance")
                    .join(&parent.source().identity)
            )
            .unwrap()
        );
    }
}

#[test]
fn all_six_original_yaml_documents_compile_against_independent_model_expectations() {
    type Case<'a> = (
        &'a str,
        Vec<TableSchema>,
        &'a [&'a str],
        &'a [usize],
        usize,
        usize,
    );
    let cases: [Case<'_>; 6] = [
        (
            "negative-zero-division",
            vec![schema(&[
                ("STUDYID", T::Str),
                ("USUBJID", T::Str),
                ("LBTESTCD", T::Str),
                ("LBSTRESN", T::Str),
                ("LBBLRESN", T::Str),
            ])],
            &["STUDYID", "USUBJID", "PARAMCD", "AVAL", "BASE", "PCHG"],
            &[0, 1, 2],
            1,
            0,
        ),
        (
            "negative-integer-overflow",
            vec![schema(&[
                ("STUDYID", T::Str),
                ("USUBJID", T::Str),
                ("CELLCNT", T::Str),
                ("VOLUML", T::Str),
            ])],
            &["STUDYID", "USUBJID", "CELLCNT", "VOLUML", "CELLTOT"],
            &[0, 1],
            1,
            0,
        ),
        (
            "adam-adlb-ordered-sum",
            vec![schema(&[
                ("STUDYID", T::Str),
                ("USUBJID", T::Str),
                ("LBSEQ", T::Str),
                ("LBTESTCD", T::Str),
                ("LBTEST", T::Str),
                ("VISIT", T::Str),
                ("LBSTRESN", T::Float),
            ])],
            &[
                "STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL", "DTYPE",
            ],
            &[0, 1, 3, 2],
            2,
            2,
        ),
        (
            "schema-window-functions",
            vec![schema(&[
                ("STUDYID", T::Str),
                ("USUBJID", T::Str),
                ("VISITN", T::Int),
                ("VISIT", T::Str),
                ("VSDTC", T::Date),
                ("TRTSDT", T::Date),
                ("VSSTRESN", T::Float),
                ("VSEVAL", T::Str),
            ])],
            &[
                "STUDYID", "USUBJID", "VISITN", "VISIT", "VSDTC", "TRTSDT", "VSSTRESN", "VSEVAL",
                "VSSEQ", "SEVRANKC", "SEVRANKD", "PREVRES", "NEXTRES", "EVALPREV", "LASTRES",
                "CARRYRES", "BLFL",
            ],
            &[1, 2],
            1,
            0,
        ),
        (
            "schema-lookup",
            vec![
                schema(&[("USUBJID", T::Str)]),
                schema(&[
                    ("USUBJID", T::Str),
                    ("AEDECOD", T::Str),
                    ("AEOUT", T::Str),
                    ("AEDY", T::Str),
                ]),
                schema(&[("LLTNAME", T::Str), ("PTNAME", T::Str)]),
            ],
            &["USUBJID", "DTHDY", "DTHCAUS", "DTHPTERM"],
            &[0],
            1,
            1,
        ),
        (
            "schema-inheritance",
            vec![schema(&[
                ("USUBJID", T::Str),
                ("LBTESTCD", T::Str),
                ("LBSTRESN", T::Float),
                ("LBSTRESU", T::Str),
            ])],
            &["USUBJID", "PARAMCD", "AVAL", "AVALU"],
            &[0, 1],
            1,
            1,
        ),
    ];
    for (name, schemas, output, keys, templates, checks) in cases {
        let (document, parents) = prepare(name, false);
        // Only YAML parent captures are possible. No study bytes are supplied;
        // binding receives independently enumerated immutable metadata.
        assert_eq!(
            parents.reads,
            if name == "schema-inheritance" {
                vec!["spec_organization.yaml", "spec_compound.yaml"]
            } else {
                vec![]
            }
        );
        let compiled = PreparedSpecification::prepare(document.model()).unwrap();
        let plan = compiled
            .bind_sources(&schemas.iter().collect::<Vec<_>>())
            .unwrap();
        assert_eq!(names(&plan), output, "{name}");
        let output_types = match name {
            "negative-zero-division" => vec![T::Str, T::Str, T::Str, T::Float, T::Float, T::Float],
            "negative-integer-overflow" => vec![T::Str, T::Str, T::Int, T::Int, T::Int],
            "adam-adlb-ordered-sum" => {
                vec![T::Str, T::Str, T::Str, T::Str, T::Str, T::Float, T::Str]
            }
            "schema-window-functions" => vec![
                T::Str,
                T::Str,
                T::Int,
                T::Str,
                T::Date,
                T::Date,
                T::Float,
                T::Str,
                T::Int,
                T::Int,
                T::Int,
                T::Float,
                T::Float,
                T::Float,
                T::Float,
                T::Float,
                T::Str,
            ],
            "schema-lookup" => vec![T::Str, T::Int, T::Str, T::Str],
            "schema-inheritance" => vec![T::Str, T::Str, T::Float, T::Str],
            _ => unreachable!(),
        };
        assert_eq!(
            plan.output()
                .columns()
                .iter()
                .map(|column| column.kind)
                .collect::<Vec<_>>(),
            output_types,
            "{name}"
        );
        let projection = match name {
            "negative-integer-overflow" => vec!["STUDYID", "USUBJID", "CELLTOT"],
            "adam-adlb-ordered-sum" => vec![
                "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "DTYPE",
            ],
            _ => output.to_vec(),
        };
        assert_eq!(compiled.projection(), projection, "{name}");
        assert_eq!(plan.keys(), keys, "{name}");
        assert_eq!(plan.templates().len(), templates, "{name}");
        assert_eq!(plan.verifications().len(), checks, "{name}");
        provenance(&document, &plan, name == "schema-inheritance");
        match name {
            "negative-zero-division" | "negative-integer-overflow" => {
                let Expression::Compute(numeric) = &plan.columns().last().unwrap().expression
                else {
                    panic!("numeric derivation")
                };
                assert_eq!(
                    numeric.expression().expression(),
                    if name == "negative-zero-division" {
                        "100 * (AVAL - BASE) / BASE"
                    } else {
                        "CELLCNT * VOLUML"
                    }
                );
                let bindings = if name == "negative-zero-division" {
                    vec![
                        Binding {
                            name: "AVAL".into(),
                            read: Read::Column(3),
                        },
                        Binding {
                            name: "BASE".into(),
                            read: Read::Column(4),
                        },
                    ]
                } else {
                    vec![
                        Binding {
                            name: "CELLCNT".into(),
                            read: Read::Column(2),
                        },
                        Binding {
                            name: "VOLUML".into(),
                            read: Read::Column(3),
                        },
                    ]
                };
                assert_eq!(numeric.bindings(), bindings);
            }
            "adam-adlb-ordered-sum" => {
                assert_eq!(plan.templates()[0].mode, RowMode::Records);
                assert_eq!(plan.templates()[1].mode, RowMode::Groups(vec![0, 1, 5]));
                let total = plan.templates()[1]
                    .assignments
                    .iter()
                    .find(|assignment| assignment.column == 5)
                    .unwrap();
                assert!(
                    matches!(&total.expression, Expression::Reduce { column: 6, reducer: yamaa_core::reduction::NumericReducer::Sum, text, .. } if text == "SUM(LB.LBSTRESN)")
                );
            }
            "schema-window-functions" => {
                let kinds = plan
                    .columns()
                    .iter()
                    .filter_map(|assignment| match &assignment.expression {
                        Expression::Window(window) => Some(window.kind),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    kinds,
                    vec![
                        WindowKind::RowNumber,
                        WindowKind::Competition,
                        WindowKind::Dense,
                        WindowKind::RowValue {
                            column: 6,
                            offset: -1
                        },
                        WindowKind::RowValue {
                            column: 6,
                            offset: 1
                        },
                        WindowKind::RowValue {
                            column: 6,
                            offset: -1
                        },
                        WindowKind::PreviousNonMissing { column: 6 },
                        WindowKind::Locf { column: 6 },
                        WindowKind::BaselineFlag {
                            date: 4,
                            reference_date: 5
                        }
                    ]
                );
            }
            "schema-lookup" => {
                assert_eq!(
                    plan.intermediates()
                        .iter()
                        .map(|item| item.identifier.as_str())
                        .collect::<Vec<_>>(),
                    ["DEATHEV", "MEDDRA_PT"]
                );
                assert_eq!(
                    plan.intermediates()[0].keys,
                    [yamaa_core::dataset::IntermediateKey {
                        source_column: 0,
                        path: "intermediates[0].key".into(),
                        value: yamaa_core::dataset::MatchValue::Variable(
                            yamaa_core::dataset::MatchRead::Output {
                                column: 0,
                                identifier: "USUBJID".into()
                            }
                        ),
                    }]
                );
                assert_eq!(
                    plan.intermediates()[1].keys,
                    [yamaa_core::dataset::IntermediateKey {
                        source_column: 0,
                        path: "intermediates[1].key".into(),
                        value: yamaa_core::dataset::MatchValue::Variable(
                            yamaa_core::dataset::MatchRead::Output {
                                column: 2,
                                identifier: "DTHCAUS".into()
                            }
                        ),
                    }]
                );
                assert_eq!(
                    plan.secondary()
                        .iter()
                        .map(|source| source.name.as_str())
                        .collect::<Vec<_>>(),
                    ["AE", "MEDDRA"]
                );
            }
            "schema-inheritance" => {
                assert_eq!(
                    compiled
                        .sources()
                        .iter()
                        .map(|source| source.name.as_str())
                        .collect::<Vec<_>>(),
                    ["LB"]
                );
                assert_eq!(
                    document.declaring_source("columns.AVAL.label"),
                    Some("spec_study.yaml")
                );
                assert_eq!(
                    document.declaring_source("input.LB.types"),
                    Some("spec_compound.yaml")
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn inherited_unsupported_operations_retain_written_layer_and_location_before_study_effects() {
    let (document, parents) = prepare("schema-inheritance", true);
    let Err(PrepareError::Unsupported(features)) = PreparedSpecification::prepare(document.model())
    else {
        panic!("unsupported source profile")
    };
    assert_eq!(features.len(), 1);
    assert_eq!(
        (features[0].operation.as_str(), features[0].path.as_str()),
        ("source_format", "input.LB.path")
    );
    assert_eq!(
        document.declaring_source(&features[0].path),
        Some("spec_organization.yaml")
    );
    let parent = document
        .parents()
        .iter()
        .find(|parent| parent.source().identity == "spec_organization.yaml")
        .unwrap();
    let raw = &parent.raw().document;
    let input = raw.field(raw.root(), "input").unwrap();
    let node = raw.field(input, "LB").unwrap();
    assert_eq!(raw.nodes()[node], N::Text("input/lb.xlsx".into()));
    let position = parent.raw().locations[node];
    assert_eq!(position.line, 3);
    assert!(position.offset < parent.source().bytes.len());
    assert_eq!(
        parents.reads,
        ["spec_organization.yaml", "spec_compound.yaml"]
    );
}

/// Temporary protocol admission keeps earlier grammar/slot/form faults ahead
/// of later coordinate, malformed scalar and policy faults; bad IPC is never read.
#[test]
fn legacy_mixed_faults_preserve_authored_admission_precedence() {
    use serde_json::json;
    use yamaa_adapters::dataset_transport::{execute_dataset, DatasetTransportError as E};
    let request = json!({
        "protocol":"dataset/1", "source":[{"name":"ID","kind":"int"}],
        "output":[{"name":"ID","kind":"int"},{"name":"VALUE","kind":"int"}],
        "templates":[{"mode":{"records":null},"assignments":[
            {"column":0,"path":"columns.ID.derivation","expression":{"source":0}}
        ]}],
        "columns":[{"column":1,"path":"columns.VALUE.derivation","expression":{
            "compute":{"text":"ID +","bindings":[{"name":"ID","read":{"column":0}}]}
        }}], "keys":[0], "verifications":[]
    });
    let fail = |value: serde_json::Value| {
        assert_eq!(
            execute_dataset(&value.to_string(), b"invalid IPC").err(),
            Some(E::InvalidPlan)
        );
    };
    let mut value = request.clone();
    value["verifications"] = json!([{"path":"verification","check":{"unique":[999]}}]);
    fail(value);
    let mut value = request.clone();
    value["verifications"] = json!([{"path":"x".repeat(1025),"check":{"unique":[0]}}]);
    fail(value);
    let mut value = request.clone();
    value["unconvertible"] =
        json!([{"assignment_path":"absent","path":"x".repeat(1025),"value":{"int":"0"}}]);
    fail(value);
    let mut value = request.clone();
    value["columns"][0]["expression"]["compute"]["bindings"][0]["name"] = json!("x".repeat(257));
    fail(value);
    let mut value = request.clone();
    value["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"column":1,"path":"later","expression":{"literal":{"int":"bad"}}}));
    fail(value);
    let mut value = request.clone();
    value["columns"][0]["expression"] = json!({"function":{"slot":999,"arguments":[{"name":"value","input":{"literal":{"int":"bad"}}}]}});
    fail(value);
    for (form, kind) in [
        ("number", json!({"row_value":{"column":0,"offset":"bad"}})),
        ("window", json!("dense")),
    ] {
        let mut value = request.clone();
        value["columns"][0]["expression"] =
            json!({form:{"kind":kind,"group_by":vec![0;65],"order_by":[]}});
        fail(value);
    }
    // Wire-wide policy admission has always preceded expression compilation.
    let mut value = request;
    value["keys"] = json!(vec![0; 65]);
    assert_eq!(
        execute_dataset(&value.to_string(), b"invalid IPC").err(),
        Some(E::RequestLimit)
    );
}
