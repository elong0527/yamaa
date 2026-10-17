//! `str_replace` execution contracts.
#[allow(dead_code)]
mod support;
use support::text_schema as schema;
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, ExecutionError, Expression, Resource, RowMode, RowTemplate,
    },
    diagnostic::ConditionCode,
    function_invocation::NoFunctions,
    numeric_lifecycle::HandlerKind,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    text_replace::StringReplace,
    value::Value,
};
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Table {
    type Error = std::convert::Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn table(text: Value) -> Table {
    Table {
        schema: schema(),
        rows: vec![vec![Value::Str("row1".into()), text]],
    }
}
fn plan(pattern: &str, replacement: &str, missing: Option<Value>) -> DatasetPlan {
    let path = "columns.TEXT.derivation.str_replace";
    DatasetPlan::new(
        schema(),
        schema(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: path.into(),
                    expression: Expression::StrReplace {
                        source: Box::new(Assignment {
                            column: 1,
                            path: format!("{path}.source"),
                            expression: Expression::Source(1),
                        }),
                        operation: StringReplace::new(
                            "SRC.TEXT".into(),
                            path.into(),
                            pattern.into(),
                            replacement.into(),
                            missing,
                        )
                        .unwrap(),
                    },
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}
#[test]
fn replaced_values_and_the_missing_handler_count_once_per_row() {
    for (pattern, replacement, input, expected, missing_count) in [
        (
            r"\s+",
            " ",
            Value::Str(" a  b ".into()),
            Value::Str(" a b ".into()),
            0,
        ),
        (
            "[^0-9]",
            "",
            Value::Str("NONE".into()),
            Value::Str("".into()),
            0,
        ),
        (
            "x",
            "",
            Value::Str("abc".into()),
            Value::Str("abc".into()),
            0,
        ),
        ("x", "", Value::Missing, Value::Str("none".into()), 1),
    ] {
        let result = plan(pattern, replacement, Some(Value::Str("none".into()))).execute(
            &table(input),
            &[],
            &mut NoFunctions::new(),
            None,
        );
        assert_eq!(
            result.result.unwrap().dataset.rows(),
            [vec![Value::Str("row1".into()), expected]]
        );
        assert_eq!(
            result
                .handler_counts
                .iter()
                .map(|count| (count.handler, count.count))
                .collect::<Vec<_>>(),
            [(HandlerKind::Missing, missing_count)]
        );
    }
}
#[test]
fn missing_source_without_handler_keeps_the_row_identity() {
    let attempt =
        plan("a", "b", None).execute(&table(Value::Missing), &[], &mut NoFunctions::new(), None);
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("missing source")
    };
    assert_eq!(diagnostic.code, ConditionCode::StrCaseMissing);
    assert_eq!(identity.unwrap().values, [Value::Str("row1".into())]);
}
#[test]
fn a_step_refusal_is_not_an_unchanged_value() {
    let attempt = plan("(a+)+$", "-", Some(Value::Missing)).execute(
        &table(Value::Str("a".repeat(30) + "!")),
        &[],
        &mut NoFunctions::new(),
        None,
    );
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::RegexWork,
            limit: 1_000_000,
            ..
        }
    ));
    assert!(attempt.handler_counts.iter().all(|count| count.count == 0));
}

#[test]
fn unsupported_patterns_report_the_declaring_operation() {
    use support::document::Tree as T;
    use yamaa_core::{
        schema::SpecificationDocument,
        specification::{PrepareError, PreparedSpecification, UnsupportedFeature},
    };

    // A valid repetition count whose two-scalar body exceeds addressable width.
    let pattern = format!("(?:ab){{{}}}", usize::MAX);
    for operation in ["str_extract", "str_replace"] {
        let mut fields = vec![
            ("source", T::text("SRC.TEXT")),
            ("pattern", T::text(&pattern)),
        ];
        if operation == "str_replace" {
            fields.push(("replacement", T::text("-")));
        }
        let document = T::map(vec![
            ("schema_version", T::text("1.0")),
            ("domain", T::text("TEST")),
            ("base", T::text("SRC")),
            (
                "input",
                T::map(vec![("SRC", T::map(vec![("path", T::text("input.csv"))]))]),
            ),
            ("keys", T::list(vec![T::text("TEXT")])),
            (
                "columns",
                T::list(vec![T::map(vec![
                    ("name", T::text("TEXT")),
                    ("type", T::text("str")),
                    ("derivation", T::map(vec![(operation, T::map(fields))])),
                ])]),
            ),
            (
                "output",
                T::map(vec![
                    ("path", T::text("output.csv")),
                    ("columns", T::list(vec![T::text("TEXT")])),
                ]),
            ),
        ])
        .document();
        let model = SpecificationDocument::admit(document).unwrap().unwrap();
        let Err(PrepareError::Unsupported(features)) = PreparedSpecification::prepare(&model)
        else {
            panic!("unrepresentable pattern must be refused for {operation}");
        };
        assert_eq!(
            features,
            [UnsupportedFeature {
                operation: format!("{operation}_pattern"),
                path: format!("columns.TEXT.derivation.{operation}.pattern"),
            }]
        );
    }
}
