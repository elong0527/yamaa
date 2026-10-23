//! Exercise the original-document compiler, not constructed failure enum values.
use super::rendered;
use std::collections::BTreeSet;
use yamaa_core::json::Value as V;
use yamaa_core::{
    diagnostic::ConditionCode,
    schema::{Document, DocumentNode as N},
    specification::PrepareError,
};

use crate::support::document::Tree;
use crate::support::specification::{header, prepare};

/// The read of `SRC.ID`, written as the name alone (D18).
fn source() -> Tree {
    Tree::map(vec![("value", Tree::text("SRC.ID"))])
}
fn column(name: &str, derivation: Option<Tree>) -> Tree {
    let mut fields = vec![("name", Tree::text(name)), ("type", Tree::text("int"))];
    if let Some(derivation) = derivation {
        fields.push(("derivation", derivation));
    }
    Tree::map(fields)
}
fn inputs() -> Tree {
    Tree::map(vec![
        ("SRC", Tree::map(vec![("path", Tree::text("source.csv"))])),
        (
            "SECOND",
            Tree::map(vec![("path", Tree::text("second.csv"))]),
        ),
    ])
}
use super::text;
fn texts(values: &[&str]) -> V {
    V::Array(values.iter().map(|v| text(v)).collect())
}
type Expected<'a> = (&'a str, Option<&'a str>, Vec<&'a str>, Vec<(&'a str, V)>);

pub(crate) fn output_reached() -> BTreeSet<ConditionCode> {
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", Some(source()))],
        vec![Tree::text("ID")],
    );
    fields.push(("base", Tree::text("SRC")));
    let output = &mut fields
        .iter_mut()
        .find(|(name, _)| *name == "output")
        .unwrap()
        .1;
    *output = Tree::map(vec![
        ("path", Tree::text("result.bad")),
        (
            "columns",
            Tree::List(vec![Tree::text("ABSENT"), Tree::text("ABSENT")]),
        ),
    ]);
    let compiled = prepare(&Tree::map(fields).document()).unwrap();
    let findings = compiled.output_findings();
    let expected = [
        (
            "unknown_artifact_profile",
            "OUTPUT-5",
            "output.path",
            vec![
                ("path", text("result.bad")),
                ("permitted", texts(&[".csv", ".parquet"])),
            ],
        ),
        (
            "undeclared_column",
            "OUTPUT-1",
            "output.columns[0]",
            vec![("column", text("ABSENT"))],
        ),
        (
            "duplicate_identifier",
            "OUTPUT-1",
            "output.columns[1]",
            vec![("column", text("ABSENT"))],
        ),
        (
            "internal_column_in_keys",
            "OUTPUT-2",
            "keys[0]",
            vec![("column", text("ID"))],
        ),
    ];
    assert_eq!(findings.len(), expected.len());
    let mut reached: BTreeSet<_> = findings
        .iter()
        .zip(expected)
        .map(|(finding, (condition, rule, path, fields))| {
            let diagnostic = finding.diagnostic();
            let definition = diagnostic.definition();
            assert_eq!(
                (definition.phase, definition.condition, definition.rule),
                ("validation", condition, Some(rule))
            );
            assert_eq!(diagnostic.spec_paths, [path]);
            assert_eq!(
                diagnostic.context,
                fields.into_iter().map(|(k, v)| (k.into(), v)).collect()
            );
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            rendered(&diagnostic)
        })
        .collect();
    for (precision, path, condition, rule, context) in [
        (
            "-9223372036854775809",
            "result.csv",
            "invalid_field_type",
            "CSV-6",
            vec![
                ("expected", text("an integer from 0 through 18")),
                (
                    "actual",
                    V::Number(("-9223372036854775809").parse().expect("canonical integer")),
                ),
            ],
        ),
        (
            "2",
            "result.parquet",
            "decimals_not_applicable",
            "CSV-6",
            vec![
                ("path", text("result.parquet")),
                ("profile", text("parquet")),
            ],
        ),
    ] {
        let mut fields = base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        fields.push(("base", Tree::text("SRC")));
        fields
            .iter_mut()
            .find(|(name, _)| *name == "output")
            .unwrap()
            .1 = Tree::map(vec![
            ("path", Tree::text(path)),
            ("decimals", Tree::integer(precision)),
            ("columns", Tree::List(vec![Tree::text("ID")])),
        ]);
        let compiled = prepare(&Tree::map(fields).document()).unwrap();
        assert_eq!(compiled.output_decimals(), Some(precision));
        let findings = compiled.output_findings();
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", condition, Some(rule))
        );
        assert_eq!(diagnostic.spec_paths, ["output.decimals"]);
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        reached.insert(rendered(&diagnostic));
    }
    for (names, condition, position, name) in [
        (vec!["ID", "ID"], "duplicate_order_term", 1, "ID"),
        (vec!["SRC.ID"], "undeclared_column", 0, "SRC.ID"),
    ] {
        let mut fields = base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        fields.push(("base", Tree::text("SRC")));
        fields
            .iter_mut()
            .find(|(name, _)| *name == "output")
            .unwrap()
            .1 = Tree::map(vec![
            ("path", Tree::text("result.csv")),
            ("columns", Tree::List(vec![Tree::text("ID")])),
            (
                "order_by",
                Tree::List(
                    names
                        .into_iter()
                        .map(|name| {
                            Tree::map(vec![
                                ("variable", Tree::text(name)),
                                ("direction", Tree::text("asc")),
                                ("nulls", Tree::text("last")),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
        let compiled = prepare(&Tree::map(fields).document()).unwrap();
        let findings = compiled.output_findings();
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().rule
            ),
            ("validation", condition, Some("OUTPUT-12"))
        );
        assert_eq!(
            diagnostic.spec_paths,
            [format!("output.order_by[{position}]")]
        );
        assert_eq!(
            diagnostic.context,
            [("column".into(), text(name))].into_iter().collect()
        );
        reached.insert(rendered(&diagnostic));
    }
    let mut fields = base(
        "TEST",
        Tree::map(vec![(
            "SRC",
            Tree::map(vec![("path", Tree::text("source.csv"))]),
        )]),
        vec![column("ID", Some(source()))],
        vec![Tree::text("ID")],
    );
    fields.push(("filter", Tree::text("ID = 1")));
    let prepared = prepare(&Tree::map(fields).document()).unwrap();
    let source = yamaa_core::table::TableSchema::new(vec![yamaa_core::table::Column {
        name: "ID".into(),
        kind: yamaa_core::value::ColumnType::Int,
    }])
    .unwrap();
    let Err(yamaa_core::specification::BindError::Invalid(findings)) = prepared.bind(&source)
    else {
        panic!("root phase")
    };
    assert_eq!(findings.len(), 1);
    let diagnostic = findings[0]
        .diagnostics(prepared.source())
        .unwrap()
        .remove(0);
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "phase_boundary", Some("ROW-9"))
    );
    assert_eq!(diagnostic.spec_paths, ["filter"]);
    assert_eq!(
        diagnostic.context,
        [
            ("identifier".into(), text("ID")),
            ("available_phase".into(), text("column_derivation")),
            ("required_phase".into(), text("row_filter"))
        ]
        .into()
    );
    reached.insert(rendered(&diagnostic));
    let collision = yamaa_core::source_ordinal::Ordinal::admit(&source, 0, "ID")
        .err()
        .unwrap();
    let diagnostic = collision.finding("SRC", "ID").unwrap().diagnostic();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().rule
        ),
        ("validation", "ordinal_field_collision", Some("INPUT-4"))
    );
    assert_eq!(diagnostic.spec_paths, ["input.SRC.ordinal"]);
    assert_eq!(
        diagnostic.context,
        [
            ("dataset".into(), text("SRC")),
            ("field".into(), text("ID"))
        ]
        .into()
    );
    reached.insert(rendered(&diagnostic));
    reached
}

fn check(fields: Vec<(&str, Tree)>, expected: Vec<Expected<'_>>) -> BTreeSet<ConditionCode> {
    let document: Document = Tree::map(fields).document();
    let Err(PrepareError::Invalid(findings)) = prepare(&document) else {
        panic!("preflight error from compiler");
    };
    assert_eq!(findings.len(), expected.len());
    let mut reached = BTreeSet::new();
    for (finding, (condition, rule, paths, context)) in findings.iter().zip(expected) {
        let diagnostic = finding.diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", condition, rule)
        );
        assert_eq!(diagnostic.spec_paths, paths);
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(rendered(&diagnostic));
    }
    reached
}
fn base(domain: &str, input: Tree, columns: Vec<Tree>, keys: Vec<Tree>) -> Vec<(&str, Tree)> {
    let mut fields = header();
    fields.extend([
        ("domain", Tree::text(domain)),
        ("input", input),
        ("keys", Tree::List(keys)),
        ("columns", Tree::List(columns)),
        (
            "output",
            Tree::map(vec![
                ("path", Tree::text("result.csv")),
                ("columns", Tree::List(vec![Tree::text("ID")])),
            ]),
        ),
    ]);
    fields
}

pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut log_codes = BTreeSet::new();
    for (columns, keys, expected) in [
        (
            vec![column(
                "ID",
                Some(Tree::map(vec![(
                    "value",
                    Tree::integer("9223372036854775808"),
                )])),
            )],
            vec![Tree::text("ID")],
            (
                "integer_overflow",
                Some("VALUE-5"),
                vec!["columns.ID.derivation"],
                vec![
                    (
                        "actual",
                        V::Number(("9223372036854775808").parse().expect("canonical integer")),
                    ),
                    ("minimum", V::from(-9223372036854775808_i64)),
                    ("maximum", V::from(9223372036854775807_i64)),
                ],
            ),
        ),
        (
            vec![column("ID", Some(source()))],
            vec![],
            (
                "invalid_field_type",
                Some("OUTPUT-2"),
                vec!["keys"],
                vec![
                    ("expected", text("at least one key column")),
                    ("actual", text("empty list")),
                ],
            ),
        ),
        (
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID"), Tree::text("ID")],
            (
                "duplicate_identifier",
                Some("OUTPUT-2"),
                vec!["keys[1]"],
                vec![("identifier", text("ID"))],
            ),
        ),
        (
            vec![column("ID", Some(source())), column("ID", Some(source()))],
            vec![Tree::text("ID")],
            (
                "duplicate_identifier",
                Some("SPEC-2"),
                vec!["columns[1].name"],
                vec![("identifier", text("ID"))],
            ),
        ),
    ] {
        log_codes.extend(check(
            base(
                "TEST",
                Tree::map(vec![(
                    "SRC",
                    Tree::map(vec![("path", Tree::text("source.csv"))]),
                )]),
                columns,
                keys,
            ),
            vec![expected],
        ));
    }
    log_codes.extend(check(
        base(
            "TEST",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![
                    ("path", Tree::text("source.parquet")),
                    ("ordinal", Tree::text("REC")),
                ]),
            )]),
            vec![column(
                "ID",
                Some(Tree::map(vec![(
                    "value",
                    Tree::map(vec![(
                        "odm",
                        Tree::map(vec![("item", Tree::text("SRC.ITEM"))]),
                    )]),
                )])),
            )],
            vec![Tree::text("ID")],
        ),
        vec![(
            "odm_schema_field_type",
            Some("INPUT-4"),
            vec!["input.SRC.ordinal"],
            vec![("dataset", text("SRC")), ("declared", text("ordinal"))],
        )],
    ));
    for (path, policy, condition, rule, spec_path, fields) in [
        (
            "input.txt",
            "missing",
            "source_profile_unknown",
            "INPUT-1",
            "input.SRC.path",
            vec![("dataset", text("SRC")), ("path", text("input.txt"))],
        ),
        (
            "input.CSV",
            "present",
            "empty_string_present_unsupported",
            "INPUT-9",
            "input.SRC.empty_string",
            vec![("dataset", text("SRC"))],
        ),
    ] {
        log_codes.extend(check(
            base(
                "TEST",
                Tree::map(vec![(
                    "SRC",
                    Tree::map(vec![
                        ("path", Tree::text(path)),
                        ("empty_string", Tree::text(policy)),
                    ]),
                )]),
                vec![column("ID", Some(source()))],
                vec![Tree::text("ID")],
            ),
            vec![(condition, Some(rule), vec![spec_path], fields)],
        ));
    }
    for (path, encoding) in [
        ("source.xpt", None),
        ("source.sas7bdat", None),
        ("source.csv", Some("windows-1252")),
        ("source.parquet", Some("utf-8")),
    ] {
        let mut input = vec![("path", Tree::text(path))];
        if let Some(encoding) = encoding {
            input.push(("encoding", Tree::text(encoding)));
        }
        log_codes.extend(check(
            base(
                "TEST",
                Tree::map(vec![("SRC", Tree::map(input))]),
                vec![column("ID", Some(source()))],
                vec![Tree::text("ID")],
            ),
            vec![(
                "source_encoding_required",
                Some("SAS-2"),
                vec!["input.SRC.encoding"],
                vec![("dataset", text("SRC"))],
            )],
        ));
    }
    for (report, condition, rule, paths, context) in [
        (
            "result.csv",
            "artifact_path_collision",
            "OUTPUT-9",
            vec!["output.path", "output.report"],
            vec![("path", text("result.csv"))],
        ),
        (
            "report.arrow",
            "unknown_artifact_profile",
            "OUTPUT-5",
            vec!["output.report"],
            vec![
                ("path", text("report.arrow")),
                ("permitted", texts(&[".csv", ".parquet"])),
            ],
        ),
    ] {
        let mut fields = base(
            "TEST",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("input.csv"))]),
            )]),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        let output = vec![
            ("path", Tree::text("result.csv")),
            ("columns", Tree::List(vec![Tree::text("ID")])),
            ("report", Tree::text(report)),
        ];
        fields
            .iter_mut()
            .find(|(name, _)| *name == "output")
            .unwrap()
            .1 = Tree::map(output);
        log_codes.extend(check(fields, vec![(condition, Some(rule), paths, context)]));
    }
    let mut fields = base(
        "TEST",
        Tree::map(vec![(
            "SRC",
            Tree::map(vec![("path", Tree::text("input.csv"))]),
        )]),
        vec![column("ID", Some(source()))],
        vec![Tree::text("ID")],
    );
    fields.push((
        "verifications",
        Tree::List(vec![Tree::map(vec![(
            "row_count",
            Tree::map(vec![
                ("min", Tree::integer("1")),
                ("severity", Tree::text("warning")),
            ]),
        )])]),
    ));
    log_codes.extend(check(
        fields,
        vec![(
            "missing_report",
            Some("CHECK-22"),
            vec!["output.report"],
            vec![("warnings", texts(&["verifications[0].row_count"]))],
        )],
    ));
    let mut fields = base(
        "SRC",
        inputs(),
        vec![column("ID", None)],
        vec![Tree::text("MISSING")],
    );
    fields.push(("base", Tree::text("ABSENT")));
    let mut reached = check(
        fields,
        vec![
            (
                "missing_derivation",
                Some("SPEC-4"),
                vec!["columns.ID.derivation"],
                vec![("column", text("ID"))],
            ),
            (
                "duplicate_identifier",
                Some("NAME-2"),
                vec!["input.SRC", "domain"],
                vec![("identifier", text("SRC"))],
            ),
            (
                "undeclared_column",
                Some("OUTPUT-2"),
                vec!["keys[0]"],
                vec![("column", text("MISSING"))],
            ),
            (
                "driver_unavailable",
                Some("ROW-8"),
                vec!["base"],
                vec![("dataset", text("ABSENT"))],
            ),
        ],
    );
    reached.extend(log_codes);
    reached.extend(check(
        base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        ),
        vec![(
            "driver_unavailable",
            Some("ROW-8"),
            vec!["base"],
            vec![("row", V::Null)],
        )],
    ));
    let aggregate = Tree::map(vec![(
        "value",
        Tree::map(vec![(
            "aggregate",
            Tree::map(vec![("expr", Tree::text("SUM(SRC.ID)"))]),
        )]),
    )]);
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", None), column("X", Some(aggregate))],
        vec![Tree::text("ID")],
    );
    fields.push(("filter", Tree::text("TRUE")));
    fields.push((
        "rows",
        Tree::List(vec![
            Tree::map(vec![
                ("id", Tree::text("first")),
                ("dataset", Tree::text("ABSENT")),
                ("group_by", Tree::List(vec![])),
                (
                    "derivations",
                    Tree::map(vec![("BOGUS", source()), ("X", source())]),
                ),
            ]),
            Tree::map(vec![
                ("id", Tree::text("second")),
                ("dataset", Tree::text("SRC")),
                ("group_by", Tree::List(vec![Tree::text("SECOND.ID")])),
                (
                    "derivations",
                    Tree::map(vec![("ID", source()), ("X", source())]),
                ),
            ]),
            Tree::map(vec![
                ("id", Tree::text("third")),
                (
                    "derivations",
                    Tree::map(vec![("ID", source()), ("X", source())]),
                ),
            ]),
        ]),
    ));
    reached.extend(check(
        fields,
        vec![
            (
                "undeclared_column",
                Some("SPEC-5"),
                vec!["rows.first.derivations.BOGUS"],
                vec![("column", text("BOGUS"))],
            ),
            (
                "missing_derivation",
                Some("SPEC-5"),
                vec!["columns.ID.derivation"],
                vec![("column", text("ID")), ("rows", texts(&["first"]))],
            ),
            (
                "duplicate_derivation",
                Some("SPEC-6"),
                vec!["columns.X.derivation"],
                vec![
                    ("column", text("X")),
                    ("rows", texts(&["first", "second", "third"])),
                    ("not_row_local", text("aggregate")),
                ],
            ),
            (
                "conflicting_row_construction",
                Some("ROW-9"),
                vec!["filter", "rows"],
                vec![],
            ),
            (
                "driver_unavailable",
                Some("ROW-1"),
                vec!["rows.first.dataset"],
                vec![("row", text("first")), ("dataset", text("ABSENT"))],
            ),
            (
                "invalid_field_type",
                Some("ROW-4"),
                vec!["rows.first.group_by"],
                vec![("row", text("first")), ("group_by", texts(&[]))],
            ),
            (
                "unknown_field",
                Some("ROW-4"),
                vec!["rows.second.group_by"],
                vec![
                    ("row", text("second")),
                    ("identifier", text("SECOND.ID")),
                    ("dataset", text("SRC")),
                ],
            ),
            (
                "driver_unavailable",
                Some("ROW-1"),
                vec!["rows.third.dataset"],
                vec![("row", text("third")), ("dataset", V::Null)],
            ),
        ],
    ));
    // Duplicate groups retain authored order instead of sorting or deduplicating.
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", Some(source()))],
        vec![Tree::text("ID")],
    );
    fields.push((
        "rows",
        Tree::List(vec![Tree::map(vec![
            ("id", Tree::text("duplicates")),
            ("dataset", Tree::text("SRC")),
            (
                "group_by",
                Tree::List(vec![Tree::text("SRC.ID"), Tree::text("SRC.ID")]),
            ),
        ])]),
    ));
    reached.extend(check(
        fields,
        vec![(
            "invalid_field_type",
            Some("ROW-4"),
            vec!["rows.duplicates.group_by"],
            vec![
                ("row", text("duplicates")),
                ("group_by", texts(&["SRC.ID", "SRC.ID"])),
            ],
        )],
    ));
    for kind in ["str", "int", "float", "date", "datetime"] {
        let input = Tree::map(vec![(
            "SRC",
            Tree::map(vec![
                ("path", Tree::text("source.PARQUET")),
                ("types", Tree::map(vec![("ID", Tree::text(kind))])),
            ]),
        )]);
        reached.extend(check(
            base(
                "TEST",
                input,
                vec![column("ID", Some(source()))],
                vec![Tree::text("ID")],
            ),
            vec![(
                "redundant_field_type",
                Some("INPUT-6"),
                vec!["input.SRC.types.ID"],
                vec![
                    ("dataset", text("SRC")),
                    ("field", text("ID")),
                    ("type", text(kind)),
                ],
            )],
        ));
    }
    reached
}

