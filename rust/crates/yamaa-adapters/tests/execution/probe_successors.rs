//! Unique prototype assertions retained over typed source, engine and issue APIs.
use yamaa_adapters::{issue_rows::Issue, typed_csv};
use yamaa_core::dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate};
use yamaa_core::{
    reduction::NumericReducer,
    table::{Column, TableAccess, TableSchema},
    value::{ColumnType as T, Value},
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
            let source =
                typed_csv::parse(content, &[("id".into(), T::Int), ("x".into(), T::Str)]).unwrap();
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
                match plan
                    .execute(
                        &source,
                        &[],
                        &mut yamaa_core::function_invocation::NoFunctions::new(),
                        None,
                    )
                    .result
                {
                    Ok(result) => {
                        assert_eq!(result.dataset.rows(), [vec![Value::Int(1), Value::Missing]])
                    }
                    Err(error) => {
                        let issue = Issue::from_core(error.diagnostic().unwrap().unwrap()).unwrap();
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
