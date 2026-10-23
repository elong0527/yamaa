//! Actual binder, scope and dependency failures reach the canonical cause registry.
#![allow(unused_imports)]
use super::rendered;
use crate::support::specification::{header, prepare};
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    column_dependencies,
    diagnostic::{ConditionCode, Context, Diagnostic},
    reference_binding::{Catalog, Dataset, Field},
    reference_scope::{self, Phase, Reach, Scope},
    schema::{Document, DocumentNode as N},
    specification::{
        BindError, BindFinding, PrepareError, PreparedSpecification, SourceDeclaration,
        SourceProfile,
    },
    table::{Column, TableSchema},
    value::ColumnType,
};

pub(crate) use super::text;
#[track_caller]
pub(crate) fn check(
    d: Diagnostic,
    condition: &str,
    rule: Option<&str>,
    paths: &[&str],
    values: Vec<(&str, V)>,
) -> ConditionCode {
    super::expect(&d, "validation", condition, rule, paths, values)
}
pub(crate) fn source() -> SourceDeclaration {
    SourceDeclaration {
        name: "SRC".into(),
        path: "source.csv".into(),
        types: vec![],
        profile: SourceProfile::Csv,
        encoding: None,
        byte_order_mark: false,
        empty_string_present: false,
        ordinal: None,
    }
}

pub(crate) use crate::support::document::Tree as T;

pub(crate) fn op<'a>(name: &'a str, field: &'a str, value: &'a str) -> T {
    T::map(vec![(
        "value",
        T::map(vec![(name, T::map(vec![(field, T::text(value))]))]),
    )])
}
/// A derivation written as one expression (D18): a name alone reads it.
pub(crate) fn expression(text: &str) -> T {
    T::map(vec![("value", T::text(text))])
}
pub(crate) fn column(name: &str, derivation: Option<T>) -> T {
    let mut fields = vec![("name", T::text(name)), ("type", T::text("int"))];
    if let Some(d) = derivation {
        fields.push(("derivation", d));
    }
    T::map(fields)
}
pub(crate) fn compile(value: T, rows: Option<T>) -> PreparedSpecification {
    prepare(&tree(value, rows).document()).unwrap()
}
/// The specification [`compile`] prepares.
fn tree(value: T, rows: Option<T>) -> T {
    let read = expression("SRC.ID");
    let mut fields = header();
    fields.extend([
        ("domain", T::text("TEST")),
        ("base", T::text("SRC")),
        (
            "input",
            T::map(vec![("SRC", T::map(vec![("path", T::text("source.csv"))]))]),
        ),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "columns",
            T::List(vec![
                column("ID", rows.is_none().then_some(read)),
                column("VALUE", rows.is_none().then_some(value)),
            ]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID"), T::text("VALUE")])),
            ]),
        ),
    ]);
    if let Some(rows) = rows {
        fields.push(("rows", rows));
    }
    T::map(fields)
}
pub(crate) fn failures(prepared: &PreparedSpecification) -> Vec<BindFinding> {
    let schema = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "V".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&schema]) else {
        panic!("binding must reject this authored document")
    };
    findings
}

pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (declared, missing, value) in [
        ("keep", "order_by", T::text("first")),
        ("order_by", "keep", T::List(vec![T::text("SRC.V")])),
    ] {
        let prepared = compile(
            T::map(vec![(
                "value",
                T::map(vec![(
                    "source",
                    T::map(vec![("variable", T::text("SRC.V")), (declared, value)]),
                )]),
            )]),
            None,
        );
        let findings = failures(&prepared);
        assert_eq!(findings.len(), 1);
        reached.insert(check(
            findings[0]
                .diagnostics(prepared.source())
                .unwrap()
                .remove(0),
            "unpaired_fields",
            Some("LOOKUP-10"),
            &["columns.VALUE.derivation.source"],
            vec![
                ("operation", text("source")),
                ("declared", V::Array(vec![text(declared)])),
                ("missing", V::Array(vec![text(missing)])),
            ],
        ));
    }
    let source = source();
    let stored = [
        Field {
            name: "ID",
            column_type: ColumnType::Int,
        },
        Field {
            name: "V",
            column_type: ColumnType::Int,
        },
    ];
    let catalog = Catalog::compile(
        &[],
        &[Dataset {
            name: "SRC",
            fields: &stored,
        }],
    )
    .unwrap();
    let group = ["SRC.ID"];
    let groups = [("SRC", &group[..])];
    for (phase, condition, rule, row) in [
        (
            Phase::Row {
                group_by: Some(&group),
            },
            "ungrouped_driver_field",
            "ROW-5",
            Some("first"),
        ),
        (
            Phase::Column { groups: &groups },
            "ungrouped_driver_field",
            "NAME-4",
            None,
        ),
    ] {
        let found = reference_scope::validate(
            &catalog,
            "SRC.V",
            None,
            Scope {
                drivers: &["SRC"],
                current_driver: true,
                reach: Reach::Scalar,
                joined: false,
                phase,
            },
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        let finding = BindFinding::QualifiedReference {
            path: "columns.VALUE.derivation.source.variable".into(),
            name: "SRC.V".into(),
            row: row.map(String::from),
            finding: found[0],
        };
        let d = finding.diagnostics(&source).unwrap().pop().unwrap();
        let mut fields = vec![("identifier", text("SRC.V")), ("dataset", text("SRC"))];
        if row.is_some() {
            fields.push(("row", text("first")));
        }
        reached.insert(check(
            d,
            condition,
            Some(rule),
            &["columns.VALUE.derivation.source.variable"],
            fields,
        ));
    }
    // A name alone keeps its source binding (EXPR-5); a formula reports at
    // its derivation (D18).
    for (value, path, condition, rule, fields) in [
        (
            "SRC.MISSING",
            "columns.VALUE.derivation.source",
            "unknown_field",
            Some("NAME-3"),
            vec![("identifier", text("SRC.MISSING"))],
        ),
        (
            "MISSING",
            "columns.VALUE.derivation.source",
            "unknown_field",
            Some("NAME-3"),
            vec![("identifier", text("MISSING"))],
        ),
        (
            "V",
            "columns.VALUE.derivation.source",
            "unresolvable_name",
            Some("NAME-3"),
            vec![("identifier", text("V")), ("suggestion", text("SRC.V"))],
        ),
        (
            "SRC.ID + 1",
            "columns.VALUE.derivation",
            "qualified_identifier",
            Some("FORMULA-9"),
            vec![("expr", text("SRC.ID + 1")), ("identifier", text("SRC.ID"))],
        ),
    ] {
        let prepared = compile(expression(value), None);
        let found = failures(&prepared);
        let d = found
            .iter()
            .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
            .find(|d| d.definition().condition == condition && d.definition().rule == rule)
            .unwrap();
        reached.insert(check(d, condition, rule, &[path], fields));
    }
    for (expr, reason) in [
        ("SUM(ID)", "a grouped row aggregate reads its row driver"),
        (
            "SUM(SECOND.ID)",
            "a grouped row aggregate reads 'SRC', not 'SECOND'",
        ),
    ] {
        let rows = T::List(vec![T::map(vec![
            ("id", T::text("first")),
            ("dataset", T::text("SRC")),
            ("group_by", T::List(vec![T::text("SRC.ID")])),
            (
                "derivations",
                T::map(vec![
                    ("ID", expression("SRC.ID")),
                    ("VALUE", op("aggregate", "expr", expr)),
                ]),
            ),
        ])]);
        let prepared = compile(expression("1"), Some(rows));
        let found = failures(&prepared);
        let d = found
            .iter()
            .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
            .find(|d| d.code == ConditionCode::AggregateDriverScope)
            .unwrap();
        reached.insert(check(
            d,
            "invalid_aggregate_context",
            Some("LOOKUP-18"),
            &["rows.first.derivations.VALUE.aggregate"],
            vec![("expr", text(expr)), ("reason", text(reason))],
        ));
    }
    for (grouped, predicate, identifier, available, required) in [
        (
            false,
            "UNKNOWN > 1",
            "UNKNOWN",
            "column_derivation",
            "row_filter",
        ),
        (
            true,
            "SRC.V > 1",
            "SRC.V",
            "row_construction",
            "grouped_row_filter",
        ),
        (
            true,
            "UNKNOWN > 1",
            "UNKNOWN",
            "column_derivation",
            "grouped_row_filter",
        ),
    ] {
        let mut row = vec![
            ("id", T::text("first")),
            ("filter", T::text(predicate)),
            (
                "derivations",
                T::map(vec![
                    ("ID", expression("SRC.ID")),
                    (
                        "VALUE",
                        if grouped {
                            op("aggregate", "expr", "COUNT(SRC.V)")
                        } else {
                            expression("SRC.V")
                        },
                    ),
                ]),
            ),
        ];
        if grouped {
            row.push(("group_by", T::List(vec![T::text("SRC.ID")])));
        }
        let prepared = compile(expression("1"), Some(T::List(vec![T::map(row)])));
        let found = failures(&prepared);
        assert_eq!(found.len(), 1);
        let d = found[0]
            .diagnostics(prepared.source())
            .unwrap()
            .pop()
            .unwrap();
        reached.insert(check(
            d,
            "phase_boundary",
            Some("ROW-6"),
            &["rows.first.filter"],
            vec![
                ("identifier", text(identifier)),
                ("row", text("first")),
                ("available_phase", text(available)),
                ("required_phase", text(required)),
            ],
        ));
    }
    let columns: Vec<String> = vec!["ID".into(), "VALUE".into()];
    let paths: Vec<String> = vec![
        "columns.ID.derivation.compute".into(),
        "columns.VALUE.derivation.compute".into(),
    ];
    for (dependencies, keys) in [
        (vec![Some(vec![1]), Some(vec![0])], vec![]),
        (vec![Some(vec![1]), Some(vec![])], vec![0]),
        (vec![None, Some(vec![])], vec![0]),
    ] {
        let analysis = column_dependencies::analyze(&dependencies, &keys, false).unwrap();
        for f in analysis.diagnostics {
            let (condition, rule, site, fields) = match &f {
                column_dependencies::Diagnostic::Cycle { .. } => (
                    "dependency_cycle",
                    "COLUMN-6",
                    vec![paths[0].as_str(), paths[1].as_str()],
                    vec![(
                        "cycle",
                        V::Array(vec![text("ID"), text("VALUE"), text("ID")]),
                    )],
                ),
                column_dependencies::Diagnostic::ForwardReference { .. } => (
                    "forward_reference",
                    "COLUMN-5",
                    vec![paths[0].as_str()],
                    vec![("column", text("ID")), ("dependency", text("VALUE"))],
                ),
                column_dependencies::Diagnostic::MissingKeyDerivation { .. } => (
                    "key_dependency",
                    "COLUMN-7",
                    vec!["columns.ID.derivation"],
                    vec![("column", text("ID"))],
                ),
                column_dependencies::Diagnostic::KeyDependency { .. } => (
                    "key_dependency",
                    "COLUMN-7",
                    vec!["columns.ID.derivation"],
                    vec![("column", text("ID")), ("dependency", text("VALUE"))],
                ),
            };
            assert_eq!(f.condition(), condition);
            assert_eq!(f.rule(), rule);
            reached.insert(check(
                f.specification_diagnostic(&columns, &paths).unwrap(),
                condition,
                Some(rule),
                &site,
                fields,
            ));
        }
    }
    reached
}