/// Requirement, condition, path and context of one temporal declaration finding.
type TemporalFinding<'a> = (&'a str, &'a str, &'a str, Vec<(&'a str, V)>);

pub(crate) fn temporal_reached() -> BTreeSet<ConditionCode> {
    let derivation = |operation: &str, fields: Vec<(&str, Tree)>| {
        Some(Tree::map(vec![(
            "value",
            Tree::map(vec![(operation, Tree::map(fields))]),
        )]))
    };
    let mut fields = base(
        "TEST",
        inputs(),
        vec![
            column("ID", Some(source())),
            column(
                "DIFF",
                derivation(
                    "diff",
                    vec![
                        ("type", Tree::text("date")),
                        ("start", Tree::text("ID")),
                        ("end", Tree::text("ID")),
                        ("unit", Tree::text("month")),
                        ("closed", Tree::text("both")),
                    ],
                ),
            ),
            column(
                "UNIT",
                derivation(
                    "diff",
                    vec![
                        ("type", Tree::text("date")),
                        ("start", Tree::text("ID")),
                        ("end", Tree::text("ID")),
                        ("unit", Tree::text("hour")),
                    ],
                ),
            ),
            column(
                "SHIFT",
                derivation(
                    "shift",
                    vec![
                        ("type", Tree::text("date")),
                        ("source", Tree::text("ID")),
                        ("amount", Tree::text("ID")),
                        ("unit", Tree::text("day")),
                    ],
                ),
            ),
            column(
                "IMPUTED",
                derivation(
                    "impute",
                    vec![
                        ("type", Tree::text("date")),
                        ("source", Tree::text("ID")),
                        ("day", Tree::text("first")),
                        ("time", Tree::text("first")),
                    ],
                ),
            ),
            column(
                "GAP",
                derivation(
                    "dtc",
                    vec![("year", Tree::text("ID")), ("day", Tree::text("ID"))],
                ),
            ),
        ],
        vec![Tree::text("ID")],
    );
    fields.push(("base", Tree::text("SRC")));
    let Err(PrepareError::Invalid(findings)) = prepare(&Tree::map(fields).document()) else {
        panic!("temporal preflight rejection")
    };
    let findings = findings
        .iter()
        .map(|finding| finding.diagnostic())
        .collect::<Vec<_>>();
    let expected: Vec<TemporalFinding<'_>> = vec![
        // Endpoint inclusion is spelled `closed`, with the values the
        // rename table gives (P-ORD-2, D58, D138).
        (
            "TIME-5",
            "value_not_permitted",
            "columns.DIFF.derivation.diff.closed",
            vec![("value", text("both")), ("permitted", texts(&["right"]))],
        ),
        (
            "TIME-1",
            "incompatible_input_type",
            "columns.DIFF.derivation.diff.start",
            vec![
                ("source", text("ID")),
                ("expected", text("date")),
                ("actual", text("int")),
            ],
        ),
        (
            "TIME-1",
            "incompatible_input_type",
            "columns.DIFF.derivation.diff.end",
            vec![
                ("source", text("ID")),
                ("expected", text("date")),
                ("actual", text("int")),
            ],
        ),
        (
            "TIME-4",
            "value_not_permitted",
            "columns.UNIT.derivation.diff.unit",
            vec![
                ("value", text("hour")),
                ("permitted", texts(&["day", "week", "month", "year"])),
            ],
        ),
        (
            "TIME-1",
            "missing_required_field",
            "columns.SHIFT.derivation.shift.days",
            vec![("field", text("days")), ("type", text("date"))],
        ),
        (
            "TIME-1",
            "unknown_field",
            "columns.SHIFT.derivation.shift.amount",
            vec![("field", text("amount")), ("type", text("date"))],
        ),
        (
            "TIME-1",
            "unknown_field",
            "columns.SHIFT.derivation.shift.unit",
            vec![("field", text("unit")), ("type", text("date"))],
        ),
        // Temporal operations are read before the imputation operations.
        (
            "TIME-19",
            "missing_required_field",
            "columns.GAP.derivation.dtc.month",
            vec![("field", text("month")), ("finer", text("day"))],
        ),
        (
            "TIME-1",
            "unknown_field",
            "columns.IMPUTED.derivation.impute.time",
            vec![("field", text("time")), ("type", text("date"))],
        ),
    ];
    assert_eq!(findings.len(), expected.len(), "{findings:?}");
    let mut reached = BTreeSet::new();
    for (diagnostic, (rule, condition, path, context)) in findings.iter().zip(expected) {
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", condition, Some(rule))
        );
        assert_eq!(diagnostic.spec_paths, [path]);
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        reached.insert(rendered(&diagnostic));
    }
    reached
}

