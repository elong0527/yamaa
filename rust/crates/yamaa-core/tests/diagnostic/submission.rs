//! Independently authored arenas reach submission detectors and portable boundary policy.
use std::{collections::BTreeSet, convert::Infallible};
use yamaa_core::{
    define_diagnostics::{self, TargetRelation},
    diagnostic::{ConditionCode, ContextValue},
    project_terminology as ct,
    schema::{Document, DocumentLimits, DocumentNode as N},
    submission_composition::{self as composition, Finding, Input, Study},
    submission_document, submission_json, submission_xml,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
#[derive(Clone)]
enum Tree {
    Scalar(N),
    Map(Vec<(String, Tree)>),
    List(Vec<Tree>),
}
impl Tree {
    fn text(value: &str) -> Self {
        Self::Scalar(N::Text(value.into()))
    }
    fn integer(value: &str) -> Self {
        Self::Scalar(N::Integer(value.into()))
    }
    fn boolean(value: bool) -> Self {
        Self::Scalar(N::Boolean(value))
    }
    fn map(fields: Vec<(&'static str, Self)>) -> Self {
        Self::Map(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }
    fn list(items: Vec<Self>) -> Self {
        Self::List(items)
    }
    fn at(&mut self, path: &[&str]) -> &mut Self {
        let Some((field, tail)) = path.split_first() else {
            return self;
        };
        match self {
            Self::Map(fields) => fields
                .iter_mut()
                .find(|(key, _)| key == field)
                .expect(field)
                .1
                .at(tail),
            Self::List(items) => items[field.parse::<usize>().unwrap()].at(tail),
            _ => panic!("scalar path {path:?}"),
        }
    }
    fn set(&mut self, path: &[&str], value: Self) {
        let (key, head) = path.split_last().unwrap();
        match self.at(head) {
            Self::Map(fields) => {
                if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
                    *old = value;
                } else {
                    fields.push(((*key).into(), value));
                }
            }
            _ => panic!("set needs map"),
        }
    }
    fn remove(&mut self, path: &[&str]) {
        let (key, head) = path.split_last().unwrap();
        match self.at(head) {
            Self::Map(fields) => fields.retain(|(name, _)| name != key),
            _ => panic!("remove needs map"),
        }
    }
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(key, value)| (Self::text(&key).append(nodes), value.append(nodes)))
                    .collect(),
            ),
            Self::List(items) => {
                N::Sequence(items.into_iter().map(|item| item.append(nodes)).collect())
            }
        };
        let index = nodes.len();
        nodes.push(node);
        index
    }
    fn document(self) -> Document {
        let mut nodes = vec![];
        let root = self.append(&mut nodes);
        Document::new(
            nodes,
            root,
            DocumentLimits {
                text_bytes: 16_777_216,
                ..DocumentLimits::default()
            },
        )
        .unwrap()
    }
}
fn metadata() -> Tree {
    Tree::map(vec![
        ("core", Tree::text("Exp")),
        ("length", Tree::integer("8")),
        (
            "origin",
            Tree::map(vec![
                ("type", Tree::text("Assigned")),
                ("source", Tree::text("Sponsor")),
            ]),
        ),
    ])
}
fn column(name: &str) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text("str")),
        ("label", Tree::text("Independent column")),
        ("derivation", Tree::map(vec![("literal", Tree::text("X"))])),
        ("submission", metadata()),
    ])
}
fn specification() -> Tree {
    Tree::map(vec![
        ("domain", Tree::text("D0")),
        ("keys", Tree::list(vec![Tree::text("VALUE")])),
        (
            "submission",
            Tree::map(vec![
                ("label", Tree::text("Independent dataset")),
                ("class", Tree::text("SPECIAL PURPOSE")),
                ("structure", Tree::text("One record per subject")),
                ("repeating", Tree::boolean(false)),
            ]),
        ),
        ("columns", Tree::list(vec![column("VALUE")])),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("d0.csv")),
                ("columns", Tree::list(vec![Tree::text("VALUE")])),
            ]),
        ),
    ])
}
fn environment() -> Tree {
    Tree::map(vec![
        (
            "study",
            Tree::map(vec![
                ("id", Tree::text("S01")),
                ("name", Tree::text("Independent study")),
                ("description", Tree::text("Submission detector witness")),
                ("protocol_name", Tree::text("P01")),
            ]),
        ),
        (
            "sdtm",
            Tree::map(vec![
                (
                    "standard",
                    Tree::map(vec![
                        ("name", Tree::text("SDTMIG")),
                        ("version", Tree::text("3.4")),
                        ("status", Tree::text("Final")),
                    ]),
                ),
                ("context", Tree::text("Other")),
                ("define", Tree::text("define.xml")),
                ("specs", Tree::list(vec![Tree::text("d0.yaml")])),
            ]),
        ),
    ])
}
fn document_reference(id: &str, kind: &str) -> Tree {
    Tree::map(vec![
        ("id", Tree::text(id)),
        ("kind", Tree::text(kind)),
        ("link", Tree::text("form.pdf")),
        ("title", Tree::text("Independent form")),
    ])
}
fn catalogue(standard: bool, value: Value) -> ct::Catalogue {
    ct::Catalogue::admit(vec![ct::Source {
        standard: standard.then(|| ct::Standard {
            name: "CDISC/NCI".into(),
            publishing_set: "SDTM".into(),
            version: "2026-09-30".into(),
        }),
        codelists: vec![ct::Codelist {
            id: "LIST".into(),
            name: "Independent list".into(),
            data_type: ct::DataType::Text,
            extensible: false,
            alias: None,
            format_name: None,
            external: None,
            items: Some(vec![ct::Item {
                value,
                decode: None,
                rank: None,
                alias: None,
                extended: false,
            }]),
        }],
    }])
    .unwrap()
}
fn compose(
    env: Tree,
    docs: Vec<Tree>,
    catalogue: &ct::Catalogue,
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    let env = env.document();
    let declared = submission_document::decode(&env, env.root(), Default::default()).unwrap();
    let docs: Vec<_> = docs.into_iter().map(Tree::document).collect();
    let inputs: Vec<_> = docs
        .iter()
        .map(|document| Input {
            document,
            source: "d0.yaml",
            artifact: "d0.csv",
            href: "d0.csv",
        })
        .collect();
    composition::compose(
        declared.study().unwrap(),
        &declared.sections()[0],
        catalogue,
        &inputs,
        installed,
        created,
    )
}
fn one(env: Tree, spec: Tree, catalogue: &ct::Catalogue) -> Result<Study, Vec<Finding>> {
    compose(env, vec![spec], catalogue, "1.0", "2026-09-30T00:00:00")
}
fn reached_finding(
    reached: &mut BTreeSet<ConditionCode>,
    findings: &[Finding],
    code: ConditionCode,
    phase: &str,
    condition: &str,
    requirement: &str,
) {
    let finding = findings
        .iter()
        .find(|finding| finding.code == code)
        .unwrap_or_else(|| panic!("missing {code:?}: {findings:?}"));
    let definition = finding.definition();
    assert_eq!(
        (
            definition.phase,
            definition.condition,
            definition.requirement
        ),
        (phase, condition, Some(requirement))
    );
    assert!(!finding.path.is_empty());
    assert!(finding.context.contains_key("detail"));
    reached.insert(code);
}
fn row(id: &str, code: &str, field: &'static str, metadata: Tree, literal: bool) -> Tree {
    Tree::map(vec![
        ("id", Tree::text(id)),
        (
            "derivations",
            Tree::map(vec![(
                "D0TESTCD",
                Tree::map(vec![(
                    if literal { "literal" } else { "source" },
                    Tree::text(code),
                )]),
            )]),
        ),
        ("submission", Tree::map(vec![(field, metadata)])),
    ])
}
fn value_spec() -> Tree {
    let mut spec = specification();
    *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), column("D0TESTCD")]);
    *spec.at(&["output", "columns"]) =
        Tree::list(vec![Tree::text("VALUE"), Tree::text("D0TESTCD")]);
    spec.set(
        &["rows"],
        Tree::list(vec![row("row 1", "CODE", "VALUE", Tree::map(vec![]), true)]),
    );
    spec
}
struct Held {
    schema: TableSchema,
    value: Value,
}
impl TableAccess for Held {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        if row == 0 && column == 0 {
            Ok((&self.value).into())
        } else {
            Err(CellError::OutOfBounds { row, column })
        }
    }
}
pub fn reached() -> BTreeSet<ConditionCode> {
    use ConditionCode::*;
    let mut reached = BTreeSet::new();
    let empty = ct::Catalogue::admit(vec![]).unwrap();
    assert!(one(environment(), specification(), &empty).is_ok());
    for (code, condition, requirement, change) in [
        (
            SubmissionMetadataMissing,
            "submission_metadata_missing",
            "REQ-1015",
            0,
        ),
        (SubmissionOriginMissing, "origin_missing", "REQ-0918", 1),
        (SubmissionSasName, "sas_name_too_long", "REQ-1023", 2),
        (
            SubmissionCoreMandatory,
            "core_mandatory_conflict",
            "REQ-0915",
            3,
        ),
        (
            SubmissionMandatoryUnenforced,
            "mandatory_not_enforced",
            "REQ-0916",
            4,
        ),
        (SubmissionAdamField, "value_not_permitted", "REQ-0917", 5),
        (
            SubmissionOriginSource,
            "origin_source_not_admitted",
            "REQ-0919",
            6,
        ),
        (
            SubmissionOriginDescription,
            "origin_description_missing",
            "REQ-0920",
            7,
        ),
        (SubmissionMethodMissing, "method_missing", "REQ-0923", 8),
        (
            SubmissionOriginRefuted,
            "origin_contradicts_derivation",
            "REQ-0922",
            9,
        ),
        (
            SubmissionAnnotatedCrfMissing,
            "origin_document_missing",
            "REQ-0984",
            10,
        ),
        (
            SubmissionOriginDocument,
            "origin_document_missing",
            "REQ-0893",
            11,
        ),
        (
            SubmissionDataType,
            "submission_data_type_not_admitted",
            "REQ-0914",
            12,
        ),
        (
            SubmissionLengthMissing,
            "submission_length_missing",
            "REQ-0912",
            13,
        ),
        (
            SubmissionLengthNotApplicable,
            "submission_length_not_applicable",
            "REQ-0912",
            14,
        ),
        (
            SubmissionLengthInvalid,
            "submission_length_invalid",
            "REQ-0911",
            15,
        ),
        (
            SubmissionLengthConflict,
            "declared_length_conflict",
            "REQ-0913",
            16,
        ),
        (
            SubmissionAllowedValuesLiteral,
            "value_not_permitted",
            "REQ-0955",
            17,
        ),
        (
            SubmissionUnknownDocument,
            "unknown_document",
            "REQ-1016",
            18,
        ),
        (
            SubmissionReservedMetadata,
            "reserved_metadata_key",
            "REQ-0910",
            19,
        ),
        (
            SubmissionReferenceRepeating,
            "reference_data_repeating_conflict",
            "REQ-0925",
            20,
        ),
        (
            SubmissionNoDataComment,
            "submission_metadata_missing",
            "REQ-1022",
            21,
        ),
        (
            SubmissionDatasetJsonNoData,
            "dataset_json_without_data",
            "REQ-1227",
            22,
        ),
        (
            SubmissionKeyNotOutput,
            "value_not_permitted",
            "REQ-1015",
            23,
        ),
        (
            SubmissionColumnNotOutput,
            "submission_not_output_column",
            "REQ-0909",
            24,
        ),
    ] {
        let mut env = environment();
        let mut spec = specification();
        let mut lists = empty.clone();
        let meta = ["columns", "0", "submission"];
        match change {
            0 => spec.remove(&["columns", "0", "submission"]),
            1 => spec.remove(&["columns", "0", "submission", "origin"]),
            2 => {
                env.set(&["sdtm", "context"], Tree::text("Submission"));
                *spec.at(&["domain"]) = Tree::text("LONGDOMAIN");
            }
            3 => spec.remove(&["columns", "0", "submission", "core"]),
            4 => {
                *spec.at(&["keys"]) = Tree::list(vec![]);
                spec.at(&meta).set(&["mandatory"], Tree::boolean(true));
            }
            5 => {
                env = Tree::Map(match env {
                    Tree::Map(mut fields) => {
                        fields[1].0 = "adam".into();
                        fields
                    }
                    _ => unreachable!(),
                });
                *env.at(&["adam", "standard", "name"]) = Tree::text("ADaMIG");
                *spec.at(&["submission", "class"]) = Tree::text("SUBJECT LEVEL ANALYSIS DATASET");
                spec.at(&meta).set(&["mandatory"], Tree::boolean(false));
                spec.at(&meta).set(&["role"], Tree::text("Identifier"));
            }
            6 => {
                *spec.at(&["columns", "0", "submission", "origin", "source"]) =
                    Tree::text("Investigator")
            }
            7 => *spec.at(&["columns", "0", "submission", "origin", "type"]) = Tree::text("Other"),
            8 => {
                *spec.at(&["columns", "0", "submission", "origin", "type"]) = Tree::text("Derived")
            }
            9 => {
                *spec.at(&["columns", "0", "submission", "origin", "type"]) =
                    Tree::text("Collected");
                *spec.at(&["columns", "0", "submission", "origin", "source"]) =
                    Tree::text("Vendor");
            }
            10 | 11 => {
                *spec.at(&["columns", "0", "submission", "origin", "type"]) =
                    Tree::text("Collected");
                *spec.at(&["columns", "0", "submission", "origin", "source"]) =
                    Tree::text("Investigator");
                if change == 11 {
                    env.set(
                        &["sdtm", "documents"],
                        Tree::list(vec![
                            document_reference("ACRF", "annotated_crf"),
                            document_reference("OTHER", "other"),
                        ]),
                    );
                    spec.at(&meta).set(
                        &["origin", "documents"],
                        Tree::list(vec![Tree::text("OTHER")]),
                    );
                }
            }
            12 => spec.at(&meta).set(&["data_type"], Tree::text("integer")),
            13 => spec.remove(&["columns", "0", "submission", "length"]),
            14 => spec.at(&meta).set(&["data_type"], Tree::text("date")),
            15 => *spec.at(&["columns", "0", "submission", "length"]) = Tree::integer("0"),
            16 => spec.set(
                &["columns", "0", "verifications"],
                Tree::list(vec![Tree::map(vec![(
                    "max_length",
                    Tree::map(vec![("max", Tree::integer("4"))]),
                )])]),
            ),
            17 => {
                lists = catalogue(false, Value::Str("X".into()));
                spec.at(&meta).set(&["codelist"], Tree::text("LIST"));
                spec.set(
                    &["columns", "0", "verifications"],
                    Tree::list(vec![Tree::map(vec![(
                        "allowed_values",
                        Tree::map(vec![(
                            "values",
                            Tree::list(vec![Tree::map(vec![("nested", Tree::text("X"))])]),
                        )]),
                    )])]),
                );
            }
            18 => spec.at(&meta).set(
                &["origin", "documents"],
                Tree::list(vec![Tree::text("UNKNOWN")]),
            ),
            19 => spec.set(
                &["metadata"],
                Tree::map(vec![("label", Tree::text("Reserved"))]),
            ),
            20 => {
                *spec.at(&["submission", "repeating"]) = Tree::boolean(true);
                spec.set(&["submission", "reference_data"], Tree::boolean(true));
            }
            21 | 22 => {
                spec.set(&["submission", "has_no_data"], Tree::boolean(true));
                if change == 22 {
                    env.set(&["sdtm", "dataset_json"], Tree::boolean(true));
                    spec.set(&["submission", "comment"], Tree::text("No data collected"));
                }
            }
            23 => *spec.at(&["keys"]) = Tree::list(vec![Tree::text("MISSING")]),
            24 => *spec.at(&["output", "columns"]) = Tree::list(vec![]),
            _ => unreachable!(),
        }
        let findings = one(env, spec, &lists).unwrap_err();
        reached_finding(
            &mut reached,
            &findings,
            code,
            "validation",
            condition,
            requirement,
        );
    }
    for (code, condition, requirement, change) in [
        (
            SubmissionValueTestcdMissing,
            "value_metadata_requires_testcd",
            "REQ-1163",
            0,
        ),
        (
            SubmissionValueTestcdDynamic,
            "value_metadata_requires_literal_testcd",
            "REQ-1164",
            1,
        ),
        (
            SubmissionValueColumnUnknown,
            "value_metadata_unknown_column",
            "REQ-1165",
            2,
        ),
        (
            SubmissionValueColumnNotOutput,
            "value_metadata_not_output_column",
            "REQ-1166",
            3,
        ),
        (
            SubmissionValueMetadataConflict,
            "conflicting_value_metadata",
            "REQ-1167",
            4,
        ),
        (
            SubmissionValueOriginRefuted,
            "refuted_value_origin",
            "REQ-1168",
            5,
        ),
    ] {
        let mut spec = value_spec();
        match change {
            0 => *spec.at(&["columns"]) = Tree::list(vec![column("VALUE")]),
            1 => {
                *spec.at(&["rows", "0"]) =
                    row("row 1", "SOURCE.CODE", "VALUE", Tree::map(vec![]), false)
            }
            2 => *spec.at(&["rows", "0"]) = row("row 1", "CODE", "BOGUS", Tree::map(vec![]), true),
            3 => *spec.at(&["output", "columns"]) = Tree::list(vec![Tree::text("D0TESTCD")]),
            4 => {
                *spec.at(&["rows"]) = Tree::list(vec![
                    row(
                        "row 1",
                        "CODE",
                        "VALUE",
                        Tree::map(vec![("length", Tree::integer("4"))]),
                        true,
                    ),
                    row(
                        "row 2",
                        "CODE",
                        "VALUE",
                        Tree::map(vec![("length", Tree::integer("5"))]),
                        true,
                    ),
                ])
            }
            5 => {
                *spec.at(&["rows", "0"]) = row(
                    "row 1",
                    "CODE",
                    "VALUE",
                    Tree::map(vec![(
                        "origin",
                        Tree::map(vec![
                            ("type", Tree::text("Collected")),
                            ("source", Tree::text("Vendor")),
                        ]),
                    )]),
                    true,
                )
            }
            _ => unreachable!(),
        }
        let findings = one(environment(), spec, &empty).unwrap_err();
        reached_finding(
            &mut reached,
            &findings,
            code,
            "validation",
            condition,
            requirement,
        );
        if change == 5 {
            let finding = findings.iter().find(|f| f.code == code).unwrap();
            for (key, value) in [
                ("column", "VALUE"),
                ("row", "row 1"),
                ("testcd", "CODE"),
                ("derivation_kind", "literal"),
                ("origin_type", "Collected"),
            ] {
                assert_eq!(
                    finding.context[key],
                    ContextValue::Scalar(Value::Str(value.into()))
                );
            }
        }
    }
    for (code, condition, requirement, change) in [
        (
            SubmissionStandardFamily,
            "unknown_standard_family",
            "REQ-0924",
            0,
        ),
        (
            SubmissionDefineExtension,
            "value_not_permitted",
            "REQ-1014",
            1,
        ),
        (
            SubmissionSourceVersion,
            "source_system_incomplete",
            "REQ-0979",
            2,
        ),
        (
            SubmissionDuplicateIdentifier,
            "duplicate_define_identifier",
            "REQ-1017",
            3,
        ),
        (
            SubmissionAnnotatedCrfMultiplicity,
            "value_not_permitted",
            "REQ-1019",
            4,
        ),
        (
            SubmissionDuplicateDataset,
            "duplicate_dataset_entry",
            "REQ-1018",
            5,
        ),
        (
            SubmissionCdiscAliasMissing,
            "missing_required_field",
            "REQ-1004",
            6,
        ),
    ] {
        let mut env = environment();
        let mut spec = specification();
        let mut lists = empty.clone();
        let mut docs = None;
        match change {
            0 => *env.at(&["sdtm", "standard", "name"]) = Tree::text("ADaMIG"),
            1 => *env.at(&["sdtm", "define"]) = Tree::text("define.json"),
            2 => {}
            3 => env.set(
                &["sdtm", "documents"],
                Tree::list(vec![document_reference("D0", "other")]),
            ),
            4 => env.set(
                &["sdtm", "documents"],
                Tree::list(vec![
                    document_reference("A1", "annotated_crf"),
                    document_reference("A2", "annotated_crf"),
                ]),
            ),
            5 => {
                *env.at(&["sdtm", "specs"]) =
                    Tree::list(vec![Tree::text("d0.yaml"), Tree::text("d0.yaml")]);
                docs = Some(vec![spec.clone(), spec.clone()]);
            }
            6 => {
                env.set(&["sdtm", "context"], Tree::text("Submission"));
                lists = catalogue(true, Value::Str("X".into()));
                spec.set(
                    &["columns", "0", "submission", "codelist"],
                    Tree::text("LIST"),
                );
            }
            _ => unreachable!(),
        }
        let findings = compose(
            env,
            docs.unwrap_or_else(|| vec![spec]),
            &lists,
            if change == 2 { "" } else { "1.0" },
            "2026-09-30T00:00:00",
        )
        .unwrap_err();
        reached_finding(
            &mut reached,
            &findings,
            code,
            "validation",
            condition,
            requirement,
        );
    }
    let mut spec = specification();
    *spec.at(&["columns", "0", "label"]) = Tree::text("A\tB");
    let study = one(environment(), spec, &empty).unwrap();
    let finding = submission_xml::render(&study).unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        SubmissionXmlWhitespace,
        "validation",
        "untransportable_text",
        "REQ-1025",
    );
    let study = compose(
        environment(),
        vec![specification()],
        &empty,
        "1.0",
        "invalid civil timestamp",
    )
    .unwrap();
    let finding = submission_xml::render(&study).unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        SubmissionXmlInvalid,
        "validation",
        "value_not_permitted",
        "REQ-1026",
    );
    let study = one(environment(), specification(), &empty).unwrap();
    let table = Held {
        schema: TableSchema::new(vec![Column {
            name: "VALUE".into(),
            kind: ColumnType::Str,
        }])
        .unwrap(),
        value: Value::Bool(true),
    };
    let finding =
        submission_json::render(&study, &study.datasets()[0], &table, "define.xml").unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        SubmissionJsonInvalid,
        "validation",
        "value_not_permitted",
        "REQ-1231",
    );
    let mut spec = specification();
    spec.set(
        &["columns", "0", "submission", "inventory_vocabulary"],
        Tree::boolean(true),
    );
    let study = one(environment(), spec, &empty).unwrap();
    let table = Held {
        schema: TableSchema::new(vec![Column {
            name: "VALUE".into(),
            kind: ColumnType::Str,
        }])
        .unwrap(),
        value: Value::Str("UNKNOWN_DATASET".into()),
    };
    let finding =
        submission_json::render(&study, &study.datasets()[0], &table, "define.xml").unwrap_err();
    reached_finding(
        &mut reached,
        std::slice::from_ref(&finding),
        SubmissionInventoryFailed,
        "verification",
        "allowed_values_failed",
        "REQ-1156",
    );
    assert_eq!(
        finding.path,
        "datasets.D0.columns.VALUE.submission.inventory_vocabulary"
    );
    assert_eq!(finding.context.len(), 8);
    assert_eq!(finding.context["row"], ContextValue::Integer("0".into()));
    assert_eq!(
        finding.context["value"],
        ContextValue::Scalar(Value::Str("UNKNOWN_DATASET".into()))
    );
    assert_eq!(
        finding.context["key_columns"],
        ContextValue::Sequence(vec![ContextValue::Scalar(Value::Str("VALUE".into()))])
    );
    assert_eq!(
        finding.context["key_values"],
        ContextValue::Sequence(vec![ContextValue::Scalar(Value::Str(
            "UNKNOWN_DATASET".into()
        ))])
    );
    for valid in ["sdtm", "adam", "send"] {
        assert!(define_diagnostics::standard_name(valid).is_ok());
    }
    let finding = define_diagnostics::standard_name("invalid").unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        DefineStandardName,
        "validation",
        "value_not_permitted",
        "REQ-1074",
    );
    assert_eq!(
        define_diagnostics::required(Some(9), "study", "study").unwrap(),
        9
    );
    let finding = define_diagnostics::required::<usize>(None, "study", "study").unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        DefineRequiredDeclaration,
        "validation",
        "missing_required_field",
        "REQ-1015",
    );
    assert!(define_diagnostics::created_text("2026-09-30T00:00:00").is_ok());
    let finding = define_diagnostics::created_text("2026-09-30T00:00:00.1").unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        DefineCreatedInvalid,
        "validation",
        "value_not_permitted",
        "REQ-1072",
    );
    assert!(define_diagnostics::admit_relation(
        TargetRelation::Independent,
        "sdtm.specs[0]",
        "held paths"
    )
    .is_ok());
    for (relation, code, condition, requirement) in [
        (
            TargetRelation::ArtifactOutsidePackage,
            DefineArtifactOutsidePackage,
            "artifact_outside_document",
            "REQ-1021",
        ),
        (
            TargetRelation::DatasetJsonOutsidePackage,
            DefineDatasetJsonOutsidePackage,
            "artifact_outside_document",
            "REQ-1229",
        ),
        (
            TargetRelation::ArtifactCollision,
            DefineArtifactCollision,
            "artifact_path_collision",
            "REQ-1195",
        ),
        (
            TargetRelation::DatasetJsonCollision,
            DefineDatasetJsonCollision,
            "dataset_json_path_collision",
            "REQ-1226",
        ),
    ] {
        let finding = define_diagnostics::admit_relation(
            relation,
            "sdtm.specs[0]",
            "A and B name one retained target",
        )
        .unwrap_err();
        assert_eq!(finding.path, "sdtm.specs[0]");
        assert_eq!(
            finding.context,
            [(
                "detail".into(),
                ContextValue::Scalar(Value::Str("A and B name one retained target".into()))
            )]
            .into()
        );
        reached_finding(
            &mut reached,
            &[finding],
            code,
            "validation",
            condition,
            requirement,
        );
    }
    reached
}
#[test]
fn actual_binding_quota_keeps_its_path_detail_and_complete_implementation_report() {
    // Each independently owned input fits admission; their combined binding exceeds
    // the checked 16MiB text quota. This reaches BindingFinding::Limit itself.
    let long = "X".repeat(8_388_609);
    let lists = catalogue(false, Value::Str(long.clone()));
    let mut spec = specification();
    spec.set(
        &["columns", "0", "submission", "codelist"],
        Tree::text("LIST"),
    );
    spec.set(
        &["columns", "0", "verifications"],
        Tree::list(vec![Tree::map(vec![(
            "allowed_values",
            Tree::map(vec![("values", Tree::list(vec![Tree::text(&long)]))]),
        )])]),
    );
    let findings = one(environment(), spec, &lists).unwrap_err();
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.code, ConditionCode::ApplicationRejected);
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().requirement
        ),
        ("engine", "engine_rejected", None)
    );
    assert_eq!(
        finding.path,
        "sdtm.specs[0].columns.VALUE.submission.codelist"
    );
    assert_eq!(
        finding.context,
        [(
            "detail".into(),
            ContextValue::Scalar(Value::Str("LIST".into()))
        )]
        .into()
    );
    // Ordinary finite unequal sets retain their language conflict classification.
    let mut spec = specification();
    spec.set(
        &["columns", "0", "submission", "codelist"],
        Tree::text("LIST"),
    );
    spec.set(
        &["columns", "0", "verifications"],
        Tree::list(vec![Tree::map(vec![(
            "allowed_values",
            Tree::map(vec![("values", Tree::list(vec![Tree::text("Y")]))]),
        )])]),
    );
    let findings = one(
        environment(),
        spec,
        &catalogue(false, Value::Str("X".into())),
    )
    .unwrap_err();
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].code,
        ConditionCode::CodelistBindingValuesConflict
    );
    assert_eq!(findings[0].definition().requirement, Some("REQ-0955"));
}
