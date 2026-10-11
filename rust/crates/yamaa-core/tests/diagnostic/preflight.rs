//! Exercise the original-document compiler, not constructed failure enum values.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V},
    schema::{Document, DocumentNode as N, SpecificationDocument, ValidationBudget},
    specification::{PrepareError, PreparedSpecification},
    value::Value,
};

enum Tree<'a> {
    Text(&'a str),
    Integer(&'a str),
    Map(Vec<(&'a str, Tree<'a>)>),
    List(Vec<Tree<'a>>),
}
use Tree::*;
impl Tree<'_> {
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Text(value) => N::Text(value.into()),
            Integer(value) => N::Integer(value.into()),
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
fn source<'a>() -> Tree<'a> {
    Map(vec![(
        "value",
        Map(vec![("source", Map(vec![("variable", Text("SRC.ID"))]))]),
    )])
}
fn column<'a>(name: &'a str, derivation: Option<Tree<'a>>) -> Tree<'a> {
    let mut fields = vec![("name", Text(name)), ("type", Text("int"))];
    if let Some(derivation) = derivation {
        fields.push(("derivation", derivation));
    }
    Map(fields)
}
fn inputs<'a>() -> Tree<'a> {
    Map(vec![
        ("SRC", Map(vec![("path", Text("source.csv"))])),
        ("SECOND", Map(vec![("path", Text("second.csv"))])),
    ])
}
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn texts(values: &[&str]) -> V {
    V::Sequence(values.iter().map(|v| text(v)).collect())
}
type Expected<'a> = (&'a str, Option<&'a str>, Vec<&'a str>, Vec<(&'a str, V)>);

pub(super) fn output_reached() -> BTreeSet<ConditionCode> {
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", Some(source()))],
        vec![Text("ID")],
    );
    fields.push(("base", Text("SRC")));
    let output = &mut fields
        .iter_mut()
        .find(|(name, _)| *name == "output")
        .unwrap()
        .1;
    *output = Map(vec![
        ("path", Text("result.bad")),
        ("columns", List(vec![Text("ABSENT"), Text("ABSENT")])),
    ]);
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    let document = Document::new(nodes, root, Default::default()).unwrap();
    let model =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    let compiled = PreparedSpecification::prepare(&model).unwrap();
    let findings = compiled.output_findings();
    let expected = [
        (
            "unknown_artifact_profile",
            "REQ-0760",
            "output.path",
            vec![
                ("path", text("result.bad")),
                ("permitted", texts(&[".csv", ".parquet"])),
            ],
        ),
        (
            "undeclared_column",
            "REQ-0234",
            "output.columns[0]",
            vec![("column", text("ABSENT"))],
        ),
        (
            "duplicate_identifier",
            "REQ-0234",
            "output.columns[1]",
            vec![("column", text("ABSENT"))],
        ),
        (
            "internal_column_in_keys",
            "REQ-0220",
            "keys[0]",
            vec![("column", text("ID"))],
        ),
    ];
    assert_eq!(findings.len(), expected.len());
    let mut reached: BTreeSet<_> = findings
        .iter()
        .zip(expected)
        .map(|(finding, (condition, requirement, path, fields))| {
            let diagnostic = finding.diagnostic();
            let definition = diagnostic.definition();
            assert_eq!(
                (
                    definition.phase,
                    definition.condition,
                    definition.requirement
                ),
                ("validation", condition, Some(requirement))
            );
            assert_eq!(diagnostic.spec_paths, [path]);
            assert_eq!(
                diagnostic.context,
                fields.into_iter().map(|(k, v)| (k.into(), v)).collect()
            );
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            diagnostic.code
        })
        .collect();
    for (precision, path, condition, requirement, context) in [
        (
            "-9223372036854775809",
            "result.csv",
            "invalid_field_type",
            "REQ-0744",
            vec![
                ("expected", text("a non-negative integer")),
                ("actual", V::Integer("-9223372036854775809".into())),
            ],
        ),
        (
            "2",
            "result.parquet",
            "decimals_not_applicable",
            "REQ-0762",
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
            vec![Text("ID")],
        );
        fields.push(("base", Text("SRC")));
        fields
            .iter_mut()
            .find(|(name, _)| *name == "output")
            .unwrap()
            .1 = Map(vec![
            ("path", Text(path)),
            ("decimals", Integer(precision)),
            ("columns", List(vec![Text("ID")])),
        ]);
        let mut nodes = Vec::new();
        let root = Map(fields).append(&mut nodes);
        let document = Document::new(nodes, root, Default::default()).unwrap();
        let model =
            SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
                .unwrap()
                .unwrap();
        let compiled = PreparedSpecification::prepare(&model).unwrap();
        assert_eq!(compiled.output_decimals(), Some(precision));
        let findings = compiled.output_findings();
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_paths, ["output.decimals"]);
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect()
        );
        reached.insert(diagnostic.code);
    }
    for (names, condition, position, name) in [
        (vec!["ID", "ID"], "duplicate_order_term", 1, "ID"),
        (vec!["SRC.ID"], "undeclared_column", 0, "SRC.ID"),
    ] {
        let mut fields = base(
            "TEST",
            inputs(),
            vec![column("ID", Some(source()))],
            vec![Text("ID")],
        );
        fields.push(("base", Text("SRC")));
        fields
            .iter_mut()
            .find(|(name, _)| *name == "output")
            .unwrap()
            .1 = Map(vec![
            ("path", Text("result.csv")),
            ("columns", List(vec![Text("ID")])),
            (
                "order_by",
                List(
                    names
                        .into_iter()
                        .map(|name| {
                            Map(vec![
                                ("variable", Text(name)),
                                ("direction", Text("asc")),
                                ("nulls", Text("last")),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
        let mut nodes = Vec::new();
        let root = Map(fields).append(&mut nodes);
        let model = SpecificationDocument::admit(
            Document::new(nodes, root, Default::default()).unwrap(),
            &mut ValidationBudget::new(Default::default()),
        )
        .unwrap()
        .unwrap();
        let compiled = PreparedSpecification::prepare(&model).unwrap();
        let findings = compiled.output_findings();
        assert_eq!(findings.len(), 1);
        let diagnostic = findings[0].diagnostic();
        assert_eq!(
            (
                diagnostic.definition().phase,
                diagnostic.definition().condition,
                diagnostic.definition().requirement
            ),
            ("validation", condition, Some("REQ-0224"))
        );
        assert_eq!(
            diagnostic.spec_paths,
            [format!("output.order_by[{position}]")]
        );
        assert_eq!(
            diagnostic.context,
            [("column".into(), text(name))].into_iter().collect()
        );
        reached.insert(diagnostic.code);
    }
    let mut fields = base(
        "TEST",
        Map(vec![("SRC", Map(vec![("path", Text("source.csv"))]))]),
        vec![column("ID", Some(source()))],
        vec![Text("ID")],
    );
    fields.push(("filter", Text("ID = 1")));
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    let model = SpecificationDocument::admit(
        Document::new(nodes, root, Default::default()).unwrap(),
        &mut ValidationBudget::new(Default::default()),
    )
    .unwrap()
    .unwrap();
    let prepared = PreparedSpecification::prepare(&model).unwrap();
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
            diagnostic.definition().requirement
        ),
        ("validation", "phase_boundary", Some("REQ-1170"))
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
    reached.insert(diagnostic.code);
    let collision = yamaa_core::source_ordinal::Ordinal::admit(
        &source,
        0,
        "ID",
        yamaa_core::source_ordinal::Limits {
            columns: 0,
            cells: 0,
        },
    )
    .err()
    .unwrap();
    let diagnostic = collision
        .finding("SRC", "ID")
        .unwrap()
        .diagnostic()
        .unwrap();
    assert_eq!(
        (
            diagnostic.definition().phase,
            diagnostic.definition().condition,
            diagnostic.definition().requirement
        ),
        ("validation", "ordinal_field_collision", Some("REQ-1294"))
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
    reached.insert(diagnostic.code);
    reached
}

fn check(fields: Vec<(&str, Tree<'_>)>, expected: Vec<Expected<'_>>) -> BTreeSet<ConditionCode> {
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    let document = Document::new(nodes, root, Default::default()).unwrap();
    let model =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&model) else {
        panic!("preflight error from compiler");
    };
    assert_eq!(findings.len(), expected.len());
    let mut reached = BTreeSet::new();
    for (finding, (condition, requirement, paths, context)) in findings.iter().zip(expected) {
        let diagnostic = finding.diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("validation", condition, requirement)
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
        reached.insert(diagnostic.code);
    }
    reached
}
fn base<'a>(
    domain: &'a str,
    input: Tree<'a>,
    columns: Vec<Tree<'a>>,
    keys: Vec<Tree<'a>>,
) -> Vec<(&'a str, Tree<'a>)> {
    vec![
        ("schema_version", Text("1.0")),
        ("domain", Text(domain)),
        ("input", input),
        ("keys", List(keys)),
        ("columns", List(columns)),
        (
            "output",
            Map(vec![
                ("path", Text("result.csv")),
                ("columns", List(vec![Text("ID")])),
            ]),
        ),
    ]
}

pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut log_codes = BTreeSet::new();
    log_codes.extend(check(
        base(
            "TEST",
            Map(vec![(
                "SRC",
                Map(vec![
                    ("path", Text("source.parquet")),
                    ("ordinal", Text("REC")),
                ]),
            )]),
            vec![column(
                "ID",
                Some(Map(vec![(
                    "value",
                    Map(vec![("odm", Map(vec![("item", Text("SRC.ITEM"))]))]),
                )])),
            )],
            vec![Text("ID")],
        ),
        vec![(
            "odm_schema_field_type",
            Some("REQ-1295"),
            vec!["input.SRC.ordinal"],
            vec![("dataset", text("SRC")), ("declared", text("ordinal"))],
        )],
    ));
    for (path, policy, condition, requirement, spec_path, fields) in [
        (
            "input.txt",
            "missing",
            "source_profile_unknown",
            "REQ-0852",
            "input.SRC.path",
            vec![("dataset", text("SRC")), ("path", text("input.txt"))],
        ),
        (
            "input.CSV",
            "present",
            "empty_string_present_unsupported",
            "REQ-1161",
            "input.SRC.empty_string",
            vec![("dataset", text("SRC"))],
        ),
    ] {
        log_codes.extend(check(
            base(
                "TEST",
                Map(vec![(
                    "SRC",
                    Map(vec![("path", Text(path)), ("empty_string", Text(policy))]),
                )]),
                vec![column("ID", Some(source()))],
                vec![Text("ID")],
            ),
            vec![(condition, Some(requirement), vec![spec_path], fields)],
        ));
    }
    for (warning, verification, condition, requirement, paths) in [
        (
            Some("result.csv"),
            None,
            "artifact_path_collision",
            "REQ-0756",
            vec!["output.path", "output.warning_log"],
        ),
        (
            None,
            Some("result.csv"),
            "artifact_path_collision",
            "REQ-1180",
            vec!["output.path", "output.verification_log"],
        ),
        (
            Some("same.csv"),
            Some("same.csv"),
            "artifact_path_collision",
            "REQ-1180",
            vec!["output.warning_log", "output.verification_log"],
        ),
    ] {
        let mut fields = base(
            "TEST",
            Map(vec![("SRC", Map(vec![("path", Text("input.csv"))]))]),
            vec![column("ID", Some(source()))],
            vec![Text("ID")],
        );
        let mut output = vec![
            ("path", Text("result.csv")),
            ("columns", List(vec![Text("ID")])),
        ];
        if let Some(path) = warning {
            output.push(("warning_log", Text(path)));
        }
        if let Some(path) = verification {
            output.push(("verification_log", Text(path)));
        }
        fields
            .iter_mut()
            .find(|(name, _)| *name == "output")
            .unwrap()
            .1 = Map(output);
        log_codes.extend(check(
            fields,
            vec![(
                condition,
                Some(requirement),
                paths,
                vec![("path", text(verification.unwrap_or("result.csv")))],
            )],
        ));
    }
    let mut fields = base(
        "TEST",
        Map(vec![("SRC", Map(vec![("path", Text("input.csv"))]))]),
        vec![column("ID", Some(source()))],
        vec![Text("ID")],
    );
    fields.push((
        "verifications",
        List(vec![Map(vec![(
            "row_count",
            Map(vec![("min", Integer("1")), ("severity", Text("warning"))]),
        )])]),
    ));
    log_codes.extend(check(
        fields,
        vec![(
            "missing_warning_log",
            Some("REQ-0391"),
            vec!["output.warning_log"],
            vec![("warnings", texts(&["verifications[0].row_count"]))],
        )],
    ));
    let mut fields = base(
        "SRC",
        inputs(),
        vec![column("ID", None)],
        vec![Text("MISSING")],
    );
    fields.push(("base", Text("ABSENT")));
    let mut reached = check(
        fields,
        vec![
            (
                "missing_derivation",
                Some("REQ-0198"),
                vec!["columns.ID.derivation"],
                vec![("column", text("ID"))],
            ),
            (
                "duplicate_identifier",
                Some("REQ-0080"),
                vec!["input.SRC", "domain"],
                vec![("identifier", text("SRC"))],
            ),
            (
                "undeclared_column",
                Some("REQ-0220"),
                vec!["keys[0]"],
                vec![("column", text("MISSING"))],
            ),
            (
                "driver_unavailable",
                None,
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
            vec![Text("ID")],
        ),
        vec![(
            "driver_unavailable",
            None,
            vec!["base"],
            vec![("row", V::Scalar(Value::Missing))],
        )],
    ));
    let aggregate = Map(vec![(
        "value",
        Map(vec![(
            "aggregate",
            Map(vec![("expr", Text("SUM(SRC.ID)"))]),
        )]),
    )]);
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", None), column("X", Some(aggregate))],
        vec![Text("ID")],
    );
    fields.push(("filter", Text("TRUE")));
    fields.push((
        "rows",
        List(vec![
            Map(vec![
                ("id", Text("first")),
                ("dataset", Text("ABSENT")),
                ("group_by", List(vec![])),
                (
                    "derivations",
                    Map(vec![("BOGUS", source()), ("X", source())]),
                ),
            ]),
            Map(vec![
                ("id", Text("second")),
                ("dataset", Text("SRC")),
                ("group_by", List(vec![Text("SECOND.ID")])),
                ("derivations", Map(vec![("ID", source()), ("X", source())])),
            ]),
            Map(vec![
                ("id", Text("third")),
                ("derivations", Map(vec![("ID", source()), ("X", source())])),
            ]),
        ]),
    ));
    reached.extend(check(
        fields,
        vec![
            (
                "undeclared_column",
                None,
                vec!["rows[0].derivations.BOGUS"],
                vec![("column", text("BOGUS"))],
            ),
            (
                "missing_derivation",
                Some("REQ-0200"),
                vec!["columns.ID.derivation"],
                vec![("column", text("ID")), ("rows", texts(&["first"]))],
            ),
            (
                "duplicate_derivation",
                Some("REQ-1260"),
                vec!["columns.X.derivation"],
                vec![
                    ("column", text("X")),
                    ("rows", texts(&["first", "second", "third"])),
                ],
            ),
            (
                "conflicting_row_construction",
                Some("REQ-1171"),
                vec!["filter", "rows"],
                vec![],
            ),
            (
                "driver_unavailable",
                None,
                vec!["rows[0].dataset"],
                vec![("row", text("first")), ("dataset", text("ABSENT"))],
            ),
            (
                "invalid_field_type",
                Some("REQ-0065"),
                vec!["rows[0].group_by"],
                vec![("row", text("first")), ("group_by", texts(&[]))],
            ),
            (
                "unknown_field",
                Some("REQ-0066"),
                vec!["rows[1].group_by"],
                vec![
                    ("row", text("second")),
                    ("identifier", text("SECOND.ID")),
                    ("dataset", text("SRC")),
                ],
            ),
            (
                "driver_unavailable",
                None,
                vec!["rows[2].dataset"],
                vec![
                    ("row", text("third")),
                    ("dataset", V::Scalar(Value::Missing)),
                ],
            ),
        ],
    ));
    // Duplicate groups retain authored order instead of sorting or deduplicating.
    let mut fields = base(
        "TEST",
        inputs(),
        vec![column("ID", Some(source()))],
        vec![Text("ID")],
    );
    fields.push((
        "rows",
        List(vec![Map(vec![
            ("id", Text("duplicates")),
            ("dataset", Text("SRC")),
            ("group_by", List(vec![Text("SRC.ID"), Text("SRC.ID")])),
            ("derivations", Map(vec![])),
        ])]),
    ));
    reached.extend(check(
        fields,
        vec![(
            "invalid_field_type",
            Some("REQ-0065"),
            vec!["rows[0].group_by"],
            vec![
                ("row", text("duplicates")),
                ("group_by", texts(&["SRC.ID", "SRC.ID"])),
            ],
        )],
    ));
    for kind in ["str", "int", "float", "date", "datetime"] {
        let input = Map(vec![(
            "SRC",
            Map(vec![
                ("path", Text("source.PARQUET")),
                ("types", Map(vec![("ID", Text(kind))])),
            ]),
        )]);
        reached.extend(check(
            base(
                "TEST",
                input,
                vec![column("ID", Some(source()))],
                vec![Text("ID")],
            ),
            vec![(
                "redundant_field_type",
                Some("REQ-0533"),
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

pub(super) fn temporal_reached() -> BTreeSet<ConditionCode> {
    let mut fields = base(
        "TEST",
        inputs(),
        vec![
            column("ID", Some(source())),
            column(
                "DIFF",
                Some(Map(vec![(
                    "value",
                    Map(vec![(
                        "date_diff",
                        Map(vec![
                            ("start", Text("ID")),
                            ("end", Text("ID")),
                            ("unit", Text("month")),
                            ("bounds", Text("inclusive")),
                        ]),
                    )]),
                )])),
            ),
        ],
        vec![Text("ID")],
    );
    fields.push(("base", Text("SRC")));
    let mut nodes = Vec::new();
    let root = Map(fields).append(&mut nodes);
    let document = Document::new(nodes, root, Default::default()).unwrap();
    let model =
        SpecificationDocument::admit(document, &mut ValidationBudget::new(Default::default()))
            .unwrap()
            .unwrap();
    let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&model) else {
        panic!("temporal preflight rejection")
    };
    assert_eq!(findings.len(), 3);
    let bounds = findings[0].diagnostic();
    assert_eq!(bounds.definition().requirement, Some("REQ-0613"));
    assert_eq!(
        bounds.spec_paths,
        ["columns.DIFF.derivation.date_diff.bounds"]
    );
    assert_eq!(
        bounds.context,
        [
            ("value".into(), text("inclusive")),
            ("permitted".into(), texts(&["exclusive"]))
        ]
        .into()
    );
    let mut reached = BTreeSet::new();
    reached.insert(bounds.code);
    for (finding, field) in findings[1..].iter().zip(["start", "end"]) {
        let diagnostic = finding.diagnostic();
        assert_eq!(diagnostic.definition().requirement, Some("REQ-0606"));
        assert_eq!(
            diagnostic.spec_paths,
            [format!("columns.DIFF.derivation.date_diff.{field}")]
        );
        assert_eq!(
            diagnostic.context,
            [
                ("source".into(), text("ID")),
                ("expected".into(), text("date")),
                ("actual".into(), text("int"))
            ]
            .into()
        );
        reached.insert(diagnostic.code);
    }
    reached
}
