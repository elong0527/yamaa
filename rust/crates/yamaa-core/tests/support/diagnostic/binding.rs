//! Actual binder, scope and dependency failures reach the canonical cause registry.
#![allow(unused_imports)]
pub(crate) use std::collections::BTreeSet;
pub(crate) use yamaa_core::json::Value as V;
pub(crate) use yamaa_core::{
    column_dependencies,
    diagnostic::{ConditionCode, Context, Diagnostic},
    reference_binding::{Catalog, Dataset, Field},
    reference_scope::{self, Phase, Reach, Scope},
    schema::{Document, DocumentNode as N, SpecificationDocument},
    specification::{
        BindError, BindFinding, PreparedSpecification, SourceDeclaration, SourceProfile,
    },
    table::{Column, TableSchema},
    value::ColumnType,
};

pub(crate) fn text(v: &str) -> V {
    V::String(v.into())
}
pub(crate) fn check(
    d: Diagnostic,
    condition: &str,
    requirement: Option<&str>,
    paths: &[&str],
    values: Vec<(&str, V)>,
) -> ConditionCode {
    let def = d.definition();
    assert_eq!(
        (def.phase, def.condition, def.requirement),
        ("validation", condition, requirement)
    );
    assert_eq!(d.spec_paths, paths);
    assert_eq!(
        d.context,
        values
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect::<Context>()
    );
    assert_eq!(d.source_span, None);
    assert_eq!(d.operand_route, None);
    d.code
}
pub(crate) fn source() -> SourceDeclaration {
    SourceDeclaration {
        name: "SRC".into(),
        path: "source.csv".into(),
        types: vec![],
        profile: SourceProfile::Csv,
        encoding: None,
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
pub(crate) fn column(name: &str, derivation: Option<T>) -> T {
    let mut fields = vec![("name", T::text(name)), ("type", T::text("int"))];
    if let Some(d) = derivation {
        fields.push(("derivation", d));
    }
    T::map(fields)
}
pub(crate) fn compile(value: T, rows: Option<T>) -> PreparedSpecification {
    let mut fields = vec![
        ("schema_version", T::text("1.0")),
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
                column(
                    "ID",
                    rows.is_none().then_some(op("source", "variable", "SRC.ID")),
                ),
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
    ];
    if let Some(rows) = rows {
        fields.push(("rows", rows));
    }
    let mut nodes = vec![];
    let root = T::map(fields).append(&mut nodes);
    let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap();
    PreparedSpecification::prepare(&model).unwrap()
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
            Some("REQ-0119"),
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
    let groups = [&group[..]];
    for (phase, condition, requirement, row) in [
        (
            Phase::Row {
                group_by: Some(&group),
            },
            "ungrouped_driver_field",
            "REQ-0067",
            Some("first"),
        ),
        (
            Phase::Column { groups: &groups },
            "ungrouped_driver_field",
            "REQ-0107",
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
            Some(requirement),
            &["columns.VALUE.derivation.source.variable"],
            fields,
        ));
    }
    for (operation, field, value, condition, requirement, fields) in [
        (
            "source",
            "variable",
            "SRC.MISSING",
            "unknown_field",
            Some("REQ-0103"),
            vec![("identifier", text("SRC.MISSING"))],
        ),
        (
            "source",
            "variable",
            "MISSING",
            "unknown_field",
            Some("REQ-0106"),
            vec![("identifier", text("MISSING"))],
        ),
        (
            "source",
            "variable",
            "V",
            "unresolvable_name",
            Some("REQ-0106"),
            vec![("identifier", text("V")), ("suggestion", text("SRC.V"))],
        ),
        (
            "compute",
            "expr",
            "SRC.ID + 1",
            "qualified_identifier",
            Some("REQ-0442"),
            vec![("expr", text("SRC.ID + 1")), ("identifier", text("SRC.ID"))],
        ),
    ] {
        let prepared = compile(op(operation, field, value), None);
        let found = failures(&prepared);
        let d = found
            .iter()
            .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
            .find(|d| {
                d.definition().condition == condition && d.definition().requirement == requirement
            })
            .unwrap();
        let path = if operation == "compute" {
            format!("columns.VALUE.derivation.{operation}.{field}")
        } else {
            format!("columns.VALUE.derivation.{operation}")
        };
        reached.insert(check(d, condition, requirement, &[&path], fields));
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
                    ("ID", op("source", "variable", "SRC.ID")),
                    ("VALUE", op("aggregate", "expr", expr)),
                ]),
            ),
        ])]);
        let prepared = compile(op("compute", "expr", "1"), Some(rows));
        let found = failures(&prepared);
        let d = found
            .iter()
            .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
            .find(|d| d.code == ConditionCode::AggregateDriverScope)
            .unwrap();
        reached.insert(check(
            d,
            "invalid_aggregate_context",
            Some("REQ-0329"),
            &["rows[0].derivations.VALUE.aggregate"],
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
                    ("ID", op("source", "variable", "SRC.ID")),
                    (
                        "VALUE",
                        if grouped {
                            op("aggregate", "expr", "COUNT(SRC.V)")
                        } else {
                            op("source", "variable", "SRC.V")
                        },
                    ),
                ]),
            ),
        ];
        if grouped {
            row.push(("group_by", T::List(vec![T::text("SRC.ID")])));
        }
        let prepared = compile(op("compute", "expr", "1"), Some(T::List(vec![T::map(row)])));
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
            Some("REQ-0068"),
            &["rows[0].filter"],
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
            let (condition, requirement, site, fields) = match &f {
                column_dependencies::Diagnostic::Cycle { .. } => (
                    "dependency_cycle",
                    "REQ-0072",
                    vec![paths[0].as_str(), paths[1].as_str()],
                    vec![(
                        "cycle",
                        V::Array(vec![text("ID"), text("VALUE"), text("ID")]),
                    )],
                ),
                column_dependencies::Diagnostic::ForwardReference { .. } => (
                    "forward_reference",
                    "REQ-0071",
                    vec![paths[0].as_str()],
                    vec![("column", text("ID")), ("dependency", text("VALUE"))],
                ),
                column_dependencies::Diagnostic::MissingKeyDerivation { .. } => (
                    "key_dependency",
                    "REQ-0074",
                    vec!["columns.ID.derivation"],
                    vec![("column", text("ID"))],
                ),
                column_dependencies::Diagnostic::KeyDependency { .. } => (
                    "key_dependency",
                    "REQ-0074",
                    vec!["columns.ID.derivation"],
                    vec![("column", text("ID")), ("dependency", text("VALUE"))],
                ),
            };
            assert_eq!(f.condition(), condition);
            assert_eq!(f.requirement(), requirement);
            reached.insert(check(
                f.specification_diagnostic(&columns, &paths).unwrap(),
                condition,
                Some(requirement),
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
    for (operation, payload, condition, requirement, suffix, values) in [
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
            "REQ-0328",
            ".offset",
            vec![("offset", V::from(0))],
        ),
        (
            "rank",
            T::map(vec![]),
            "window_order_by_required",
            "REQ-0340",
            ".window",
            vec![("operation", text("rank"))],
        ),
        (
            "baseline_flag",
            T::map(vec![
                ("date", T::text("ID")),
                ("reference_date", T::text("ID")),
                (
                    "window",
                    T::map(vec![("order_by", T::List(vec![T::text("ID")]))]),
                ),
            ]),
            "window_order_by_forbidden",
            "REQ-0341",
            ".window.order_by",
            vec![("operation", text("baseline_flag"))],
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
            "REQ-0293",
            ".window.group_by[1]",
            vec![("identifier", text("ID"))],
        ),
    ] {
        let prepared = compile(
            T::map(vec![("value", T::map(vec![(operation, payload)]))]),
            None,
        );
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
            Some(requirement),
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
                    ("reference_date", T::text("ID")),
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
        Some("REQ-0323"),
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
    let tree = T::map(vec![
        ("schema_version", T::text("1.0")),
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
                column("ID", Some(op("source", "variable", "SRC.ID"))),
                column("VALUE", Some(op("source", "variable", value))),
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
    let mut nodes = vec![];
    let root = tree.append(&mut nodes);
    let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap();
    PreparedSpecification::prepare(&model).unwrap()
}

pub(crate) fn lookup_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let source = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    for (value, donor, target, filter, kind, requirement, path, fields) in [
        (
            "SEL.V",
            "DID",
            "ID",
            Some("V > 0"),
            ColumnType::Int,
            "REQ-0120",
            "intermediates[0].filter",
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
            "REQ-0116",
            "intermediates[0].key",
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
            "REQ-0117",
            "intermediates[0].key",
            vec![("identifier", text("BAD")), ("intermediate", text("SEL"))],
        ),
        (
            "SEL.BAD",
            "DID",
            "ID",
            None,
            ColumnType::Int,
            "REQ-0125",
            "columns.VALUE.derivation.source",
            vec![("identifier", text("SEL.BAD"))],
        ),
        (
            "SEL.V",
            "DID",
            "ID",
            None,
            ColumnType::Str,
            "REQ-0323",
            "intermediates[0].key",
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
            .find(|d| d.definition().requirement == Some(requirement))
            .unwrap();
        reached.insert(check(
            projected,
            if requirement == "REQ-0323" {
                "incompatible_input_type"
            } else {
                "unknown_field"
            },
            Some(requirement),
            &[path],
            fields,
        ));
    }
    reached
}

pub(crate) fn source_filter_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (variable, predicate, condition, requirement, values) in [
        (
            "SRC.V",
            "ID = 1",
            "unknown_field",
            "REQ-0132",
            vec![("identifier", text("ID")), ("dataset", text("SRC"))],
        ),
        (
            "ID",
            "SRC.V =",
            "prohibited_construct",
            "REQ-0148",
            vec![("identifier", text("ID"))],
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
            Some(requirement),
            &["columns.VALUE.derivation.source.filter"],
            values,
        ));
    }
    reached
}

pub fn verification_reached() -> BTreeSet<ConditionCode> {
    let mut nodes = vec![];
    let assertion = T::map(vec![
        ("when", T::text("FALSE")),
        ("require", T::text("Z > 0 OR A > 0")),
    ]);
    let verifications = T::List(vec![T::map(vec![("assert", assertion)])]);
    let root = T::map(vec![
        ("schema_version", T::text("1.0")),
        ("domain", T::text("TEST")),
        ("base", T::text("SRC")),
        (
            "input",
            T::map(vec![("SRC", T::map(vec![("path", T::text("source.csv"))]))]),
        ),
        ("keys", T::List(vec![T::text("ID")])),
        (
            "columns",
            T::List(vec![column("ID", Some(op("source", "variable", "SRC.ID")))]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID")])),
            ]),
        ),
        ("verifications", verifications),
    ])
    .append(&mut nodes);
    let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap();
    let prepared = PreparedSpecification::prepare(&model).unwrap();
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
        Some("REQ-0405"),
        &["verifications[0].assert.require"],
        vec![("identifier", text("A"))],
    )]
    .into()
}