/// Reach SELF admission from normalized original documents, with independent truth.
pub(crate) fn self_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for collision in [false, true] {
        let mut declared_inputs = vec![("SRC", Tree::map(vec![("path", Tree::text("src.csv"))]))];
        if collision {
            declared_inputs.push(("SELF", Tree::map(vec![("path", Tree::text("self.csv"))])));
        }
        let mut fields = base(
            "TEST",
            Tree::map(declared_inputs),
            vec![column("ID", None)],
            vec![Tree::text("ID")],
        );
        fields.push((
            "intermediates",
            Tree::List(vec![Tree::map(vec![
                ("id", Tree::text("P")),
                ("dataset", Tree::text("SELF")),
                ("key", Tree::List(vec![Tree::text("ID")])),
            ])]),
        ));
        fields.push((
            "rows",
            Tree::List(vec![Tree::map(vec![
                ("id", Tree::text("first")),
                ("dataset", Tree::text("SRC")),
                (
                    "derivations",
                    Tree::map(vec![(
                        "ID",
                        Tree::map(vec![(
                            "value",
                            Tree::text(if collision { "SRC.ID" } else { "P.ID" }),
                        )]),
                    )]),
                ),
            ])]),
        ));
        let expected = if collision {
            vec![(
                "duplicate_identifier",
                Some("LOOKUP-3"),
                vec!["intermediates.P.dataset", "input.SELF"],
                vec![("identifier", text("SELF"))],
            )]
        } else {
            vec![(
                "phase_boundary",
                Some("LOOKUP-7"),
                vec!["rows.first.derivations.ID.source"],
                vec![
                    ("identifier", text("P.ID")),
                    ("available_phase", text("completed_prior_row_template")),
                    ("required_phase", text("row_construction")),
                ],
            )]
        };
        reached.extend(check(fields, expected));
    }
    reached
}