pub(crate) fn window_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let source = source();
    for (operation, payload, condition, rule, suffix, values) in [
        (
            "row_value",
            T::map(vec![
                ("source", T::text("ID")),
                ("offset", T::Scalar(N::Integer("0".into()))),
                (
                    "window",
                    T::map(vec![("order_by", T::List(vec![T::text("ID")]))]),
                ),
            ]),
            "zero_offset",
            "WINDOW-7",
            ".offset",
            vec![("offset", V::from(0))],
        ),
        (
            "rank",
            T::map(vec![]),
            "window_order_by_required",
            "WINDOW-5",
            ".window",
            vec![("operation", text("rank"))],
        ),
        (
            "row_number",
            T::map(vec![(
                "window",
                T::map(vec![
                    ("group_by", T::List(vec![T::text("ID"), T::text("ID")])),
                    ("order_by", T::List(vec![T::text("ID")])),
                ]),
            )]),
            "invalid_field_type",
            "WINDOW-3",
            ".window.group_by[1]",
            vec![
                ("identifier", text("ID")),
                ("expected", text("each variable once")),
            ],
        ),
    ] {
        let value = T::map(vec![("value", T::map(vec![(operation, payload)]))]);
        let prepared = compile(value, None);
        let found = failures(&prepared);
        assert_eq!(found.len(), 1);
        let BindFinding::Window(window) = &found[0] else {
            panic!("window binder must produce the finding")
        };
        assert_eq!(window.definition(), window.diagnostic().definition());
        let mut projected = found[0].diagnostics(&source).unwrap();
        assert_eq!(projected.len(), 1);
        let path = format!("columns.VALUE.derivation.{operation}{suffix}");
        reached.insert(check(
            projected.remove(0),
            condition,
            Some(rule),
            &[&path],
            values,
        ));
    }
    let prepared = compile(
        T::map(vec![(
            "value",
            T::map(vec![(
                "baseline_flag",
                T::map(vec![
                    ("date", T::text("ID")),
                    ("reference", T::text("ID")),
                    ("source", T::text("ID")),
                ]),
            )]),
        )]),
        None,
    );
    let found = failures(&prepared);
    assert_eq!(found.len(), 1);
    let mut projected = found[0].diagnostics(&source).unwrap();
    reached.insert(check(
        projected.remove(0),
        "incompatible_input_type",
        Some("VALUE-2"),
        &["columns.VALUE.derivation.baseline_flag.date"],
        vec![
            ("operation", text("baseline_flag")),
            ("source", text("ID")),
            ("expected", text("date or datetime")),
            ("actual", text("int")),
        ],
    ));
    reached
}

