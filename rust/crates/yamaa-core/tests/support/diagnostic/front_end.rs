//! The 1.0 front end's own causes (FUNC-6, WINDOW-5, TIME-10, LOOKUP-3,
//! CSV-10), raised by its checks on a lowered 1.0 tree and pinned to their
//! phase, condition, rule, path and context.
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
    reached
}

/// A 1.0 specification prepared without a project file binds nothing a
/// project supplies: each codelist, inventory vocabulary and project function
/// it names fails as `project_binding_missing` (CT-13) at the field that
/// binds it, naming what it binds. A 0.5 specification keeps its 0.5.0
/// meaning (#3030 D75).
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
