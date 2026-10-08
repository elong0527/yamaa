//! Actual binder, scope and dependency failures reach the canonical cause registry.
use std::collections::BTreeSet;
use yamaa_core::{
    column_dependencies,
    diagnostic::{ConditionCode, Context, ContextValue as V, Diagnostic},
    reference_binding::{Catalog, Dataset, Field},
    reference_scope::{self, Phase, Reach, Scope},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    specification::{
        BindError, BindFinding, PreparedSpecification, SourceDeclaration, SourceProfile,
    },
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};

fn text(v: &str) -> V {
    V::Scalar(Value::Str(v.into()))
}
fn check(
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
fn source() -> SourceDeclaration {
    SourceDeclaration {
        name: "SRC".into(),
        path: "source.csv".into(),
        types: vec![],
        profile: SourceProfile::Csv,
        empty_string_present: false,
    }
}

enum T<'a> {
    Scalar(N),
    Text(&'a str),
    Map(Vec<(&'a str, T<'a>)>),
    List(Vec<T<'a>>),
}
use T::*;
impl T<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            T::Scalar(node) => node,
            Text(v) => N::Text(v.into()),
            Map(fields) => N::Mapping(
                fields
                    .into_iter()
                    .map(|(k, v)| (Text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
            List(values) => N::Sequence(values.into_iter().map(|v| v.append(nodes)).collect()),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
}
fn op<'a>(name: &'a str, field: &'a str, value: &'a str) -> T<'a> {
    Map(vec![(
        "value",
        Map(vec![(name, Map(vec![(field, Text(value))]))]),
    )])
}
fn column<'a>(name: &'a str, derivation: Option<T<'a>>) -> T<'a> {
    let mut fields = vec![("name", Text(name)), ("type", Text("int"))];
    if let Some(d) = derivation {
        fields.push(("derivation", d));
    }
    Map(fields)
}
fn compile(value: T<'_>, rows: Option<T<'_>>) -> PreparedSpecification {
    let mut fields = vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("source.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column(
                    "ID",
                    rows.is_none().then_some(op("source", "variable", "SRC.ID")),
                ),
                column("VALUE", rows.is_none().then_some(value)),
            ]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("VALUE")])),
            ]),
        ),
    ];
    if let Some(rows) = rows {
        fields.push(("rows", rows));
    }
    let mut nodes = vec![];
    let root = Map(fields).append(&mut nodes);
    let model = SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap();
    PreparedSpecification::prepare(&model).unwrap()
}
fn failures(prepared: &PreparedSpecification) -> Vec<BindFinding> {
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

pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
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
        Default::default(),
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
            Default::default(),
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
            None,
            vec![("identifier", text("MISSING"))],
        ),
        (
            "source",
            "variable",
            "V",
            "unresolvable_name",
            None,
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
        let rows = List(vec![Map(vec![
            ("id", Text("first")),
            ("dataset", Text("SRC")),
            ("group_by", List(vec![Text("SRC.ID")])),
            (
                "derivations",
                Map(vec![
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
        let analysis =
            column_dependencies::analyze(&dependencies, &keys, false, Default::default()).unwrap();
        for f in analysis.diagnostics {
            let (condition, requirement, site, fields) = match &f {
                column_dependencies::Diagnostic::Cycle { .. } => (
                    "dependency_cycle",
                    "REQ-0072",
                    vec![paths[0].as_str(), paths[1].as_str()],
                    vec![(
                        "cycle",
                        V::Sequence(vec![text("ID"), text("VALUE"), text("ID")]),
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

#[test]
fn malformed_dependency_metadata_has_no_projection_or_panic() {
    let analysis = column_dependencies::analyze(
        &[Some(vec![1]), Some(vec![0])],
        &[],
        false,
        Default::default(),
    )
    .unwrap();
    for finding in analysis.diagnostics {
        assert!(finding.specification_diagnostic(&[], &[]).is_none());
    }
}

pub(super) fn window_reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    let source = source();
    for (operation, payload, condition, requirement, suffix, values) in [
        (
            "row_value",
            Map(vec![
                ("source", Text("ID")),
                ("offset", T::Scalar(N::Integer("0".into()))),
                ("window", Map(vec![("order_by", List(vec![Text("ID")]))])),
            ]),
            "zero_offset",
            "REQ-0328",
            ".offset",
            vec![("offset", V::Scalar(Value::Int(0)))],
        ),
        (
            "rank",
            Map(vec![]),
            "window_order_by_required",
            "REQ-0340",
            ".window",
            vec![("operation", text("rank"))],
        ),
        (
            "baseline_flag",
            Map(vec![
                ("date", Text("ID")),
                ("reference_date", Text("ID")),
                ("window", Map(vec![("order_by", List(vec![Text("ID")]))])),
            ]),
            "window_order_by_forbidden",
            "REQ-0341",
            ".window.order_by",
            vec![("operation", text("baseline_flag"))],
        ),
    ] {
        let prepared = compile(Map(vec![("value", Map(vec![(operation, payload)]))]), None);
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
    reached
}

fn lookup_compile(
    value: &str,
    donor: &str,
    target: &str,
    filter: Option<&str>,
) -> PreparedSpecification {
    let mut selection = vec![
        ("id", Text("SEL")),
        ("dataset", Text("DONOR")),
        ("key", Map(vec![(donor, Text(target))])),
    ];
    if let Some(filter) = filter {
        selection.push(("filter", Text(filter)));
    }
    let tree = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        (
            "input",
            Map(vec![
                ("SRC", Map(vec![("path", Text("source.csv"))])),
                ("DONOR", Map(vec![("path", Text("donor.csv"))])),
            ]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![
                column("ID", Some(op("source", "variable", "SRC.ID"))),
                column("VALUE", Some(op("source", "variable", value))),
            ]),
        ),
        ("intermediates", List(vec![Map(selection)])),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID"), Text("VALUE")])),
            ]),
        ),
    ]);
    let mut nodes = vec![];
    let root = tree.append(&mut nodes);
    let model = SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap();
    PreparedSpecification::prepare(&model).unwrap()
}

pub(super) fn lookup_reached() -> BTreeSet<ConditionCode> {
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

pub(super) fn source_filter_reached() -> BTreeSet<ConditionCode> {
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
            Map(vec![(
                "value",
                Map(vec![(
                    "source",
                    Map(vec![
                        ("variable", Text(variable)),
                        ("filter", Text(predicate)),
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
    let assertion = Map(vec![
        ("when", Text("FALSE")),
        ("require", Text("Z > 0 OR A > 0")),
    ]);
    let verifications = List(vec![Map(vec![("assert", assertion)])]);
    let root = Map(vec![
        ("schema_version", Text("1.0")),
        ("domain", Text("TEST")),
        ("base", Text("SRC")),
        (
            "input",
            Map(vec![("SRC", Map(vec![("path", Text("source.csv"))]))]),
        ),
        ("keys", List(vec![Text("ID")])),
        (
            "columns",
            List(vec![column("ID", Some(op("source", "variable", "SRC.ID")))]),
        ),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
        ("verifications", verifications),
    ])
    .append(&mut nodes);
    let model = SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
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