/// A column-level window spans the rows of every row template, so preparation
/// refuses its read of a driver that not every template has (WINDOW-11), and
/// the same window over templates sharing that driver prepares.
pub(crate) fn window_driver_reached() -> BTreeSet<ConditionCode> {
    use yamaa_core::specification::{PreflightFinding, PrepareError};
    let prepare = |second: &str| {
        let template = |id: &str, driver: &str| {
            T::map(vec![
                ("id", T::text(id)),
                ("dataset", T::text(driver)),
                (
                    "derivations",
                    T::map(vec![("ID", expression(&format!("{driver}.ID")))]),
                ),
            ])
        };
        let window = T::map(vec![(
            "value",
            T::map(vec![(
                "row_number",
                T::map(vec![(
                    "window",
                    T::map(vec![("order_by", T::List(vec![T::text("DM.AGE")]))]),
                )]),
            )]),
        )]);
        let mut fields = header();
        fields.extend([
            ("domain", T::text("TEST")),
            (
                "input",
                T::map(vec![
                    ("DM", T::map(vec![("path", T::text("dm.csv"))])),
                    ("AE", T::map(vec![("path", T::text("ae.csv"))])),
                ]),
            ),
            ("keys", T::List(vec![T::text("ID")])),
            (
                "columns",
                T::List(vec![column("ID", None), column("VALUE", Some(window))]),
            ),
            (
                "output",
                T::map(vec![
                    ("path", T::text("result.csv")),
                    ("columns", T::List(vec![T::text("ID"), T::text("VALUE")])),
                ]),
            ),
            (
                "rows",
                T::List(vec![template("subjects", "DM"), template("events", second)]),
            ),
        ]);
        prepare(&T::map(fields).document())
    };
    assert!(prepare("DM").is_ok(), "a shared driver prepares");
    let Err(PrepareError::Invalid(mut findings)) = prepare("AE") else {
        panic!("a driver the events template lacks is refused before binding")
    };
    assert_eq!(findings.len(), 1);
    let PreflightFinding::Scalar(diagnostic) = findings.remove(0) else {
        panic!("one scalar preparation finding")
    };
    BTreeSet::from([check(
        diagnostic,
        "unknown_field",
        Some("WINDOW-11"),
        &["columns.VALUE.derivation.row_number.window.order_by[0]"],
        vec![
            ("identifier", text("DM.AGE")),
            ("drivers", V::Array(vec![text("DM"), text("AE")])),
            ("operation", text("row_number")),
        ],
    )])
}

pub(crate) fn lookup_compile(
    value: &str,
    donor: &str,
    target: &str,
    filter: Option<&str>,
) -> PreparedSpecification {
    let mut selection = vec![
        ("id", T::text("SEL")),
        ("dataset", T::text("DONOR")),
        ("key", T::map(vec![(donor, T::text(target))])),
    ];
    if let Some(filter) = filter {
        selection.push(("filter", T::text(filter)));
    }
    let mut fields = header();
    fields.extend([
        ("domain", T::text("TEST")),
        ("base", T::text("SRC")),
        (
            "input",
            T::map(vec![
                ("SRC", T::map(vec![("path", T::text("source.csv"))])),
                ("DONOR", T::map(vec![("path", T::text("donor.csv"))])),
            ]),
        ),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "columns",
            T::List(vec![
                column("ID", Some(expression("SRC.ID"))),
                column("VALUE", Some(expression(value))),
            ]),
        ),
        ("intermediates", T::List(vec![T::map(selection)])),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID"), T::text("VALUE")])),
            ]),
        ),
    ]);
    prepare(&T::map(fields).document()).unwrap()
}

