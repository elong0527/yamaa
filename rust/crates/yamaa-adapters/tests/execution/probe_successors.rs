//! Unique prototype assertions retained over typed source, engine and issue APIs.
use yamaa_adapters::{arrow_table::TableLimits, issue_rows::Issue, typed_csv};
use yamaa_core::{
    reduction::NumericReducer,
    table::{Column, TableAccess, TableSchema},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{
    Assignment, DatasetExecution, DatasetPlan, Expression, RowMode, RowTemplate,
};
#[test]
fn grouped_reduction_preserves_authored_identifier_missingness_and_fresh_attempts() {
    let output = TableSchema::new(vec![
        Column {
            name: "id".into(),
            kind: T::Int,
        },
        Column {
            name: "total".into(),
            kind: T::Float,
        },
    ])
    .unwrap();
    for identifier in [None, Some("SRC.x".to_owned())] {
        for content in [b"id,x\n1,\n1,2\n".as_slice(), b"id,x\n1,\n1,\n".as_slice()] {
            let source = typed_csv::parse(
                content,
                &[("id".into(), T::Int), ("x".into(), T::Str)],
                Default::default(),
                TableLimits {
                    max_rows: 10,
                    max_columns: 2,
                    max_batches: 1,
                    max_cells: 20,
                },
            )
            .unwrap();
            let plan = DatasetPlan::new(
                source.schema().clone(),
                output.clone(),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    filter: None,
                    assignments: vec![
                        Assignment {
                            column: 0,
                            path: "rows[0].derivations.id.source".into(),
                            expression: Expression::Source(0),
                        },
                        Assignment {
                            column: 1,
                            path: "rows[0].derivations.total.aggregate".into(),
                            expression: Expression::Reduce {
                                identifier: identifier.clone(),
                                column: 1,
                                reducer: NumericReducer::Sum,
                                text: "SUM(SRC.x)".into(),
                            },
                        },
                    ],
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            for _ in 0..2 {
                match plan.execute(
                    &source,
                    yamaa_engine::dataset::Limits {
                        source_rows: 10,
                        output_rows: 10,
                        output_cells: 20,
                        key_cells: 10,
                        work_cells: 1000,
                        scalar_text_bytes: 1000,
                        output_text_bytes: 1000,
                        identity_cells: 10,
                        identity_text_bytes: 1000,
                    },
                ) {
                    Ok(result) => {
                        assert_eq!(result.dataset.rows(), [vec![Value::Int(1), Value::Missing]])
                    }
                    Err(error) => {
                        let issue =
                            Issue::from_core(error.diagnostic_with_limit(4096).unwrap()).unwrap();
                        assert_eq!(issue.condition, "incompatible_input_type");
                        assert_eq!(issue.requirement.as_deref(), Some("REQ-0510"));
                        assert_eq!(issue.spec_paths, ["rows[0].derivations.total.aggregate"]);
                        assert_eq!(
                            serde_json::from_str::<serde_json::Value>(&issue.context).unwrap(),
                            serde_json::json!({"expr":"SUM(SRC.x)","reducer":"SUM","source":identifier.as_deref().unwrap_or("x"),"expected":"numeric","actual":"str"})
                        );
                    }
                }
            }
        }
    }
}
