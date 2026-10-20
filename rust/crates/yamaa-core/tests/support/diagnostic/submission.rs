//! Independently authored arenas reach submission detectors and portable boundary policy.
#![allow(unused_imports)]
pub(crate) use crate::support::document::Tree;
pub(crate) use std::{collections::BTreeSet, convert::Infallible};
pub(crate) use yamaa_core::json::Value as JsonValue;
pub(crate) use yamaa_core::{
    define_diagnostics::{self, TargetRelation},
    diagnostic::ConditionCode,
    project_environment::Submission,
    project_terminology as ct,
    submission_composition::{self as composition, Finding, Input, Study},
    submission_document, submission_transport, submission_xml, submission_xpt,
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
pub(crate) fn project() -> Tree {
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
    project: Tree,
    docs: Vec<Tree>,
    catalogue: &ct::Catalogue,
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    compose_study(project, docs, Vec::new(), catalogue, installed, created)
}
/// Compose the first section; `others` are the specifications the study's
/// other submission sections list.
pub(crate) fn compose_study(
    project: Tree,
    docs: Vec<Tree>,
    others: Vec<Tree>,
    catalogue: &ct::Catalogue,
    installed: &str,
    created: &str,
) -> Result<Study, Vec<Finding>> {
    let project = project.document();
    let declared = submission_document::decode(&project, project.root()).unwrap();
    let docs: Vec<_> = docs.into_iter().map(Tree::document).collect();
    let others: Vec<_> = others.into_iter().map(Tree::document).collect();
    let others: Vec<_> = others.iter().collect();
    let inputs: Vec<_> = docs
        .iter()
        .map(|document| Input {
            document,
            source: "d0.yaml",
            artifact: "d0.csv",
            href: Some("d0.csv"),
        })
        .collect();
    let section = &declared.sections()[0];
    let required = composition::Required::admit(declared.study(), Some(section), section.family)?;
    composition::compose(required, catalogue, &inputs, &others, installed, created)
}
pub(crate) fn one(
    project: Tree,
    spec: Tree,
    catalogue: &ct::Catalogue,
) -> Result<Study, Vec<Finding>> {
    compose(project, vec![spec], catalogue, "1.0", "2026-09-30T00:00:00")
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
/// One value-level entry selecting the rows whose `where_column` holds one of
/// `values`, with its own metadata fields.
pub(crate) fn entry(
    id: &str,
    where_column: &str,
    values: &[&str],
    metadata: Vec<(&'static str, Tree)>,
) -> Tree {
    let mut fields = vec![
        ("id", Tree::text(id)),
        (
            "where",
            Tree::map(vec![(
                where_column,
                Tree::list(values.iter().copied().map(Tree::text).collect()),
            )]),
        ),
    ];
    fields.extend(metadata);
    Tree::map(fields)
}
/// `VALUE` carries the given entries over the rows `D0TESTCD`, a literal
/// `X`, selects.
pub(crate) fn value_spec(entries: Vec<Tree>) -> Tree {
    let mut spec = specification();
    let mut value = column("VALUE");
    value
        .at(&["submission"])
        .set(&["value_level"], Tree::list(entries));
    *spec.at(&["columns"]) = Tree::list(vec![value, column("D0TESTCD")]);
    *spec.at(&["output", "columns"]) =
        Tree::list(vec![Tree::text("VALUE"), Tree::text("D0TESTCD")]);
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
    assert!(one(project(), specification(), &empty).is_ok());
    for (code, condition, requirement, change) in [
        (
            SubmissionMetadataMissing,
            "submission_metadata_missing",
            "DEFINE-7",
            0,
        ),
        (SubmissionOriginMissing, "origin_missing", "SUB-14", 1),
        (SubmissionSasName, "sas_name_too_long", "DEFINE-22", 2),
        (
            SubmissionCoreMandatory,
            "core_mandatory_conflict",
            "SUB-10",
            3,
        ),
        (
            VerificationNotMissingRedundant,
            "redundant_declaration",
            "CHECK-5",
            4,
        ),
        (SubmissionAdamField, "value_not_permitted", "SUB-5", 5),
        (
            SubmissionOriginSource,
            "origin_source_not_admitted",
            "SUB-15",
            6,
        ),
        (
            SubmissionOriginDescription,
            "origin_description_missing",
            "SUB-16",
            7,
        ),
        (SubmissionMethodMissing, "method_missing", "SUB-18", 8),
        (
            SubmissionOriginRefuted,
            "origin_contradicts_derivation",
            "SUB-19",
            9,
        ),
        (
            SubmissionAnnotatedCrfMissing,
            "origin_document_missing",
            "DEFINE-14",
            10,
        ),
        (
            SubmissionOriginDocument,
            "origin_document_missing",
            "SUB-17",
            11,
        ),
        (
            SubmissionDataType,
            "submission_data_type_not_admitted",
            "SUB-6",
            12,
        ),
        (
            SubmissionLengthMissing,
            "submission_length_missing",
            "SUB-7",
            13,
        ),
        (
            SubmissionLengthNotApplicable,
            "submission_length_not_applicable",
            "SUB-7",
            14,
        ),
        (
            SubmissionLengthInvalid,
            "submission_length_invalid",
            "SUB-7",
            15,
        ),
        (
            SubmissionLengthConflict,
            "declared_length_conflict",
            "SUB-8",
            16,
        ),
        (
            SubmissionKeyMandatory,
            "value_not_permitted",
            "SUB-10",
            17,
        ),
        (
            SubmissionUnknownDocument,
            "unknown_document",
            "DEFINE-8",
            18,
        ),
        (
            SubmissionReservedMetadata,
            "reserved_metadata_key",
            "SUB-2",
            19,
        ),
        (
            SubmissionReferenceRepeating,
            "reference_data_repeating_conflict",
            "SUB-4",
            20,
        ),
        (
            SubmissionNoDataComment,
            "submission_metadata_missing",
            "DEFINE-17",
            21,
        ),
        (
            SubmissionLabelInvalid,
            "transport_label_invalid",
            "XPT-5",
            22,
        ),
        (
            SubmissionKeyNotOutput,
            "value_not_permitted",
            "DEFINE-7",
            23,
        ),
        (
            SubmissionColumnNotOutput,
            "submission_not_output_column",
            "SUB-1",
            24,
        ),
        (SubmissionClassFamily, "value_not_permitted", "SUB-10", 25),
        (
            VerificationUniqueRedundant,
            "redundant_declaration",
            "CHECK-10",
            26,
        ),
        (
            CodelistAllowedValuesRedundant,
            "redundant_declaration",
            "CT-9",
            27,
        ),
        (
            SubmissionMandatoryRedundant,
            "redundant_declaration",
            "SUB-11",
            28,
        ),
        (
            SubmissionDisplayFormatConflict,
            "value_not_permitted",
            "SUB-25",
            29,
        ),
        (
            XptDisplayFormatInvalid,
            "xpt_display_format_invalid",
            "XPT-9",
            30,
        ),
        (
            SubmissionTransportDecimals,
            "decimals_not_applicable",
            "JSON-5",
            31,
        ),
        (
            SubmissionDatetimeColumn,
            "value_not_permitted",
            "SUB-27",
            32,
        ),
    ] {
        let mut project = project();
        let mut spec = specification();
        let mut lists = empty.clone();
        let meta = ["columns", "0", "submission"];
        match change {
            0 => spec.remove(&["columns", "0", "submission"]),
            1 => spec.remove(&["columns", "0", "submission", "origin"]),
            2 => {
                project.set(&["sdtm", "context"], Tree::text("Submission"));
                *spec.at(&["domain"]) = Tree::text("LONGDOMAIN");
            }
            3 => spec.remove(&["columns", "0", "submission", "core"]),
            // A key column is mandatory on every row; a `not_missing` restates it.
            4 => spec.set(
                &["columns", "0", "verifications"],
                Tree::list(vec![Tree::map(vec![("not_missing", Tree::map(vec![]))])]),
            ),
            5 => {
                project = Tree::Map(match project {
                    Tree::Map(mut fields) => {
                        fields[1].0 = "adam".into();
                        fields
                    }
                    _ => unreachable!(),
                });
                *project.at(&["adam", "standard", "name"]) = Tree::text("ADaMIG");
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
            // A bare literal refutes Derived: the value is read from no input.
            9 => {
                *spec.at(&["columns", "0", "submission", "origin", "type"]) = Tree::text("Derived");
                spec.at(&meta)
                    .set(&["method"], Tree::text("Independent algorithm"));
            }
            10 | 11 => {
                *spec.at(&["columns", "0", "submission", "origin", "type"]) =
                    Tree::text("Collected");
                *spec.at(&["columns", "0", "submission", "origin", "source"]) =
                    Tree::text("Investigator");
                if change == 11 {
                    project.set(
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
            21 => spec.set(&["submission", "has_no_data"], Tree::boolean(true)),
            // A transport carries labels of at most 40 printable characters.
            22 => {
                project.set(&["sdtm", "datasets"], Tree::text("xpt"));
                *spec.at(&["submission", "label"]) =
                    Tree::text("An independent dataset label beyond forty characters");
            }
            23 => *spec.at(&["keys"]) = Tree::list(vec![Tree::text("MISSING")]),
            24 => *spec.at(&["output", "columns"]) = Tree::list(vec![]),
            25 => *spec.at(&["submission", "class"]) = Tree::text("BASIC DATA STRUCTURE"),
            // Composition reads the canonical class of the `unique` shorthand.
            26 => spec.set(
                &["verifications"],
                Tree::list(vec![Tree::map(vec![(
                    "unique",
                    Tree::map(vec![("columns", Tree::list(vec![Tree::text("VALUE")]))]),
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
            // An ADaM date travels as a SAS date with its one display format.
            29 => {
                project = Tree::Map(match project {
                    Tree::Map(mut fields) => {
                        fields[1].0 = "adam".into();
                        fields
                    }
                    _ => unreachable!(),
                });
                *project.at(&["adam", "standard", "name"]) = Tree::text("ADaMIG");
                *spec.at(&["submission", "class"]) = Tree::text("SUBJECT LEVEL ANALYSIS DATASET");
                spec.at(&meta).set(&["mandatory"], Tree::boolean(true));
                let mut date = column("ADT");
                *date.at(&["type"]) = Tree::text("date");
                date.remove(&["submission", "length"]);
                date.at(&["submission"])
                    .set(&["mandatory"], Tree::boolean(false));
                date.at(&["submission"])
                    .set(&["display_format"], Tree::text("YYMMDD10."));
                date.set(
                    &["derivation"],
                    Tree::map(vec![("source", Tree::text("SRC.ADT"))]),
                );
                *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), date]);
                *spec.at(&["output", "columns"]) =
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("ADT")]);
            }
            // A numeric display format under xpt must be a SAS format.
            30 => {
                project.set(&["sdtm", "datasets"], Tree::text("xpt"));
                let mut number = column("NUMBER");
                *number.at(&["type"]) = Tree::text("int");
                number.set(
                    &["derivation"],
                    Tree::map(vec![("literal", Tree::integer("1"))]),
                );
                number
                    .at(&["submission"])
                    .set(&["display_format"], Tree::text("8,2"));
                *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), number]);
                *spec.at(&["output", "columns"]) =
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("NUMBER")]);
            }
            // A transport re-reads the artifact, so its display digits would
            // become the submitted values.
            31 => {
                project.set(&["sdtm", "datasets"], Tree::text("json"));
                spec.set(&["output", "decimals"], Tree::integer("2"));
            }
            // An `sdtm` dataset carries its date and time text as `str`.
            32 => {
                let mut moment = column("DTM");
                *moment.at(&["type"]) = Tree::text("datetime");
                moment.remove(&["submission", "length"]);
                moment.set(
                    &["derivation"],
                    Tree::map(vec![("source", Tree::text("SRC.DTM"))]),
                );
                *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), moment]);
                *spec.at(&["output", "columns"]) =
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("DTM")]);
            }
            _ => unreachable!(),
        }
        let findings = one(project, spec, &lists).unwrap_err();
        reached_finding(
            &mut reached,
            &findings,
            code,
            "validation",
            condition,
            requirement,
        );
        if change == 31 {
            assert_eq!(findings.len(), 1, "{findings:?}");
            assert_eq!(findings[0].path, "sdtm.specs[0].output.decimals");
            for (key, value) in [
                ("dataset", JsonValue::String("D0".into())),
                ("specification", JsonValue::String("d0.yaml".into())),
                (
                    "decimals",
                    JsonValue::Number(("2").parse().expect("canonical integer")),
                ),
            ] {
                assert_eq!(findings[0].context[key], value, "{key}");
            }
        }
        if change == 32 {
            // SUB-27 reports the details of the specification's own refusal.
            let finding = findings
                .iter()
                .find(|finding| finding.code == SubmissionDatetimeColumn)
                .expect("reached");
            assert_eq!(finding.path, "sdtm.specs[0].columns.DTM.type");
            let text = |value: &str| JsonValue::String(value.into());
            for (key, value) in [
                ("detail", text("DTM")),
                ("column", text("DTM")),
                ("value", text("datetime")),
                ("class", text("SPECIAL PURPOSE")),
                (
                    "permitted",
                    JsonValue::Array(vec![text("str"), text("date")]),
                ),
            ] {
                assert_eq!(finding.context[key], value, "{key}");
            }
            assert_eq!(finding.context.len(), 5, "{:?}", finding.context);
        }
    }
    // DEFINE-4 and CT-6: under Submission every published item of a CDISC/NCI
    // list carries its NCI code, and a sponsor-extended item, which has none,
    // carries no alias.
    let epoch = |alias: Option<&str>| {
        let item = |value: &str, alias: Option<&str>, extended| ct::Item {
            value: Value::Str(value.into()),
            decode: None,
            rank: None,
            alias: alias.map(String::from),
            extended,
        };
        ct::Catalogue::admit(vec![ct::Source {
            standard: Some(ct::Standard {
                name: "CDISC/NCI".into(),
                publishing_set: "SDTM".into(),
                version: "2026-09-30".into(),
            }),
            codelists: vec![ct::Codelist {
                id: "LIST".into(),
                name: "Independent list".into(),
                data_type: ct::DataType::Text,
                extensible: true,
                alias: Some("C99079".into()),
                format_name: None,
                external: None,
                items: Some(vec![item("SCREENING", alias, false), item("X", None, true)]),
            }],
        }])
        .unwrap()
    };
    let mut env = project();
    env.set(&["sdtm", "context"], Tree::text("Submission"));
    let mut spec = specification();
    spec.set(
        &["columns", "0", "submission", "codelist"],
        Tree::text("LIST"),
    );
    assert!(one(env.clone(), spec.clone(), &epoch(Some("C48262"))).is_ok());
    let findings = one(env, spec, &epoch(None)).unwrap_err();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, SubmissionCdiscAliasMissing);
    assert_eq!(findings[0].path, "codelists.LIST");
    // A valid entry composes; each static fault and a refuted entry origin has
    // its own registered cause.
    let valid = value_spec(vec![entry("X", "D0TESTCD", &["X"], vec![])]);
    let study = one(project(), valid, &empty).unwrap();
    assert_eq!(study.datasets()[0].columns()[0].values()[0].id(), "X");
    let derived = || {
        vec![
            (
                "origin",
                Tree::map(vec![
                    ("type", Tree::text("Derived")),
                    ("source", Tree::text("Sponsor")),
                ]),
            ),
            ("method", Tree::text("Independent algorithm")),
        ]
    };
    for (code, condition, requirement, entries, path) in [
        (
            SubmissionValueWhereInvalid,
            "value_level_where_invalid",
            "SUB-12",
            vec![entry("X", "BOGUS", &["X"], vec![])],
            "sdtm.specs[0].columns.VALUE.submission.value_level.X.where.BOGUS",
        ),
        (
            SubmissionValueDuplicateId,
            "duplicate_identifier",
            "SUB-12",
            vec![
                entry("X", "D0TESTCD", &["X"], vec![]),
                entry("X", "D0TESTCD", &["Y"], vec![]),
            ],
            "sdtm.specs[0].columns.VALUE.submission.value_level.X.id",
        ),
        (
            SubmissionValueOverlap,
            "value_level_overlap",
            "SUB-12",
            vec![
                entry("X", "D0TESTCD", &["X", "Y"], vec![]),
                entry("Y", "D0TESTCD", &["Y"], vec![]),
            ],
            "sdtm.specs[0].columns.VALUE.submission.value_level.Y.where",
        ),
        // A literal value refutes Derived on the rows the entry selects.
        (
            SubmissionValueOriginRefuted,
            "refuted_value_origin",
            "SUB-13",
            vec![entry("X", "D0TESTCD", &["X"], derived())],
            "sdtm.specs[0].columns.VALUE.submission.value_level.X",
        ),
    ] {
        let findings = one(project(), value_spec(entries), &empty).unwrap_err();
        reached_finding(
            &mut reached,
            &findings,
            code,
            "validation",
            condition,
            requirement,
        );
        let finding = findings.iter().find(|f| f.code == code).unwrap();
        assert_eq!(finding.path, path);
        if code == SubmissionValueOriginRefuted {
            for (key, value) in [
                ("column", "VALUE"),
                ("entry", "X"),
                ("derivation_kind", "literal"),
                ("origin_type", "Derived"),
            ] {
                assert_eq!(finding.context[key], JsonValue::String(value.into()));
            }
        }
    }
    // A construction that derives the where column outside the entry's values
    // produces none of its rows, so it refutes nothing.
    assert!(one(
        project(),
        value_spec(vec![entry("Y", "D0TESTCD", &["Y"], derived())]),
        &empty
    )
    .is_ok());
    // A Predecessor that keeps its predecessor's name keeps its label, wherever
    // another section of the study declares that column.
    let mut spec = specification();
    spec.set(&["input"], Tree::map(vec![("P0", Tree::text("p0.csv"))]));
    *spec.at(&["columns", "0", "derivation"]) = Tree::map(vec![("source", Tree::text("P0.VALUE"))]);
    *spec.at(&["columns", "0", "submission", "origin"]) = Tree::map(vec![
        ("type", Tree::text("Predecessor")),
        ("description", Tree::text("P0.VALUE")),
    ]);
    let mut predecessor = specification();
    *predecessor.at(&["domain"]) = Tree::text("P0");
    *predecessor.at(&["columns", "0", "label"]) = Tree::text("Predecessor column");
    let created = "2026-09-30T00:00:00";
    assert!(compose_study(
        project(),
        vec![spec.clone()],
        vec![],
        &empty,
        "1.0",
        created
    )
    .is_ok());
    let findings = compose_study(
        project(),
        vec![spec],
        vec![predecessor],
        &empty,
        "1.0",
        created,
    )
    .unwrap_err();
    reached_finding(
        &mut reached,
        &findings,
        SubmissionPredecessorLabel,
        "validation",
        "predecessor_label_mismatch",
        "SUB-20",
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].path, "sdtm.specs[0].columns.VALUE.label");
    for (key, value) in [
        ("predecessor", "P0.VALUE"),
        ("label", "Independent column"),
        ("predecessor_label", "Predecessor column"),
    ] {
        assert_eq!(findings[0].context[key], JsonValue::String(value.into()));
    }
    for (code, condition, requirement, change) in [
        (
            SubmissionStandardFamily,
            "unknown_standard_family",
            "SUB-3",
            0,
        ),
        (
            SubmissionDefineExtension,
            "value_not_permitted",
            "DEFINE-2",
            1,
        ),
        (
            SubmissionSourceVersion,
            "source_system_incomplete",
            "DEFINE-12",
            2,
        ),
        (
            SubmissionDuplicateIdentifier,
            "duplicate_define_identifier",
            "DEFINE-10",
            3,
        ),
        (
            SubmissionAnnotatedCrfMultiplicity,
            "value_not_permitted",
            "DEFINE-14",
            4,
        ),
        (
            SubmissionDuplicateDataset,
            "duplicate_dataset_entry",
            "DEFINE-5",
            5,
        ),
        (
            SubmissionCdiscAliasMissing,
            "missing_required_field",
            "DEFINE-4",
            6,
        ),
        (SubmissionDefineName, "value_not_permitted", "DEFINE-2", 7),
    ] {
        let mut project = project();
        let mut spec = specification();
        let mut lists = empty.clone();
        let mut docs = None;
        match change {
            0 => *project.at(&["sdtm", "standard", "name"]) = Tree::text("ADaMIG"),
            1 => *project.at(&["sdtm", "define"]) = Tree::text("define.json"),
            2 => {}
            3 => project.set(
                &["sdtm", "documents"],
                Tree::list(vec![document_reference("D0", "other")]),
            ),
            4 => project.set(
                &["sdtm", "documents"],
                Tree::list(vec![
                    document_reference("A1", "annotated_crf"),
                    document_reference("A2", "annotated_crf"),
                ]),
            ),
            5 => {
                *project.at(&["sdtm", "specs"]) =
                    Tree::list(vec![Tree::text("d0.yaml"), Tree::text("d0.yaml")]);
                docs = Some(vec![spec.clone(), spec.clone()]);
            }
            6 => {
                project.set(&["sdtm", "context"], Tree::text("Submission"));
                lists = catalogue(true, Value::Str("X".into()));
                spec.set(
                    &["columns", "0", "submission", "codelist"],
                    Tree::text("LIST"),
                );
            }
            // A transport file sits beside a document named define.xml.
            7 => {
                project.set(&["sdtm", "datasets"], Tree::text("json"));
                *project.at(&["sdtm", "define"]) = Tree::text("out/metadata.xml");
            }
            _ => unreachable!(),
        }
        let findings = compose(
            project,
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
    let study = one(project(), spec, &empty).unwrap();
    let finding = submission_xml::render(&study).unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        SubmissionXmlWhitespace,
        "validation",
        "untransportable_text",
        "DEFINE-24",
    );
    let study = compose(
        project(),
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
        "DEFINE-25",
    );
    let study = one(project(), specification(), &empty).unwrap();
    let table = Held {
        schema: TableSchema::new(vec![Column {
            name: "VALUE".into(),
            kind: ColumnType::Str,
        }])
        .unwrap(),
        value: Value::Bool(true),
    };
    let finding = submission_transport::admit(&study, &study.datasets()[0], &table).unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        SubmissionJsonInvalid,
        "validation",
        "value_not_permitted",
        "JSON-4",
    );
    let mut spec = specification();
    spec.set(
        &["columns", "0", "submission", "inventory_vocabulary"],
        Tree::boolean(true),
    );
    let study = one(project(), spec, &empty).unwrap();
    let table = Held {
        schema: TableSchema::new(vec![Column {
            name: "VALUE".into(),
            kind: ColumnType::Str,
        }])
        .unwrap(),
        value: Value::Str("UNKNOWN_DATASET".into()),
    };
    let finding = submission_transport::admit(&study, &study.datasets()[0], &table).unwrap_err();
    reached_finding(
        &mut reached,
        std::slice::from_ref(&finding),
        SubmissionInventoryFailed,
        "verification",
        "allowed_values_failed",
        "CT-11",
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
    transport_refusals(&mut reached, &empty);
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
        "DEFINE-6",
    );
    // A section that lists only its specifications admits for runs. Define
    // composition reports the study and the section's guide and target it
    // still needs, together, before composing any dataset.
    let mut specs_only = project();
    specs_only.remove(&["study"]);
    specs_only.remove(&["sdtm", "standard"]);
    specs_only.remove(&["sdtm", "define"]);
    let findings = one(specs_only, specification(), &empty).unwrap_err();
    assert_eq!(
        findings,
        [
            Finding::cause(DefineRequiredDeclaration, "study", "study"),
            Finding::cause(DefineRequiredDeclaration, "sdtm.standard", "standard"),
            Finding::cause(DefineRequiredDeclaration, "sdtm.define", "define"),
        ]
    );
    reached_finding(
        &mut reached,
        &findings,
        DefineRequiredDeclaration,
        "validation",
        "missing_required_field",
        "DEFINE-28",
    );
    // A project without the selected family's section names that section.
    assert_eq!(
        composition::Required::admit(None, None, Submission::Adam).unwrap_err(),
        [
            Finding::cause(DefineRequiredDeclaration, "study", "study"),
            Finding::cause(DefineRequiredDeclaration, "adam", "adam"),
        ]
    );
    assert!(define_diagnostics::created_text("2026-09-30T00:00:00").is_ok());
    let finding = define_diagnostics::created_text("2026-09-30T00:00:00.1").unwrap_err();
    reached_finding(
        &mut reached,
        &[finding],
        DefineCreatedInvalid,
        "validation",
        "value_not_permitted",
        "DEFINE-3",
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
            "DEFINE-16",
        ),
        (
            TargetRelation::ArtifactCollision,
            DefineArtifactCollision,
            "artifact_path_collision",
            "JSON-2",
        ),
        (
            TargetRelation::TransportCollision,
            DefineTransportCollision,
            "transport_path_collision",
            "JSON-2",
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

/// Held rows of one typed column; a declared row count may exceed the cells
/// when only the file geometry is inspected.
pub(crate) struct Rows {
    pub(crate) schema: TableSchema,
    pub(crate) rows: usize,
    pub(crate) values: Vec<Value>,
}
impl TableAccess for Rows {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        match self.values.get(row) {
            Some(value) if column == 0 => Ok(value.into()),
            _ => Err(CellError::OutOfBounds { row, column }),
        }
    }
}
fn rows(kind: ColumnType, values: Vec<Value>) -> Rows {
    Rows {
        schema: TableSchema::new(vec![Column {
            name: "VALUE".into(),
            kind,
        }])
        .unwrap(),
        rows: values.len(),
        values,
    }
}
/// XPT-7, XPT-8, XPT-10 and JSON-3 over held published rows.
fn transport_refusals(reached: &mut BTreeSet<ConditionCode>, empty: &ct::Catalogue) {
    use ConditionCode::*;
    let mut project = project();
    project.set(&["sdtm", "datasets"], Tree::text("xpt"));
    let study = one(project.clone(), specification(), empty).unwrap();
    let dataset = &study.datasets()[0];
    let finding = submission_transport::admit(
        &study,
        dataset,
        &rows(ColumnType::Str, vec![Value::Str("caf\u{e9}".into())]),
    )
    .unwrap_err();
    assert_eq!(finding.path, "datasets.D0.columns.VALUE");
    assert_eq!(finding.context["cause"], JsonValue::String("ascii".into()));
    assert_eq!(finding.context["row"], JsonValue::from(0usize));
    reached_finding(
        reached,
        &[finding],
        TransportTextInvalid,
        "output",
        "transport_text_invalid",
        "XPT-7",
    );
    let finding =
        submission_transport::admit(&study, dataset, &rows(ColumnType::Str, vec![])).unwrap_err();
    assert_eq!(finding.path, "datasets.D0.artifact");
    reached_finding(
        reached,
        &[finding],
        SubmissionTransportEmpty,
        "output",
        "transport_dataset_empty",
        "JSON-3",
    );
    let mut spec = specification();
    *spec.at(&["columns", "0", "type"]) = Tree::text("int");
    *spec.at(&["columns", "0", "derivation"]) = Tree::map(vec![("literal", Tree::integer("1"))]);
    let numeric = one(project.clone(), spec, empty).unwrap();
    let finding = submission_transport::admit(
        &numeric,
        &numeric.datasets()[0],
        &rows(ColumnType::Int, vec![Value::Int((1 << 53) + 1)]),
    )
    .unwrap_err();
    assert_eq!(
        finding.context["cause"],
        JsonValue::String("integer".into())
    );
    reached_finding(
        reached,
        &[finding],
        XptValueOutOfRange,
        "output",
        "xpt_value_out_of_range",
        "XPT-8",
    );
    let mut measured = one(project, specification(), empty).unwrap();
    let held = rows(
        ColumnType::Str,
        vec![Value::Str("A".into()), Value::Missing],
    );
    submission_transport::measure(&mut measured, &[Some(&held)]).unwrap();
    let document = submission_xpt::document(&measured, &measured.datasets()[0]);
    assert_eq!(document.variables[0].width, 1);
    let finding = submission_xpt::admit(&document, &measured.datasets()[0], &held).unwrap_err();
    assert_eq!(finding.context["row"], JsonValue::from(1usize));
    reached_finding(
        reached,
        &[finding],
        XptLastRowBlank,
        "output",
        "xpt_last_row_blank",
        "XPT-10",
    );
    let wide = Rows {
        rows: 5_000_000_000 / document.observation(),
        ..rows(ColumnType::Str, vec![])
    };
    let finding = submission_xpt::admit(&document, &measured.datasets()[0], &wide).unwrap_err();
    reached_finding(
        reached,
        &[finding],
        XptFileTooLarge,
        "output",
        "xpt_file_too_large",
        "XPT-10",
    );
}