pub(crate) fn lookup_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let source = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    for (value, donor, target, filter, kind, rule, path, fields) in [
        (
            "SEL.V",
            "DID",
            "ID",
            Some("V > 0"),
            ColumnType::Int,
            "LOOKUP-7",
            "intermediates.SEL.filter",
            vec![
                ("identifier", text("V")),
                ("intermediate", text("SEL")),
                ("suggestion", text("DONOR.V")),
            ],
        ),
        (
            "SEL.V",
            "BAD",
            "ID",
            None,
            ColumnType::Int,
            "LOOKUP-5",
            "intermediates.SEL.key",
            vec![
                ("identifier", text("DONOR.BAD")),
                ("intermediate", text("SEL")),
            ],
        ),
        (
            "SEL.V",
            "DID",
            "BAD",
            None,
            ColumnType::Int,
            "LOOKUP-5",
            "intermediates.SEL.key",
            vec![("identifier", text("BAD")), ("intermediate", text("SEL"))],
        ),
        (
            "SEL.BAD",
            "DID",
            "ID",
            None,
            ColumnType::Int,
            "LOOKUP-11",
            "columns.VALUE.derivation.source",
            vec![("identifier", text("SEL.BAD"))],
        ),
        (
            "SEL.V",
            "DID",
            "ID",
            None,
            ColumnType::Str,
            "LOOKUP-9",
            "intermediates.SEL.key",
            vec![
                ("intermediate", text("SEL")),
                ("source", text("ID")),
                ("expected", text("int")),
                ("actual", text("str")),
            ],
        ),
    ] {
        let prepared = lookup_compile(value, donor, target, filter);
        let secondary = TableSchema::new(vec![
            Column {
                name: "DID".into(),
                kind,
            },
            Column {
                name: "V".into(),
                kind: ColumnType::Int,
            },
        ])
        .unwrap();
        let Err(BindError::Invalid(found)) = prepared.bind_sources(&[&source, &secondary]) else {
            panic!("named selection must fail")
        };
        let projected = found
            .iter()
            .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
            .find(|d| d.definition().rule == Some(rule))
            .unwrap();
        reached.insert(check(
            projected,
            if rule == "LOOKUP-9" {
                "incompatible_input_type"
            } else {
                "unknown_field"
            },
            Some(rule),
            &[path],
            fields,
        ));
    }
    reached
}

/// A nearest selection's refusals at binding (LOOKUP-26): a source the donor
/// relation does not have, a source that is neither a date nor a datetime,
/// and a reference of another type than its source's, a date beside a
/// datetime.
pub(crate) fn nearest_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let source = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "D".into(),
            kind: ColumnType::Date,
        },
        Column {
            name: "AT".into(),
            kind: ColumnType::DateTime,
        },
    ])
    .unwrap();
    let donor = TableSchema::new(vec![
        Column {
            name: "DID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "V".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "DT".into(),
            kind: ColumnType::DateTime,
        },
        Column {
            name: "TXT".into(),
            kind: ColumnType::Str,
        },
    ])
    .unwrap();
    for (nearest_source, to, condition, path, fields) in [
        (
            "BAD",
            "SRC.AT",
            "unknown_field",
            "intermediates.SEL.nearest.source",
            vec![("identifier", text("BAD")), ("intermediate", text("SEL"))],
        ),
        (
            "TXT",
            "SRC.AT",
            "incompatible_input_type",
            "intermediates.SEL.nearest.source",
            vec![
                ("identifier", text("TXT")),
                ("intermediate", text("SEL")),
                ("actual", text("str")),
                ("permitted", V::Array(vec![text("date"), text("datetime")])),
            ],
        ),
        (
            "DT",
            "SRC.D",
            "incompatible_input_type",
            "intermediates.SEL.nearest.to",
            vec![
                ("intermediate", text("SEL")),
                ("expected", text("datetime")),
                ("actual", text("date")),
            ],
        ),
    ] {
        let mut fields_of = header();
        fields_of.extend([
            ("domain", T::text("TEST")),
            ("base", T::text("SRC")),
            (
                "input",
                T::map(vec![
                    ("SRC", T::map(vec![("path", T::text("source.csv"))])),
                    ("DONOR", T::map(vec![("path", T::text("donor.csv"))])),
                ]),
            ),
            ("keys", T::List(vec![T::text("ID")])),
            (
                "columns",
                T::List(vec![
                    column("ID", Some(expression("SRC.ID"))),
                    column("VALUE", Some(expression("SEL.V"))),
                ]),
            ),
            (
                "intermediates",
                T::List(vec![T::map(vec![
                    ("id", T::text("SEL")),
                    ("dataset", T::text("DONOR")),
                    ("key", T::map(vec![("DID", T::text("ID"))])),
                    (
                        "nearest",
                        T::map(vec![
                            ("source", T::text(nearest_source)),
                            ("to", T::text(to)),
                            ("unit", T::text("hour")),
                        ]),
                    ),
                ])]),
            ),
            (
                "output",
                T::map(vec![
                    ("path", T::text("result.csv")),
                    ("columns", T::List(vec![T::text("ID"), T::text("VALUE")])),
                ]),
            ),
        ]);
        let prepared = prepare(&T::map(fields_of).document()).unwrap();
        let Err(BindError::Invalid(found)) = prepared.bind_sources(&[&source, &donor]) else {
            panic!("the nearest selection must fail: {nearest_source}, {to}")
        };
        let projected = found
            .iter()
            .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
            .find(|d| d.definition().rule == Some("LOOKUP-26"))
            .unwrap();
        reached.insert(check(
            projected,
            condition,
            Some("LOOKUP-26"),
            &[path],
            fields,
        ));
    }
    reached
}

