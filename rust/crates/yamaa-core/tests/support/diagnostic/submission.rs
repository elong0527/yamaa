//! Independently authored arenas reach submission detectors and portable boundary policy.
#![allow(unused_imports)]
use super::rendered;
pub(crate) use crate::support::document::Tree;
pub(crate) use crate::support::roots as rt;
pub(crate) use std::{collections::BTreeSet, convert::Infallible};
pub(crate) use yamaa_core::json::Value as JsonValue;
pub(crate) use yamaa_core::{
    define_diagnostics::{self, TargetRelation},
    diagnostic::ConditionCode,
    project_environment::Submission,
    project_terminology as ct,
    submission_composition::{self as composition, Finding, Input, Study},
    submission_transport,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};

/// A column's submission metadata, with its one origin (P-ORIGIN-1).
pub(crate) fn metadata() -> Tree {
    Tree::map(vec![
        ("core", Tree::text("Exp")),
        ("length", Tree::integer("8")),
        (
            "origins",
            Tree::list(vec![Tree::map(vec![
                ("type", Tree::text("Assigned")),
                ("source", Tree::text("Sponsor")),
            ])]),
        ),
    ])
}
pub(crate) fn column(name: &str) -> Tree {
    Tree::map(vec![
        ("name", Tree::text(name)),
        ("type", Tree::text("str")),
        ("label", Tree::text("Independent column")),
        ("derivation", Tree::text("'X'")),
        ("submission", metadata()),
    ])
}
/// A 1.0 specification (D4) of the `SDTM` model (D25), which the `sdtm`
/// section lists.
pub(crate) fn specification() -> Tree {
    Tree::map(vec![
        ("yamaa", Tree::text("1.0")),
        ("type", Tree::text("spec")),
        ("model", Tree::text("SDTM")),
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
/// A 1.0 project (D4) whose `sdtm` section lists `d0.yaml` under its
/// `standards` (D16).
pub(crate) fn project() -> Tree {
    let guide = Tree::map(vec![
        ("name", Tree::text("SDTMIG")),
        ("version", Tree::text("3.4")),
        ("status", Tree::text("Final")),
    ]);
    Tree::map(vec![
        ("yamaa", Tree::text("1.0")),
        ("type", Tree::text("project")),
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
                ("standards", Tree::list(vec![guide])),
                ("context", Tree::text("Other")),
                ("define", Tree::text("define.xml")),
                ("specs", Tree::list(vec![Tree::text("d0.yaml")])),
            ]),
        ),
    ])
}
/// Move the project's `sdtm` section to `adam`, under the ADaM guide its
/// specification's `model` names.
fn adam(project: &mut Tree, spec: &mut Tree) {
    let Tree::Map(fields) = project else {
        unreachable!("a project is a mapping")
    };
    let section = fields
        .iter_mut()
        .find(|(name, _)| name == "sdtm")
        .expect("the sdtm section");
    section.0 = "adam".into();
    *project.at(&["adam", "standards", "0", "name"]) = Tree::text("ADaMIG");
    *spec.at(&["model"]) = Tree::text("ADaM");
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
    ct::Catalogue::admit(vec![rt::Source {
        standard: standard.then(|| rt::Standard {
            name: "CDISC/NCI".into(),
            publishing_set: "SDTM".into(),
            version: "2026-09-30".into(),
        }),
        codelists: vec![rt::Codelist {
            id: "LIST".into(),
            name: "Independent list".into(),
            data_type: rt::CodelistType::Text,
            extensible: false,
            alias: None,
            format_name: None,
            external: None,
            items: Some(vec![rt::Item {
                value,
                decode: None,
                rank: None,
                alias: None,
                extended: false,
            }]),
        }],
    }
    .root()])
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
    let lowered = yamaa_core::front_end::v1::project::project(&project, None).unwrap();
    let declared = lowered.submission.unwrap();
    let docs: Vec<_> = docs.into_iter().map(Tree::document).collect();
    let trees: Vec<_> = docs
        .iter()
        .map(|document| yamaa_core::front_end::v1::spec::lower(document, None).into_spec())
        .collect();
    let others: Vec<_> = others.into_iter().map(Tree::document).collect();
    let others: Vec<_> = others.iter().collect();
    let inputs: Vec<_> = docs
        .iter()
        .zip(&trees)
        .map(|(document, tree)| Input {
            document,
            tree,
            source: "d0.yaml",
            artifact: "d0.csv",
            href: Some("d0.csv"),
        })
        .collect();
    let section = &declared.sections[0];
    let required = composition::Required::admit(declared.study.as_ref(), section)?;
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
    rule: &str,
) {
    let finding = findings
        .iter()
        .find(|finding| finding.code == code)
        .unwrap_or_else(|| panic!("missing {code:?}: {findings:?}"));
    let definition = finding.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        (phase, condition, Some(rule))
    );
    assert!(!finding.path.is_empty());
    assert!(finding.context.contains_key("detail"));
    reached.insert(rendered(finding));
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
    for (code, condition, rule, change) in [
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
        (SubmissionKeyMandatory, "value_not_permitted", "SUB-10", 17),
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
            1 => spec.remove(&["columns", "0", "submission", "origins"]),
            2 => {
                project.set(&["sdtm", "context"], Tree::text("Submission"));
                *spec.at(&["domain"]) = Tree::text("LONGDOMAIN");
            }
            3 => spec.remove(&["columns", "0", "submission", "core"]),
            5 => {
                adam(&mut project, &mut spec);
                *spec.at(&["submission", "class"]) = Tree::text("SUBJECT LEVEL ANALYSIS DATASET");
                spec.at(&meta).set(&["mandatory"], Tree::boolean(false));
                spec.at(&meta).set(&["role"], Tree::text("Identifier"));
            }
            6 => {
                *spec.at(&["columns", "0", "submission", "origins", "0", "source"]) =
                    Tree::text("Investigator")
            }
            7 => {
                *spec.at(&["columns", "0", "submission", "origins", "0", "type"]) =
                    Tree::text("Other")
            }
            8 => {
                *spec.at(&["columns", "0", "submission", "origins", "0", "type"]) =
                    Tree::text("Derived")
            }
            // A bare literal refutes Derived: the value is read from no input.
            9 => {
                *spec.at(&["columns", "0", "submission", "origins", "0", "type"]) =
                    Tree::text("Derived");
                spec.at(&meta)
                    .set(&["method"], Tree::text("Independent algorithm"));
            }
            10 | 11 => {
                *spec.at(&["columns", "0", "submission", "origins", "0", "type"]) =
                    Tree::text("Collected");
                *spec.at(&["columns", "0", "submission", "origins", "0", "source"]) =
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
                        &["origins", "0", "documents"],
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
                &["origins", "0", "documents"],
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
                project.set(&["sdtm", "transport"], Tree::text("xpt"));
                *spec.at(&["submission", "label"]) =
                    Tree::text("An independent dataset label beyond forty characters");
            }
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
            // An ADaM date travels as a SAS date with its one display format.
            29 => {
                adam(&mut project, &mut spec);
                *spec.at(&["submission", "class"]) = Tree::text("SUBJECT LEVEL ANALYSIS DATASET");
                spec.at(&meta).set(&["mandatory"], Tree::boolean(true));
                let mut date = column("ADT");
                *date.at(&["type"]) = Tree::text("date");
                date.remove(&["submission", "length"]);
                date.at(&["submission"])
                    .set(&["mandatory"], Tree::boolean(false));
                date.at(&["submission"])
                    .set(&["display_format"], Tree::text("YYMMDD10."));
                date.set(&["derivation"], Tree::text("SRC.ADT"));
                *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), date]);
                *spec.at(&["output", "columns"]) =
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("ADT")]);
            }
            // A numeric display format under xpt must be a SAS format.
            30 => {
                project.set(&["sdtm", "transport"], Tree::text("xpt"));
                let mut number = column("NUMBER");
                *number.at(&["type"]) = Tree::text("int");
                number.set(&["derivation"], Tree::integer("1"));
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
                project.set(&["sdtm", "transport"], Tree::text("dataset_json"));
                spec.set(&["output", "decimals"], Tree::integer("2"));
            }
            // An `sdtm` dataset carries its date and time text as `str`.
            32 => {
                let mut moment = column("DTM");
                *moment.at(&["type"]) = Tree::text("datetime");
                moment.remove(&["submission", "length"]);
                moment.set(&["derivation"], Tree::text("SRC.DTM"));
                *spec.at(&["columns"]) = Tree::list(vec![column("VALUE"), moment]);
                *spec.at(&["output", "columns"]) =
                    Tree::list(vec![Tree::text("VALUE"), Tree::text("DTM")]);
            }
            _ => unreachable!(),
        }
        let findings = compose_study(
            project,
            vec![spec],
            Vec::new(),
            &lists,
            "1.0",
            "2026-09-30T00:00:00",
        )
        .unwrap_err();
        reached_finding(&mut reached, &findings, code, "validation", condition, rule);
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
    // OUTPUT-2: composition refuses a key that is not an output column as
    // checking the specification does, at the key under its listing and
    // naming the column: a declared column that `output.columns` leaves out
    // as `internal_column_in_keys`, a name no column declares as
    // `undeclared_column`. The internal column declares no submission
    // metadata, which SUB-1 would refuse on a column that is not an output.
    for (key, declared, condition, label) in [
        (
            "INTERNAL",
            true,
            "internal_column_in_keys",
            "an internal key",
        ),
        ("MISSING", false, "undeclared_column", "an undeclared key"),
    ] {
        let mut spec = specification();
        if declared {
            let mut internal = column(key);
            internal.remove(&["submission"]);
            let Tree::List(columns) = spec.at(&["columns"]) else {
                unreachable!("columns are a list")
            };
            columns.push(internal);
        }
        *spec.at(&["keys"]) = Tree::list(vec![Tree::text("VALUE"), Tree::text(key)]);
        let findings = one(project(), spec, &empty).unwrap_err();
        let [finding] = findings.as_slice() else {
            panic!("{label} fails once: {findings:?}")
        };
        reached.insert(super::expect(
            &super::Record::record(finding),
            "validation",
            condition,
            Some("OUTPUT-2"),
            &["sdtm.specs[0].keys[1]"],
            vec![("column", super::text(key))],
        ));
    }
    // A `not_missing` check that restates a column's resolved `mandatory` is
    // refused when the specification is prepared, before composition: a key
    // column is mandatory on every row (CHECK-5), and so is a column whose
    // submission metadata declares `mandatory: true` (SUB-11). A check that
    // declares no field is written as its bare keyword (P-HND-4).
    let mut spec = specification();
    spec.set(
        &["input"],
        Tree::map(vec![(
            "SRC",
            Tree::map(vec![("path", Tree::text("src.csv"))]),
        )]),
    );
    let restated = || Tree::list(vec![Tree::text("not_missing")]);
    spec.set(&["columns", "0", "verifications"], restated());
    let mut extra = column("EXTRA");
    extra
        .at(&["submission"])
        .set(&["mandatory"], Tree::boolean(true));
    extra.set(&["verifications"], restated());
    let Tree::List(columns) = spec.at(&["columns"]) else {
        unreachable!("columns are a list")
    };
    columns.push(extra);
    *spec.at(&["output", "columns"]) = Tree::list(vec![Tree::text("VALUE"), Tree::text("EXTRA")]);
    let prepared = crate::support::specification::prepare(&spec.document());
    let Err(yamaa_core::specification::PrepareError::Invalid(findings)) = prepared else {
        panic!(
            "a restated mandatory is refused when the specification is prepared: {:?}",
            prepared.err()
        )
    };
    for (code, rule, path, column, stated_by) in [
        (
            VerificationNotMissingRedundant,
            "CHECK-5",
            "columns.VALUE.verifications[0].not_missing",
            "VALUE",
            "keys[0]",
        ),
        (
            SubmissionMandatoryRedundant,
            "SUB-11",
            "columns.EXTRA.verifications[0].not_missing",
            "EXTRA",
            "columns.EXTRA.submission.mandatory",
        ),
    ] {
        let finding = findings
            .iter()
            .map(|finding| finding.diagnostic())
            .find(|diagnostic| diagnostic.code == code)
            .unwrap_or_else(|| panic!("missing {code:?}: {findings:?}"));
        reached.insert(super::expect(
            &finding,
            "validation",
            "redundant_declaration",
            Some(rule),
            &[path],
            vec![
                ("column", super::text(column)),
                ("stated_by", super::text(stated_by)),
            ],
        ));
    }
    // DEFINE-4 and CT-6: under Submission every published item of a CDISC/NCI
    // list carries its NCI code, and a sponsor-extended item, which has none,
    // carries no alias.
    let epoch = |alias: Option<&str>| {
        let item = |value: &str, alias: Option<&str>, extended| rt::Item {
            value: Value::Str(value.into()),
            decode: None,
            rank: None,
            alias: alias.map(String::from),
            extended,
        };
        ct::Catalogue::admit(vec![rt::Source {
            standard: Some(rt::Standard {
                name: "CDISC/NCI".into(),
                publishing_set: "SDTM".into(),
                version: "2026-09-30".into(),
            }),
            codelists: vec![rt::Codelist {
                id: "LIST".into(),
                name: "Independent list".into(),
                data_type: rt::CodelistType::Text,
                extensible: true,
                alias: Some("C99079".into()),
                format_name: None,
                external: None,
                items: Some(vec![item("SCREENING", alias, false), item("X", None, true)]),
            }],
        }
        .root()])
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
                "origins",
                Tree::list(vec![Tree::map(vec![
                    ("type", Tree::text("Derived")),
                    ("source", Tree::text("Sponsor")),
                ])]),
            ),
            ("method", Tree::text("Independent algorithm")),
        ]
    };
    for (code, condition, rule, entries, path) in [
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
        reached_finding(&mut reached, &findings, code, "validation", condition, rule);
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
    *spec.at(&["columns", "0", "derivation"]) = Tree::text("P0.VALUE");
    *spec.at(&["columns", "0", "submission", "origins"]) = Tree::list(vec![Tree::map(vec![
        ("type", Tree::text("Predecessor")),
        ("description", Tree::text("P0.VALUE")),
    ])]);
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
    for (code, condition, rule, change) in [
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
                project.set(&["sdtm", "transport"], Tree::text("dataset_json"));
                *project.at(&["sdtm", "define"]) = Tree::text("out/metadata.xml");
            }
            _ => unreachable!(),
        }
        let findings = compose_study(
            project,
            docs.unwrap_or_else(|| vec![spec]),
            Vec::new(),
            &lists,
            if change == 2 { "" } else { "1.0" },
            "2026-09-30T00:00:00",
        )
        .unwrap_err();
        reached_finding(&mut reached, &findings, code, "validation", condition, rule);
    }
    // The Define-XML encoder in yamaa-adapters raises DEFINE-24 and DEFINE-25
    // through these constructors, and the adapters' encoder tests reach both
    // from authored text (`tests/submission_encoders.rs`).
    let finding = define_diagnostics::xml_whitespace("datasets.D0.columns.VALUE.label", '\t');
    assert_eq!(finding.path, "datasets.D0.columns.VALUE.label");
    assert_eq!(
        finding.context,
        [
            (
                "detail".into(),
                JsonValue::String("forbidden XML whitespace".into())
            ),
            ("whitespace".into(), JsonValue::String("tab".into())),
        ]
        .into()
    );
    reached_finding(
        &mut reached,
        &[finding],
        SubmissionXmlWhitespace,
        "validation",
        "untransportable_text",
        "DEFINE-24",
    );
    let finding = define_diagnostics::xml_invalid(
        "created",
        "CreationDateTime",
        JsonValue::String("2026-09-30T00:00:00.1".into()),
    );
    assert_eq!(finding.path, "created");
    assert_eq!(
        finding.context,
        [
            (
                "detail".into(),
                JsonValue::String("schema_violation: CreationDateTime".into())
            ),
            (
                "value".into(),
                JsonValue::String("2026-09-30T00:00:00.1".into())
            ),
        ]
        .into()
    );
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
    // A section that lists only its specifications admits for runs. Define
    // composition reports the study and the section's guide and target it
    // still needs, together, before composing any dataset.
    let mut specs_only = project();
    specs_only.remove(&["study"]);
    specs_only.remove(&["sdtm", "standards"]);
    specs_only.remove(&["sdtm", "define"]);
    let findings = one(specs_only, specification(), &empty).unwrap_err();
    // A 1.0 section's guides are its `standards` (D16).
    assert_eq!(
        findings,
        [
            Finding::cause(DefineRequiredDeclaration, "study", "study").with("field", "study"),
            Finding::cause(DefineRequiredDeclaration, "sdtm.standards", "standards")
                .with("field", "standards"),
            Finding::cause(DefineRequiredDeclaration, "sdtm.define", "define")
                .with("field", "define"),
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
        "dm.csv",
        "held paths"
    )
    .is_ok());
    for (relation, code, condition, rule) in [
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
            "dm.csv",
            "A and B name one retained target",
        )
        .unwrap_err();
        assert_eq!(finding.path, "sdtm.specs[0]");
        assert_eq!(
            finding.context,
            [
                (
                    "detail".into(),
                    JsonValue::String("A and B name one retained target".into())
                ),
                ("path".into(), JsonValue::String("dm.csv".into())),
            ]
            .into()
        );
        reached_finding(
            &mut reached,
            &[finding],
            code,
            "validation",
            condition,
            rule,
        );
    }
    // DEFINE-16: an artifact outside the document's directory names its
    // dataset, its path as written and the document's path as written.
    let finding = define_diagnostics::artifact_outside_document(
        "adam.specs[0]",
        "ADSL",
        "outside/adsl.csv",
        "package/define.xml",
    );
    let text = |value: &str| JsonValue::String(value.into());
    assert_eq!(
        (
            finding.path.as_str(),
            &finding.context,
            super::Record::record(&finding).message()
        ),
        (
            "adam.specs[0]",
            &[
                ("artifact".into(), text("outside/adsl.csv")),
                ("dataset".into(), text("ADSL")),
                ("define".into(), text("package/define.xml")),
                (
                    "detail".into(),
                    text("artifact must be inside the Define package")
                ),
            ]
            .into(),
            "the artifact outside/adsl.csv of ADSL is outside the directory of package/define.xml"
                .to_owned()
        ),
        "artifact_outside_document"
    );
    reached_finding(
        &mut reached,
        &[finding],
        DefineArtifactOutsidePackage,
        "validation",
        "artifact_outside_document",
        "DEFINE-16",
    );
    // SUB-3 (D25, D58): a section lists only 1.x specifications of its model,
    // each at its listed `model`, and composes no further dataset metadata
    // for one that fails.
    let listed = |model: Option<&str>| {
        let mut spec = specification();
        match model {
            Some(model) => *spec.at(&["model"]) = Tree::text(model),
            None => spec.remove(&["model"]),
        }
        one(project(), spec, &empty)
    };
    assert!(listed(Some("SDTM")).is_ok());
    let text = |value: &str| JsonValue::String(value.into());
    for (model, code, condition, context) in [
        (
            None,
            SubmissionModelRequired,
            "missing_required_field",
            vec![
                ("field", text("model")),
                ("model", text("SDTM")),
                ("section", text("sdtm")),
            ],
        ),
        (
            Some("ADaM"),
            SubmissionModelMismatch,
            "standard_model_mismatch",
            vec![
                ("model", text("SDTM")),
                ("section", text("sdtm")),
                ("value", text("ADaM")),
            ],
        ),
    ] {
        let findings = listed(model).unwrap_err();
        let [finding] = findings.as_slice() else {
            panic!("one finding: {findings:?}")
        };
        let definition = finding.definition();
        assert_eq!(finding.code, code);
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", condition, Some("SUB-3"))
        );
        assert_eq!(finding.path, "sdtm.specs[0].model");
        assert_eq!(
            finding.context,
            context
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect()
        );
        reached.insert(rendered(finding));
    }
    // DEFINE-6 and DEFINE-8 (D16): every guide a 1.0 section lists belongs to
    // its model under a name of its own, and a dataset binds the guide its
    // `submission.standard` names, or the first.
    let standards = |names: &[&str], selected: Option<&str>| {
        let mut project = project();
        *project.at(&["sdtm", "standards"]) = Tree::list(
            names
                .iter()
                .map(|name| {
                    Tree::map(vec![
                        ("name", Tree::text(name)),
                        ("version", Tree::text("3.4")),
                        ("status", Tree::text("Final")),
                    ])
                })
                .collect(),
        );
        let mut spec = specification();
        if let Some(selected) = selected {
            spec.at(&["submission"])
                .set(&["standard"], Tree::text(selected));
        }
        one(project, spec, &empty)
    };
    let two = ["SDTMIG", "SDTMIG-MD"];
    let study = standards(&two, Some("SDTMIG-MD")).unwrap();
    assert_eq!(study.datasets()[0].standard(), "SDTMIG-MD");
    assert_eq!(
        study
            .standards()
            .iter()
            .map(|standard| standard.name.as_str())
            .collect::<Vec<_>>(),
        two
    );
    let study = standards(&two, None).unwrap();
    assert_eq!(study.datasets()[0].standard(), "SDTMIG");
    for (names, selected, code, condition, rule, path, context) in [
        (
            ["SDTMIG", "SENDIG"],
            None,
            SectionStandardModel,
            "standard_model_mismatch",
            "DEFINE-6",
            "sdtm.standards[1].name",
            vec![
                ("model", text("SDTM")),
                ("section", text("sdtm")),
                ("value", text("SENDIG")),
            ],
        ),
        (
            ["SDTMIG", "SDTMIG"],
            None,
            SectionStandardDuplicate,
            "duplicate_identifier",
            "DEFINE-6",
            "sdtm.standards[1].name",
            vec![("identifier", text("SDTMIG"))],
        ),
        (
            two,
            Some("SDTMIG-AP"),
            SubmissionUnknownStandard,
            "unknown_standard",
            "DEFINE-8",
            "sdtm.specs[0].submission.standard",
            vec![
                ("section", text("sdtm")),
                (
                    "standards",
                    JsonValue::Array(vec![text("SDTMIG"), text("SDTMIG-MD")]),
                ),
                ("value", text("SDTMIG-AP")),
            ],
        ),
    ] {
        let findings = standards(&names, selected).unwrap_err();
        let [finding] = findings.as_slice() else {
            panic!("one finding: {findings:?}")
        };
        let definition = finding.definition();
        assert_eq!(finding.code, code);
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", condition, Some(rule))
        );
        assert_eq!(finding.path, path);
        assert_eq!(
            finding.context,
            context
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect()
        );
        reached.insert(rendered(finding));
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
/// XPT-7, XPT-8 and JSON-3 over held published rows, and XPT-10's findings.
fn transport_refusals(reached: &mut BTreeSet<ConditionCode>, empty: &ct::Catalogue) {
    use ConditionCode::*;
    let mut project = project();
    project.set(&["sdtm", "transport"], Tree::text("xpt"));
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
    *spec.at(&["columns", "0", "derivation"]) = Tree::integer("1");
    let numeric = one(project, spec, empty).unwrap();
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
    // The XPT encoder in yamaa-adapters refuses XPT-10 through these
    // constructors, and the adapters' encoder tests reach both over held rows
    // (`tests/submission_encoders.rs`).
    let text = |value: &str| JsonValue::String(value.into());
    let finding = define_diagnostics::xpt_last_row_blank(dataset, 1);
    assert_eq!(finding.path, "datasets.D0.artifact");
    assert_eq!(
        finding.context,
        [
            ("artifact".into(), text("d0.csv")),
            ("dataset".into(), text("D0")),
            ("detail".into(), text("the last row encodes to blanks only")),
            ("row".into(), JsonValue::from(1usize)),
        ]
        .into()
    );
    reached_finding(
        reached,
        &[finding],
        XptLastRowBlank,
        "output",
        "xpt_last_row_blank",
        "XPT-10",
    );
    let finding = define_diagnostics::xpt_file_too_large(dataset, 5_000_000_880);
    assert_eq!(finding.path, "datasets.D0.artifact");
    assert_eq!(
        finding.context,
        [
            ("artifact".into(), text("d0.csv")),
            ("bytes".into(), JsonValue::from(5_000_000_880u64)),
            ("dataset".into(), text("D0")),
            (
                "detail".into(),
                text("transport file larger than 5000000000 bytes")
            ),
        ]
        .into()
    );
    reached_finding(
        reached,
        &[finding],
        XptFileTooLarge,
        "output",
        "xpt_file_too_large",
        "XPT-10",
    );
}