/// Named lookup declaration causes are reached without opening any declared input.
pub(crate) fn intermediate_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for empty in [Tree::map(Vec::new()), Tree::List(Vec::new())] {
        let mut fields = base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        fields.push(("base", Tree::text("SRC")));
        fields.push((
            "intermediates",
            Tree::List(vec![Tree::map(vec![
                ("id", Tree::text("P")),
                ("dataset", Tree::text("SECOND")),
                ("key", empty),
            ])]),
        ));
        reached.extend(check(
            fields,
            vec![(
                "invalid_field_type",
                Some("LOOKUP-5"),
                vec!["intermediates.P.key"],
                vec![
                    ("intermediate", text("P")),
                    ("dataset", text("SECOND")),
                    ("expected", text("at least one match pair")),
                ],
            )],
        ));
    }
    // Every entry that repeats an earlier column is reported, at that entry.
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", Some(source()))],
        vec![Tree::text("ID")],
    );
    fields.push(("base", Tree::text("SRC")));
    fields.push((
        "intermediates",
        Tree::List(vec![Tree::map(vec![
            ("id", Tree::text("P")),
            ("dataset", Tree::text("SECOND")),
            (
                "key",
                Tree::List(vec![
                    Tree::text("ID"),
                    Tree::text("ID"),
                    Tree::text("OTHER"),
                    Tree::text("ID"),
                ]),
            ),
        ])]),
    ));
    let repeated = |path| {
        (
            "invalid_field_type",
            Some("LOOKUP-5"),
            vec![path],
            vec![
                ("intermediate", text("P")),
                ("dataset", text("SECOND")),
                ("identifier", text("ID")),
                ("expected", text("each column once")),
            ],
        )
    };
    reached.extend(check(
        fields,
        vec![
            repeated("intermediates.P.key[1]"),
            repeated("intermediates.P.key[3]"),
        ],
    ));
    // `P` declares the key its input declares, as 0.5 inferred it, so
    // without a selection it only renames the implicit join (LOOKUP-4; D23).
    let keyed = || {
        Tree::map(vec![
            ("SRC", Tree::map(vec![("path", Tree::text("source.csv"))])),
            (
                "SECOND",
                Tree::map(vec![
                    ("path", Tree::text("second.csv")),
                    ("key", Tree::List(vec![Tree::text("ID")])),
                ]),
            ),
        ])
    };
    for variant in 0..6 {
        let mut fields = base(
            "TEST",
            keyed(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        fields.push(("base", Tree::text("SRC")));
        let mut declaration = vec![
            ("id", Tree::text("P")),
            ("dataset", Tree::text("SECOND")),
            ("key", Tree::List(vec![Tree::text("ID")])),
        ];
        let null = || Tree::Scalar(N::Null);
        match variant {
            0 => declaration.push(("absent", null())),
            1 => declaration.push((
                "order_by",
                Tree::List(vec![Tree::map(vec![
                    ("variable", Tree::text("SECOND.ID")),
                    ("direction", Tree::text("asc")),
                    ("nulls", Tree::text("last")),
                ])]),
            )),
            2 => declaration.push(("keep", Tree::text("last"))),
            // Omitted, the absence answer is missing, as the implicit join's is.
            3 => (),
            4 => {
                declaration.push(("absent", null()));
                declaration.push(("require_match", Tree::boolean(true)));
            }
            // An order with no term gives `keep` nothing to choose by.
            _ => {
                declaration.push(("order_by", Tree::List(Vec::new())));
                declaration.push(("keep", Tree::text("last")));
            }
        }
        fields.push(("intermediates", Tree::List(vec![Tree::map(declaration)])));
        let expected = if variant == 0 || variant == 3 {
            vec![(
                "rename_only_intermediate",
                Some("LOOKUP-4"),
                vec!["intermediates.P"],
                vec![("intermediate", text("P")), ("dataset", text("SECOND"))],
            )]
        } else if variant == 4 {
            vec![(
                "conflicting_absence_policy",
                Some("LOOKUP-24"),
                vec!["intermediates.P.absent", "intermediates.P.require_match"],
                vec![("intermediate", text("P"))],
            )]
        } else {
            let (declared, missing) = if variant == 1 {
                ("order_by", "keep")
            } else {
                ("keep", "order_by")
            };
            vec![(
                "unpaired_fields",
                Some("LOOKUP-10"),
                vec!["intermediates.P"],
                vec![
                    ("intermediate", text("P")),
                    ("declared", texts(&[declared])),
                    ("missing", texts(&[missing])),
                ],
            )]
        };
        reached.extend(check(fields, expected));
    }
    // A nearest selection replaces the ordered one (LOOKUP-26): an order or
    // a position written beside it is refused before any input is read,
    // naming each of them and then `nearest`, and neither is also unpaired.
    for (beside, paths) in [
        (
            vec![("keep", Tree::text("first"))],
            vec!["intermediates.P.keep", "intermediates.P.nearest"],
        ),
        (
            vec![
                ("order_by", Tree::List(vec![Tree::text("SECOND.ID")])),
                ("keep", Tree::text("first")),
            ],
            vec![
                "intermediates.P.order_by",
                "intermediates.P.keep",
                "intermediates.P.nearest",
            ],
        ),
    ] {
        let mut fields = base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        fields.push(("base", Tree::text("SRC")));
        let mut declaration = vec![
            ("id", Tree::text("P")),
            ("dataset", Tree::text("SECOND")),
            ("key", Tree::List(vec![Tree::text("ID")])),
        ];
        declaration.extend(beside);
        declaration.push((
            "nearest",
            Tree::map(vec![
                ("source", Tree::text("DAY")),
                ("to", Tree::text("ID")),
                ("unit", Tree::text("day")),
            ]),
        ));
        fields.push(("intermediates", Tree::List(vec![Tree::map(declaration)])));
        reached.extend(check(
            fields,
            vec![(
                "prohibited_construct",
                Some("LOOKUP-26"),
                paths,
                vec![("intermediate", text("P"))],
            )],
        ));
    }
    reached
}

pub(crate) fn namespace_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for duplicate_row in [false, true] {
        let mut fields = base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Tree::text("ID")],
        );
        fields.push(("base", Tree::text("SRC")));
        let expected = if duplicate_row {
            fields.push((
                "rows",
                Tree::List(
                    (0..2)
                        .map(|_| {
                            Tree::map(vec![
                                ("id", Tree::text("repeated")),
                                ("dataset", Tree::text("SRC")),
                            ])
                        })
                        .collect(),
                ),
            ));
            // Row templates are addressed by identity (P-CLR-2), so the
            // repeat and the original share their path; a 1.0 layer refuses
            // the repeat while it is admitted (LAYER-4).
            vec![(
                "duplicate_identifier",
                Some("ROW-1"),
                vec!["rows.repeated.id", "rows.repeated.id"],
                vec![("identifier", text("repeated"))],
            )]
        } else {
            fields.push((
                "intermediates",
                Tree::List(vec![Tree::map(vec![
                    ("id", Tree::text("LOOK")),
                    ("dataset", Tree::text("ABSENT")),
                    ("key", Tree::List(vec![Tree::text("ID")])),
                    ("require_match", Tree::boolean(true)),
                ])]),
            ));
            vec![(
                "unknown_field",
                Some("NAME-1"),
                vec!["intermediates.LOOK.dataset"],
                vec![
                    ("identifier", text("ABSENT")),
                    ("intermediate", text("LOOK")),
                ],
            )]
        };
        reached.extend(check(fields, expected));
    }
    reached
}