pub(crate) fn source_filter_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (variable, predicate, condition, rule, values) in [
        (
            "SRC.V",
            "ID = 1",
            "unknown_field",
            "LOOKUP-8",
            vec![("identifier", text("ID")), ("dataset", text("SRC"))],
        ),
        (
            "ID",
            "SRC.V =",
            "prohibited_construct",
            "LOOKUP-8",
            vec![("identifier", text("ID")), ("construct", text("filter"))],
        ),
    ] {
        let prepared = compile(
            T::map(vec![(
                "value",
                T::map(vec![(
                    "source",
                    T::map(vec![
                        ("variable", T::text(variable)),
                        ("filter", T::text(predicate)),
                    ]),
                )]),
            )]),
            None,
        );
        let findings = failures(&prepared);
        assert_eq!(findings.len(), 1);
        reached.insert(check(
            findings[0].diagnostics(&source()).unwrap().remove(0),
            condition,
            Some(rule),
            &["columns.VALUE.derivation.source.filter"],
            values,
        ));
    }
    reached
}

/// `require_match` contradicts `absent`, and a source with no implicit join
/// has no match to require (LOOKUP-24).
pub(crate) fn source_match_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let null = || T::Scalar(N::Null);
    for (options, condition, paths, values) in [
        (
            vec![("absent", null()), ("require_match", T::boolean(true))],
            "conflicting_absence_policy",
            vec![
                "columns.VALUE.derivation.source.absent",
                "columns.VALUE.derivation.source.require_match",
            ],
            vec![("variable", text("SRC.V")), ("dataset", text("SRC"))],
        ),
        (
            vec![("require_match", T::boolean(true))],
            "prohibited_construct",
            vec!["columns.VALUE.derivation.source.require_match"],
            vec![
                ("identifier", text("SRC.V")),
                ("construct", text("require_match")),
            ],
        ),
    ] {
        let mut fields = vec![("variable", T::text("SRC.V"))];
        fields.extend(options);
        let prepared = compile(
            T::map(vec![("value", T::map(vec![("source", T::map(fields))]))]),
            None,
        );
        let findings = failures(&prepared);
        assert_eq!(findings.len(), 1);
        reached.insert(check(
            findings[0].diagnostics(&source()).unwrap().remove(0),
            condition,
            Some("LOOKUP-24"),
            &paths,
            values,
        ));
    }
    reached
}

/// A source binding's range states a bound, and a source that reads no
/// implicit join has nothing for a range to narrow (LOOKUP-9).
pub(crate) fn source_range_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (bounds, condition, path, values) in [
        (
            vec![("value", T::text("ID")), ("lower", T::text("V"))],
            "prohibited_construct",
            "columns.VALUE.derivation.source.between",
            vec![
                ("identifier", text("SRC.V")),
                ("construct", text("between")),
            ],
        ),
        (
            vec![("value", T::text("ID"))],
            "invalid_field_type",
            "columns.VALUE.derivation.source.between",
            vec![
                ("expected", text("at least one of lower, upper")),
                ("variable", text("SRC.V")),
            ],
        ),
    ] {
        let prepared = compile(
            T::map(vec![(
                "value",
                T::map(vec![(
                    "source",
                    T::map(vec![
                        ("variable", T::text("SRC.V")),
                        ("between", T::map(bounds)),
                    ]),
                )]),
            )]),
            None,
        );
        let findings = failures(&prepared);
        assert_eq!(findings.len(), 1);
        reached.insert(check(
            findings[0].diagnostics(&source()).unwrap().remove(0),
            condition,
            Some("LOOKUP-9"),
            &[path],
            values,
        ));
    }
    reached
}

