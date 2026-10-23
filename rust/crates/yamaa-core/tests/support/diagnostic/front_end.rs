//! The 1.0 front end's own causes (FUNC-6, WINDOW-5, TIME-10, LOOKUP-3,
//! LOOKUP-17, CSV-10, SUB-3), raised by its checks on a lowered 1.0 tree and
//! pinned to their phase, condition, rule, path and context.
use super::rendered;
use crate::support::document::Tree as T;
use std::collections::BTreeSet;
use yamaa_core::diagnostic::ConditionCode;
use yamaa_core::front_end::v1::spec as front_end;
use yamaa_core::schema::{Document, SchemaContext as C, SchemaDiagnostic};

/// A 1.0 specification of one column, `column`, after its `ID`.
fn spec(column: T) -> Document {
    with(column, Vec::new())
}

/// [`spec`], with the further root fields `extra`.
fn with(column: T, extra: Vec<(&str, T)>) -> Document {
    tree(column, extra).document()
}

/// The tree of [`with`].
fn tree(column: T, extra: Vec<(&str, T)>) -> T {
    let mut fields = vec![
        ("yamaa", T::text("1.0")),
        ("type", T::text("spec")),
        ("domain", T::text("T")),
        ("keys", T::list(vec![T::text("ID")])),
        (
            "input",
            T::map(vec![("SRC", T::map(vec![("path", T::text("src.csv"))]))]),
        ),
        (
            "columns",
            T::list(vec![
                T::map(vec![
                    ("name", T::text("ID")),
                    ("type", T::text("int")),
                    ("derivation", T::map(vec![("value", T::text("SRC.ID"))])),
                ]),
                column,
            ]),
        ),
        (
            "output",
            T::map(vec![
                ("path", T::text("out.csv")),
                ("columns", T::list(vec![T::text("ID")])),
            ]),
        ),
    ];
    fields.extend(extra);
    T::map(fields)
}

fn column(name: &str, kind: &str, operation: &str, payload: T) -> T {
    T::map(vec![
        ("name", T::text(name)),
        ("type", T::text(kind)),
        (
            "derivation",
            T::map(vec![("value", T::map(vec![(operation, payload)]))]),
        ),
    ])
}

/// The one finding of a lowered 1.0 specification's checks.
fn finding(document: &Document) -> SchemaDiagnostic {
    let lowered = front_end::lower(document, None);
    let mut findings = front_end::checks::findings(lowered.spec());
    assert_eq!(findings.len(), 1, "{findings:?}");
    findings.remove(0)
}

fn pinned(
    reached: &mut BTreeSet<ConditionCode>,
    finding: SchemaDiagnostic,
    (condition, rule): (&str, &str),
    path: &str,
    context: Vec<(&'static str, C)>,
) {
    let definition = finding.code.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", condition, Some(rule))
    );
    assert_eq!(finding.path, path);
    assert_eq!(finding.context, context);
    reached.insert(rendered(&finding));
}

pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    // A call argument is one leaf; text that holds an operator is none.
    let call = spec(column(
        "BMI",
        "float",
        "function",
        T::map(vec![
            ("name", T::text("bmi")),
            ("args", T::map(vec![("weight", T::text("SRC.WEIGHT * 2"))])),
        ]),
    ));
    pinned(
        &mut reached,
        finding(&call),
        ("invalid_function_argument", "FUNC-6"),
        "columns.BMI.derivation.function.args.weight",
        vec![
            ("function", C::Text("bmi".into())),
            ("argument", C::Text("weight".into())),
            ("cause", C::Text("expression".into())),
        ],
    );
    // A declared baseline order starts with the date ascending.
    let baseline = spec(column(
        "ABLFL",
        "str",
        "baseline_flag",
        T::map(vec![
            ("date", T::text("ADT")),
            ("reference", T::text("TRTSDT")),
            ("source", T::text("AVAL")),
            (
                "window",
                T::map(vec![(
                    "order_by",
                    T::list(vec![T::text("ATM"), T::text("ADT")]),
                )]),
            ),
        ]),
    ));
    pinned(
        &mut reached,
        finding(&baseline),
        ("value_not_permitted", "WINDOW-5"),
        "columns.ABLFL.derivation.baseline_flag.window.order_by[0]",
        vec![
            ("variable", C::Text("ATM".into())),
            ("direction", C::Text("asc".into())),
            ("date", C::Text("ADT".into())),
        ],
    );
    // A month rule completes no date without a day rule beside it.
    let impute = spec(column(
        "ADT",
        "date",
        "impute",
        T::map(vec![
            ("type", T::text("date")),
            ("source", T::text("SRC.DTC")),
            ("month", T::integer(6)),
        ]),
    ));
    pinned(
        &mut reached,
        finding(&impute),
        ("missing_required_field", "TIME-10"),
        "columns.ADT.derivation.impute",
        vec![("field", C::Text("day".into()))],
    );
    // An intermediate states its own key, unless it drives a row template,
    // which declares none (LOOKUP-3, ROW-2; D23, D63).
    let keyless = with(
        T::map(vec![
            ("name", T::text("NAME")),
            ("type", T::text("str")),
            ("derivation", T::map(vec![("value", T::text("NAMES.NAME"))])),
        ]),
        vec![(
            "intermediates",
            T::list(vec![T::map(vec![
                ("id", T::text("NAMES")),
                ("dataset", T::text("SRC")),
                ("filter", T::text("SRC.KEEP = 'Y'")),
            ])]),
        )],
    );
    pinned(
        &mut reached,
        finding(&keyless),
        ("missing_required_field", "LOOKUP-3"),
        "intermediates.NAMES.key",
        vec![
            ("field", C::Text("key".into())),
            ("class", C::Text("intermediate_class".into())),
        ],
    );
    // An intermediate's `unique` warns only where `keep` and `order_by`
    // choose the record every read takes (LOOKUP-17, D210): one without
    // either, with `keep` alone or with an `order_by` of no term fails at its
    // `severity`, and one beside both, or at `error`, does not.
    let checked = |selection: Vec<(&str, T)>, severity: &str| {
        let mut fields = vec![
            ("id", T::text("LAST")),
            ("dataset", T::text("SRC")),
            ("key", T::list(vec![T::text("ID")])),
        ];
        fields.extend(selection);
        fields.push((
            "verifications",
            T::list(vec![T::map(vec![(
                "unique",
                T::map(vec![
                    ("columns", T::list(vec![T::text("ID")])),
                    ("severity", T::text(severity)),
                ]),
            )])]),
        ));
        with(
            T::map(vec![
                ("name", T::text("V")),
                ("type", T::text("str")),
                ("derivation", T::map(vec![("value", T::text("LAST.V"))])),
            ]),
            vec![("intermediates", T::list(vec![T::map(fields)]))],
        )
    };
    let order = || ("order_by", T::list(vec![T::text("SRC.SEQ")]));
    for selection in [
        Vec::new(),
        vec![("keep", T::text("last"))],
        vec![("order_by", T::list(Vec::new())), ("keep", T::text("last"))],
    ] {
        pinned(
            &mut reached,
            finding(&checked(selection, "warning")),
            ("value_not_permitted", "LOOKUP-17"),
            "intermediates.LAST.verifications[0].unique.severity",
            vec![
                ("intermediate", C::Text("LAST".into())),
                ("value", C::Text("warning".into())),
                ("permitted", C::TextList(vec!["error".into()])),
            ],
        );
    }
    for (selection, severity) in [
        (vec![order(), ("keep", T::text("last"))], "warning"),
        (Vec::new(), "error"),
    ] {
        let lowered = front_end::lower(&checked(selection, severity), None);
        assert_eq!(front_end::checks::findings(lowered.spec()), []);
    }
    // Only the csv profile reads a byte-order mark (CSV-10): an input that
    // another profile reads declares none, whatever the value written, and
    // an input whose extension selects no profile leaves that fault to
    // INPUT-1.
    let marked = |path: &str, value: bool| {
        let mut tree = tree(
            T::map(vec![
                ("name", T::text("V")),
                ("type", T::text("str")),
                ("derivation", T::map(vec![("value", T::text("SRC.V"))])),
            ]),
            Vec::new(),
        );
        tree.set(
            &["input", "SRC"],
            T::map(vec![
                ("path", T::text(path)),
                ("byte_order_mark", T::boolean(value)),
            ]),
        );
        tree.document()
    };
    for (path, value) in [("src.parquet", true), ("src.xpt", false)] {
        pinned(
            &mut reached,
            finding(&marked(path, value)),
            ("value_not_permitted", "CSV-10"),
            "input.SRC.byte_order_mark",
            vec![("dataset", C::Text("SRC".into()))],
        );
    }
    for path in ["src.csv", "src.txt"] {
        for value in [true, false] {
            let lowered = front_end::lower(&marked(path, value), None);
            assert_eq!(front_end::checks::findings(lowered.spec()), []);
        }
    }
    // A specification that declares submission metadata, here one column's,
    // names its model (SUB-3, D25); one that declares none follows none.
    let described = |model: Option<&str>| {
        let mut extra = Vec::new();
        if let Some(model) = model {
            extra.push(("model", T::text(model)));
        }
        with(
            T::map(vec![
                ("name", T::text("SEX")),
                ("type", T::text("str")),
                ("derivation", T::map(vec![("value", T::text("SRC.SEX"))])),
                (
                    "submission",
                    T::map(vec![(
                        "origins",
                        T::list(vec![T::map(vec![
                            ("type", T::text("Collected")),
                            ("source", T::text("Subject")),
                        ])]),
                    )]),
                ),
            ]),
            extra,
        )
    };
    pinned(
        &mut reached,
        finding(&described(None)),
        ("missing_required_field", "SUB-3"),
        "model",
        vec![
            ("field", C::Text("model".into())),
            ("class", C::Text("root_class".into())),
        ],
    );
    let lowered = front_end::lower(&described(Some("SDTM")), None);
    assert_eq!(front_end::checks::findings(lowered.spec()), []);
    // Two origins with one type and source, or any origin beside a
    // Predecessor or Not Available one, conflict at the later origin (SUB-14,
    // D69); distinct origins do not.
    let origin = |kind: &str, source: Option<&str>| {
        let mut fields = vec![("type", T::text(kind))];
        if let Some(source) = source {
            fields.push(("source", T::text(source)));
        }
        if matches!(kind, "Predecessor" | "Not Available") {
            fields.push(("description", T::text("SRC.SEX")));
        }
        T::map(fields)
    };
    let declaring = |origins: Vec<T>| {
        with(
            T::map(vec![
                ("name", T::text("SEX")),
                ("type", T::text("str")),
                ("derivation", T::map(vec![("value", T::text("SRC.SEX"))])),
                ("submission", T::map(vec![("origins", T::list(origins))])),
            ]),
            vec![("model", T::text("SEND"))],
        )
    };
    let lowered = front_end::lower(
        &declaring(vec![
            origin("Collected", Some("Investigator")),
            origin("Collected", Some("Subject")),
            origin("Derived", Some("Sponsor")),
        ]),
        None,
    );
    assert_eq!(front_end::checks::findings(lowered.spec()), []);
    for (origins, path, mut context) in [
        (
            vec![
                origin("Collected", Some("Subject")),
                origin("Collected", Some("Subject")),
            ],
            "columns.SEX.submission.origins[1]",
            vec![
                ("origin_type", C::Text("Collected".into())),
                ("origin_source", C::Text("Subject".into())),
                ("reason", C::Text("same_type_and_source".into())),
            ],
        ),
        (
            vec![
                origin("Collected", Some("Subject")),
                origin("Not Available", None),
            ],
            "columns.SEX.submission.origins[1]",
            vec![
                ("origin_type", C::Text("Not Available".into())),
                ("reason", C::Text("exclusive_type".into())),
            ],
        ),
        (
            vec![
                origin("Predecessor", None),
                origin("Derived", Some("Sponsor")),
            ],
            "columns.SEX.submission.origins[1]",
            vec![
                ("origin_type", C::Text("Derived".into())),
                ("origin_source", C::Text("Sponsor".into())),
                ("reason", C::Text("exclusive_type".into())),
            ],
        ),
    ] {
        context.insert(0, ("column", C::Text("SEX".into())));
        pinned(
            &mut reached,
            finding(&declaring(origins)),
            ("conflicting_origins", "SUB-14"),
            path,
            context,
        );
    }
    // A value-level entry's own origins conflict the same way, naming it.
    let mut entry = tree(
        T::map(vec![
            ("name", T::text("SEX")),
            ("type", T::text("str")),
            ("derivation", T::map(vec![("value", T::text("SRC.SEX"))])),
            (
                "submission",
                T::map(vec![
                    (
                        "origins",
                        T::list(vec![origin("Collected", Some("Subject"))]),
                    ),
                    (
                        "value_level",
                        T::list(vec![T::map(vec![
                            ("id", T::text("CODED")),
                            ("where", T::map(vec![("ID", T::list(vec![T::text("1")]))])),
                            (
                                "origins",
                                T::list(vec![
                                    origin("Assigned", Some("Sponsor")),
                                    origin("Assigned", Some("Sponsor")),
                                ]),
                            ),
                        ])]),
                    ),
                ]),
            ),
        ]),
        Vec::new(),
    );
    entry.set(&["model"], T::text("SDTM"));
    pinned(
        &mut reached,
        finding(&entry.document()),
        ("conflicting_origins", "SUB-14"),
        "columns.SEX.submission.value_level.CODED.origins[1]",
        vec![
            ("column", C::Text("SEX".into())),
            ("entry", C::Text("CODED".into())),
            ("origin_type", C::Text("Assigned".into())),
            ("origin_source", C::Text("Sponsor".into())),
            ("reason", C::Text("same_type_and_source".into())),
        ],
    );
    reached
}

