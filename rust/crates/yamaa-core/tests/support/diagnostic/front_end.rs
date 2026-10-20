//! The 1.0 front end's own causes (FUNC-6, WINDOW-5, LOOKUP-3), raised by its checks
//! on a lowered 1.0 tree and pinned to their phase, condition, rule, path
//! and context.
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
    T::map(fields).document()
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
    reached
}