pub fn verification_reached() -> BTreeSet<ConditionCode> {
    let assertion = T::map(vec![
        ("when", T::text("FALSE")),
        ("require", T::text("Z > 0 OR A > 0")),
    ]);
    let verifications = T::List(vec![T::map(vec![("assert", assertion)])]);
    let mut fields = header();
    fields.extend([
        ("domain", T::text("TEST")),
        ("base", T::text("SRC")),
        (
            "input",
            T::map(vec![("SRC", T::map(vec![("path", T::text("source.csv"))]))]),
        ),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "columns",
            T::List(vec![column("ID", Some(expression("SRC.ID")))]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID")])),
            ]),
        ),
        ("verifications", verifications),
    ]);
    let prepared = prepare(&T::map(fields).document()).unwrap();
    assert_eq!(prepared.verifications().len(), 2);
    assert!(matches!(
        prepared.verifications()[0].check,
        yamaa_core::dataset::Check::PredicateDeclaration(_)
    ));
    let diagnostic = prepared
        .verifications()
        .iter()
        .find_map(|check| match &check.check {
            yamaa_core::dataset::Check::InvalidDiagnostic(diagnostic) => Some(diagnostic),
            _ => None,
        })
        .expect("unknown assertion field retained until verification");
    [check(
        diagnostic.clone(),
        "unknown_field",
        Some("CHECK-4"),
        &["verifications[0].assert.require"],
        vec![("identifier", text("A"))],
    )]
    .into()
}