/// A 1.0 specification prepared without a project file binds nothing a
/// project supplies: each codelist, inventory vocabulary and project function
/// it names fails as `project_binding_missing` (CT-13) at the field that
/// binds it, naming what it binds.
pub fn project_binding_reached() -> BTreeSet<ConditionCode> {
    use yamaa_core::json::Value;
    use yamaa_core::specification::{PrepareError, PreparedSpecification};
    let submitted = |name: &str, submission: Vec<(&str, T)>| {
        T::map(vec![
            ("name", T::text(name)),
            ("type", T::text("str")),
            (
                "derivation",
                T::map(vec![("value", T::text(format!("SRC.{name}")))]),
            ),
            ("submission", T::map(submission)),
        ])
    };
    let mut tree = tree(
        submitted("SEX", vec![("codelist", T::text("SEX"))]),
        Vec::new(),
    );
    // After `ID` and `SEX`, a column whose glucose rows bind the inventory,
    // and one that calls a project function, all published.
    let T::Map(fields) = &tree else {
        unreachable!("a specification is a map")
    };
    let Some((_, T::List(written))) = fields.iter().find(|(key, _)| key == "columns") else {
        unreachable!("the specification declares its columns")
    };
    let mut columns = written.clone();
    columns.extend([
        submitted(
            "SRCDOM",
            vec![(
                "value_level",
                T::list(vec![T::map(vec![
                    ("id", T::text("AE")),
                    ("where", T::map(vec![("SEX", T::list(vec![T::text("F")]))])),
                    ("inventory_vocabulary", T::boolean(true)),
                ])]),
            )],
        ),
        column(
            "BMI",
            "float",
            "function",
            T::map(vec![
                ("name", T::text("bmi")),
                ("args", T::map(vec![("weight", T::text("SRC.WEIGHT"))])),
            ]),
        ),
    ]);
    tree.set(&["columns"], T::list(columns));
    tree.set(
        &["output", "columns"],
        T::list(vec![
            T::text("ID"),
            T::text("SEX"),
            T::text("SRCDOM"),
            T::text("BMI"),
        ]),
    );
    let document = tree.document();
    let prepare = |document: &Document| {
        let lowered = front_end::lower(document, None);
        PreparedSpecification::prepare_tree_linked(
            lowered.into_spec(),
            document,
            None,
            Default::default(),
        )
    };
    let Err(PrepareError::Invalid(findings)) = prepare(&document) else {
        panic!("a 1.0 specification without its project fails validation")
    };
    let text = |value: &str| Value::String(value.into());
    let found = findings
        .iter()
        .map(|finding| finding.diagnostic())
        .filter(|diagnostic| diagnostic.code == ConditionCode::ProjectBindingMissing)
        .collect::<Vec<_>>();
    let expected = [
        (
            "columns.BMI.derivation.function.name",
            vec![("binding", text("function")), ("function", text("bmi"))],
        ),
        (
            "columns.SEX.submission.codelist",
            vec![
                ("binding", text("codelist")),
                ("codelist", text("SEX")),
                ("column", text("SEX")),
            ],
        ),
        (
            "columns.SRCDOM.submission.value_level.AE.inventory_vocabulary",
            vec![
                ("binding", text("inventory_vocabulary")),
                ("column", text("SRCDOM")),
            ],
        ),
    ];
    assert_eq!(found.len(), expected.len(), "{found:?}");
    let mut reached = BTreeSet::new();
    for (diagnostic, (path, context)) in found.iter().zip(expected) {
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("validation", "project_binding_missing", Some("CT-13"))
        );
        assert_eq!(diagnostic.spec_paths, [path]);
        assert_eq!(
            diagnostic.context,
            context
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect()
        );
        reached.insert(rendered(diagnostic));
    }
    reached
}