pub fn verification_declarations_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (kind, column_checks, dataset_checks, condition, requirement, path, reason) in [
        (
            "int",
            T::List(vec![]),
            T::List(vec![T::map(vec![(
                "unique",
                T::map(vec![
                    ("id", T::text("")),
                    ("columns", T::List(vec![T::text("ID")])),
                ]),
            )])]),
            "invalid_declaration",
            "REQ-0374",
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
            "REQ-0397",
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
                        ("columns", T::List(vec![T::text("ID")])),
                    ]),
                )]),
                T::map(vec![(
                    "unique",
                    T::map(vec![
                        ("id", T::text("same")),
                        ("columns", T::List(vec![T::text("ID")])),
                    ]),
                )]),
            ]),
            "duplicate_identifier",
            "REQ-0398",
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
            "REQ-0399",
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
            "REQ-0403",
            "verifications[0].row_count",
            "row_count group_by repeats a column",
        ),
        (
            "str",
            T::List(vec![T::map(vec![(
                "max_length",
                T::map(vec![("max", T::Scalar(N::Integer("0".into())))]),
            )])]),
            T::List(vec![]),
            "invalid_declaration",
            "REQ-0400",
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
            "REQ-0401",
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
            "REQ-0404",
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
            "REQ-0405",
            "verifications[0].all_or_none.columns",
            "unknown column 'UNKNOWN'",
        ),
    ] {
        let mut nodes = vec![];
        let root = T::map(vec![
            ("schema_version", T::text("1.0")),
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
                    column("ID", Some(op("source", "variable", "SRC.ID"))),
                    T::map(vec![
                        ("name", T::text("V")),
                        ("type", T::text(kind)),
                        ("derivation", op("source", "variable", "SRC.V")),
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
        ])
        .append(&mut nodes);
        let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
            .unwrap()
            .unwrap();
        let prepared = PreparedSpecification::prepare(&model).unwrap();
        let findings = prepared.verification_declaration_diagnostics();
        assert_eq!(prepared.verification_declaration_diagnostics(), findings);
        assert_eq!(findings.len(), 1);
        reached.insert(check(
            findings.into_iter().next().unwrap(),
            condition,
            Some(requirement),
            &[path],
            vec![("reason", text(reason))],
        ));
        assert_eq!(prepared.source().path, "source.csv");
    }
    reached
}

/// Inferred source keys use their own normative causes, rather than named-lookup causes.
pub(crate) fn implicit_source_reached() -> BTreeSet<ConditionCode> {
    let tree = T::map(vec![
        ("schema_version", T::text("1.0")),
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
                column("ID", Some(op("source", "variable", "SRC.ID"))),
                column("VALUE", Some(op("source", "variable", "DONOR.V"))),
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
    let mut nodes = vec![];
    let root = tree.append(&mut nodes);
    let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap();
    let prepared = PreparedSpecification::prepare(&model).unwrap();
    let driver = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let mut reached = BTreeSet::new();
    for (name, kind, condition, requirement, context) in [
        (
            "OTHER",
            ColumnType::Int,
            "no_applicable_keys",
            "REQ-0152",
            vec![
                ("dataset", text("DONOR")),
                ("keys", V::Array(vec![text("ID")])),
                (
                    "hint",
                    text("declare an explicit `intermediate:` with `source`/`key` pairs"),
                ),
            ],
        ),
        (
            "ID",
            ColumnType::Str,
            "incompatible_input_type",
            "REQ-0151",
            vec![
                ("source", text("ID")),
                ("expected", text("int")),
                ("actual", text("str")),
            ],
        ),
    ] {
        let donor = TableSchema::new(vec![
            Column {
                name: name.into(),
                kind,
            },
            Column {
                name: "V".into(),
                kind: ColumnType::Int,
            },
        ])
        .unwrap();
        let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&driver, &donor]) else {
            panic!("inferred key admission");
        };
        assert_eq!(findings.len(), 1);
        reached.insert(check(
            findings[0]
                .diagnostics(prepared.source())
                .unwrap()
                .remove(0),
            condition,
            Some(requirement),
            &["columns.VALUE.derivation.source"],
            context,
        ));
    }
    reached
}

/// Omitted named keys share the inference service, while retaining their REQ-0153 cause.
pub(crate) fn intermediate_key_reached() -> BTreeSet<ConditionCode> {
    let tree = T::map(vec![
        ("schema_version", T::text("1.0")),
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
            "intermediates",
            T::List(vec![T::map(vec![
                ("id", T::text("P")),
                ("dataset", T::text("DONOR")),
            ])]),
        ),
        (
            "columns",
            T::List(vec![column("ID", Some(op("source", "variable", "SRC.ID")))]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("result.csv")),
                ("columns", T::List(vec![T::text("ID")])),
            ]),
        ),
    ]);
    let mut nodes = vec![];
    let root = tree.append(&mut nodes);
    let model = SpecificationDocument::admit(Document::new(nodes, root).unwrap())
        .unwrap()
        .unwrap();
    let prepared = PreparedSpecification::prepare(&model).unwrap();
    let driver = TableSchema::new(vec![Column {
        name: "ID".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let donor = TableSchema::new(vec![Column {
        name: "OTHER".into(),
        kind: ColumnType::Int,
    }])
    .unwrap();
    let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&driver, &donor]) else {
        panic!("named inference failure");
    };
    assert_eq!(findings.len(), 1);
    [check(
        findings[0]
            .diagnostics(prepared.source())
            .unwrap()
            .remove(0),
        "no_applicable_keys",
        Some("REQ-0153"),
        &["intermediates[0]"],
        vec![
            ("dataset", text("DONOR")),
            ("keys", V::Array(vec![text("ID")])),
            ("hint", text("declare the `key` explicitly")),
        ],
    )]
    .into()
}