pub fn verification_declarations_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (kind, column_checks, dataset_checks, condition, rule, path, reason) in [
        (
            "int",
            T::List(vec![]),
            T::List(vec![T::map(vec![(
                "unique",
                T::map(vec![
                    ("id", T::text("")),
                    ("columns", T::List(vec![T::text("V")])),
                ]),
            )])]),
            "invalid_declaration",
            "CHECK-3",
            "verifications[0].unique",
            "a verification id is text",
        ),
        (
            "int",
            T::List(vec![T::map(vec![(
                "allowed_values",
                T::map(vec![("values", T::List(vec![]))]),
            )])]),
            T::List(vec![]),
            "invalid_declaration",
            "CHECK-4",
            "columns.V.verifications[0].allowed_values",
            "allowed_values requires values",
        ),
        (
            "int",
            T::List(vec![]),
            T::List(vec![
                T::map(vec![(
                    "unique",
                    T::map(vec![
                        ("id", T::text("same")),
                        ("columns", T::List(vec![T::text("V")])),
                    ]),
                )]),
                T::map(vec![(
                    "unique",
                    T::map(vec![
                        ("id", T::text("same")),
                        ("columns", T::List(vec![T::text("V")])),
                    ]),
                )]),
            ]),
            "duplicate_identifier",
            "CHECK-3",
            "verifications[1].unique",
            "verification id 'same' repeats verifications[0].unique",
        ),
        (
            "int",
            T::List(vec![T::map(vec![(
                "range",
                T::map(vec![
                    ("min", T::Scalar(N::Integer("2".into()))),
                    ("max", T::Scalar(N::Integer("1".into()))),
                ]),
            )])]),
            T::List(vec![]),
            "invalid_declaration",
            "CHECK-4",
            "columns.V.verifications[0].range",
            "range min exceeds max",
        ),
        (
            "int",
            T::List(vec![]),
            T::List(vec![T::map(vec![(
                "row_count",
                T::map(vec![
                    ("min", T::Scalar(N::Integer("1".into()))),
                    ("group_by", T::List(vec![T::text("ID"), T::text("ID")])),
                ]),
            )])]),
            "invalid_declaration",
            "CHECK-14",
            "verifications[0].row_count",
            "row_count group_by repeats a column",
        ),
        // A written empty `group_by` is refused; an omitted one groups nothing.
        (
            "int",
            T::List(vec![]),
            T::List(vec![T::map(vec![(
                "row_count",
                T::map(vec![
                    ("min", T::Scalar(N::Integer("1".into()))),
                    ("group_by", T::List(vec![])),
                ]),
            )])]),
            "invalid_declaration",
            "CHECK-14",
            "verifications[0].row_count",
            "group_by names at least one column",
        ),
        (
            "str",
            T::List(vec![T::map(vec![(
                "max_length",
                T::map(vec![("max", T::Scalar(N::Integer("0".into())))]),
            )])]),
            T::List(vec![]),
            "invalid_declaration",
            "CHECK-8",
            "columns.V.verifications[0].max_length",
            "max_length max is at least one",
        ),
        (
            "int",
            T::List(vec![]),
            T::List(vec![T::map(vec![(
                "all_or_none",
                T::map(vec![("columns", T::List(vec![T::text("ID")]))]),
            )])]),
            "invalid_declaration",
            "CHECK-11",
            "verifications[0].all_or_none",
            "all_or_none names two distinct columns",
        ),
        (
            "int",
            T::List(vec![T::map(vec![(
                "max_length",
                T::map(vec![("max", T::Scalar(N::Integer("1".into())))]),
            )])]),
            T::List(vec![]),
            "invalid_declaration",
            "CHECK-4",
            "columns.V.verifications[0].max_length",
            "a int column does not admit this verification",
        ),
        (
            "int",
            T::List(vec![]),
            T::List(vec![T::map(vec![(
                "all_or_none",
                T::map(vec![(
                    "columns",
                    T::List(vec![T::text("ID"), T::text("UNKNOWN")]),
                )]),
            )])]),
            "unknown_field",
            "CHECK-4",
            "verifications[0].all_or_none.columns",
            "unknown column 'UNKNOWN'",
        ),
    ] {
        let mut fields = header();
        fields.extend([
            ("domain", T::text("TEST")),
            ("base", T::text("SRC")),
            (
                "input",
                T::map(vec![("SRC", T::map(vec![("path", T::text("source.csv"))]))]),
            ),
            ("keys", T::List(vec![T::text("ID")])),
            (
                "columns",
                T::List(vec![
                    column("ID", Some(expression("SRC.ID"))),
                    T::map(vec![
                        ("name", T::text("V")),
                        ("type", T::text(kind)),
                        ("derivation", expression("SRC.V")),
                        ("verifications", column_checks),
                    ]),
                ]),
            ),
            (
                "output",
                T::map(vec![
                    ("path", T::text("result.csv")),
                    ("columns", T::List(vec![T::text("ID"), T::text("V")])),
                ]),
            ),
            ("verifications", dataset_checks),
        ]);
        let prepared = prepare(&T::map(fields).document()).unwrap();
        let findings = prepared.verification_declaration_diagnostics();
        assert_eq!(prepared.verification_declaration_diagnostics(), findings);
        assert_eq!(findings.len(), 1);
        // A repeated id and an unknown column also name the identifier.
        let mut values = vec![("reason", text(reason))];
        match condition {
            "duplicate_identifier" => values.push(("identifier", text("same"))),
            "unknown_field" => values.push(("identifier", text("UNKNOWN"))),
            _ => {}
        }
        reached.insert(check(
            findings.into_iter().next().unwrap(),
            condition,
            Some(rule),
            &[path],
            values,
        ));
        assert_eq!(prepared.source().path, "source.csv");
    }
    reached
}

/// Without row templates, a key computed by an aggregate reads every record
/// of a key combination that does not exist yet (COLUMN-7). Binding and the
/// static check report the same finding.
pub(crate) fn key_record_reached() -> BTreeSet<ConditionCode> {
    let mut fields = header();
    fields.extend([
        ("domain", T::text("TEST")),
        ("base", T::text("SRC")),
        (
            "input",
            T::map(vec![("SRC", T::map(vec![("path", T::text("source.csv"))]))]),
        ),
        ("keys", T::List(vec![T::text("ID"), T::text("TOP")])),
        (
            "columns",
            T::List(vec![
                column("ID", Some(expression("SRC.ID"))),
                column("TOP", Some(op("aggregate", "expr", "MAX(SRC.V)"))),
            ]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID"), T::text("TOP")])),
            ]),
        ),
    ]);
    let prepared = prepare(&T::map(fields).document()).unwrap();
    let expected = |d: Diagnostic| {
        check(
            d,
            "key_dependency",
            Some("COLUMN-7"),
            &["columns.TOP.derivation.aggregate"],
            vec![("column", text("TOP")), ("operation", text("aggregate"))],
        )
    };
    let checked = prepared.check_declaration_diagnostics();
    assert_eq!(checked.len(), 1);
    expected(checked[0].clone());
    let found = failures(&prepared);
    assert_eq!(found.len(), 1);
    let mut diagnostics = found[0].diagnostics(prepared.source()).unwrap();
    assert_eq!(diagnostics.len(), 1);
    BTreeSet::from([expected(diagnostics.remove(0))])
}
