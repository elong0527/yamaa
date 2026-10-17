//! Independently authored arenas reach submission detectors and portable boundary policy.
#![allow(unused_imports)]
pub(crate) use crate::support::document::Tree;
pub(crate) use std::{collections::BTreeSet, convert::Infallible};
pub(crate) use yamaa_core::json::Value as JsonValue;
pub(crate) use yamaa_core::{
    define_diagnostics::{self, TargetRelation},
    diagnostic::ConditionCode,
    project_terminology as ct,
    submission_composition::{self as composition, Finding, Input, Study},
    submission_document, submission_json, submission_xml,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};

pub(crate) fn metadata() -> Tree {
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
pub(crate) fn column(name: &str) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text("str")),
        ("label", Tree::text("Independent column")),
        ("derivation", Tree::map(vec![("literal", Tree::text("X"))])),
        ("submission", metadata()),
    ])
}
pub(crate) fn specification() -> Tree {
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
pub(crate) fn environment() -> Tree {
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
pub(crate) fn document_reference(id: &str, kind: &str) -> Tree {
    Tree::map(vec![
        ("id", Tree::text(id)),
        ("kind", Tree::text(kind)),
        ("link", Tree::text("form.pdf")),
        ("title", Tree::text("Independent form")),
    ])
}
pub(crate) fn catalogue(standard: bool, value: Value) -> ct::Catalogue {
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
pub(crate) fn compose(
    env: Tree,
    docs: Vec<Tree>,
    catalogue: &ct::Catalogue,
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    let env = env.document();
    let declared = submission_document::decode(&env, env.root()).unwrap();
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
pub(crate) fn one(env: Tree, spec: Tree, catalogue: &ct::Catalogue) -> Result<Study, Vec<Finding>> {
    compose(env, vec![spec], catalogue, "1.0", "2026-09-30T00:00:00")
}
pub(crate) fn reached_finding(
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
pub(crate) fn row(
    id: &str,
    code: &str,
    field: &'static str,
    metadata: Tree,
    literal: bool,
) -> Tree {
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
pub(crate) fn value_spec() -> Tree {
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
pub(crate) struct Held {
    pub(crate) schema: TableSchema,
    pub(crate) value: Value,
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
    pub(crate) use ConditionCode::*;
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
            VerificationNotMissingRedundant,
            "redundant_declaration",
            "REQ-0375",
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
            SubmissionKeyMandatory,
            "value_not_permitted",
            "REQ-0881",
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
        (SubmissionClassFamily, "value_not_permitted", "REQ-0882", 25),
        (
            VerificationUniqueRedundant,
            "redundant_declaration",
            "REQ-0381",
            26,
        ),
        (
            CodelistAllowedValuesRedundant,
            "redundant_declaration",
            "REQ-0955",
            27,
        ),
        (
            SubmissionMandatoryRedundant,
            "redundant_declaration",
            "REQ-0916",
            28,
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
            // A key column is mandatory on every row; a `not_missing` restates it.
            4 => spec.set(
                &["columns", "0", "verifications"],
                Tree::list(vec![Tree::map(vec![("not_missing", Tree::map(vec![]))])]),
            ),
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
            17 => spec.at(&meta).set(&["mandatory"], Tree::boolean(false)),
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
            25 => *spec.at(&["submission", "class"]) = Tree::text("BASIC DATA STRUCTURE"),
            26 => spec.set(
                &["verifications"],
                Tree::list(vec![Tree::map(vec![(
                    "unique",
                    Tree::list(vec![Tree::text("VALUE")]),
                )])]),
            ),
            27 => {
                lists = catalogue(false, Value::Str("X".into()));
                spec.at(&meta).set(&["codelist"], Tree::text("LIST"));
                spec.set(
                    &["columns", "0", "verifications"],
                    Tree::list(vec![Tree::map(vec![(
                        "allowed_values",
                        Tree::map(vec![("values", Tree::list(vec![Tree::text("X")]))]),
                    )])]),
                );
            }
            // A declared `mandatory: true` on a non-key column already requires
            // every value; a `not_missing` beside it restates it.
            28 => {
                let mut extra = column("EXTRA");
                extra
                    .at(&["submission"])
                    .set(&["mandatory"], Tree::boolean(true));
                extra.set(
                    &["verifications"],
                    Tree::list(vec![Tree::map(vec![("not_missing", Tree::map(vec![]))])]),
                );
                *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), extra]);
                *spec.at(&["output", "columns"]) =
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("EXTRA")]);
            }
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
                assert_eq!(finding.context[key], JsonValue::String(value.into()));
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
    let finding = submission_json::admit(&study, &study.datasets()[0], &table).unwrap_err();
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
    let finding = submission_json::admit(&study, &study.datasets()[0], &table).unwrap_err();
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
    assert_eq!(
        finding.context["row"],
        JsonValue::Number(("0").parse().expect("canonical integer"))
    );
    assert_eq!(
        finding.context["value"],
        JsonValue::String("UNKNOWN_DATASET".into())
    );
    assert_eq!(
        finding.context["key_columns"],
        JsonValue::Array(vec![JsonValue::String("VALUE".into())])
    );
    assert_eq!(
        finding.context["key_values"],
        JsonValue::Array(vec![JsonValue::String("UNKNOWN_DATASET".into())])
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
        "REQ-0962",
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
                JsonValue::String("A and B name one retained target".into())
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
