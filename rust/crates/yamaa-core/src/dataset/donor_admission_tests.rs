//! Independent private-constructor controls for augmented source field timing.
use super::*;
use crate::{
    bound_expression::Binding,
    expression::{compile, Context as ExpressionContext},
    function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence},
    table::Column,
    value::{ColumnType, ValueType},
};

fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap()
}
fn assignment(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("intermediates[0].derivations.C{column}"),
    }
}
fn model(expression: Expression) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new_donor(
        SourceSchemas {
            primary: schema(&["STORED", "FIRST", "LATER", "CURRENT"]),
            secondary: vec![],
        },
        vec![],
        schema(&["STORED", "FIRST", "LATER"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            filter: None,
            assignments: vec![assignment(0, Expression::Source(0))],
        }],
        vec![
            assignment(1, expression),
            assignment(2, Expression::Literal(Value::Int(5))),
        ],
        vec![Some(ColumnType::Int); 3],
    )
}
fn predicate() -> crate::expression::CompiledExpression {
    compile("LATER > 0", ExpressionContext::Predicate, "donor.when").unwrap()
}
fn numeric(column: usize) -> Expression {
    Expression::Compute(
        BoundNumeric::new(
            compile(
                "LATER + 1",
                ExpressionContext::Compute,
                "donor.compute.expr",
            )
            .unwrap(),
            vec![Binding {
                name: "LATER".into(),
                read: Read::Source(column),
            }],
        )
        .unwrap(),
    )
}
fn function(column: usize) -> Expression {
    Expression::Function(
        BoundFunction::new(
            0,
            InvocationPlan::new(
                FunctionIdentity {
                    name: "identity".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "identity".into(),
                },
                vec![Parameter {
                    name: "value".into(),
                    host_name: "value".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                }],
                ColumnType::Int,
                false,
            )
            .unwrap(),
            vec![FunctionArgument {
                name: "value".into(),
                input: FunctionInput::Read(Read::Source(column)),
            }],
        )
        .unwrap(),
    )
}
#[test]
fn later_augmented_source_fields_fail_before_a_donor_plan_can_execute() {
    let flag = Expression::Flag(
        Flag::new(
            predicate(),
            vec![FlagBinding {
                name: "LATER".into(),
                read: FlagRead::Read(Read::Source(2)),
            }],
            Value::Int(1),
            None,
            None,
        )
        .unwrap(),
    );
    let case = Expression::Case(vec![CaseBranch {
        when: Some(
            BoundPredicate::new(
                predicate(),
                vec![Binding {
                    name: "LATER".into(),
                    read: Read::Source(2),
                }],
            )
            .unwrap(),
        ),
        assignment: std::boxed::Box::new(assignment(1, Expression::Literal(Value::Int(1)))),
    }]);
    let nested = Expression::StrCase {
        source: std::boxed::Box::new(assignment(1, numeric(2))),
        operation: StringCase::new(
            "LATER".into(),
            "donor.case".into(),
            crate::text_case::Case::Upper,
            None,
        )
        .unwrap(),
    };
    for expression in [
        Expression::Source(2),
        numeric(2),
        function(2),
        flag,
        case,
        nested,
    ] {
        assert_eq!(model(expression), Err(PlanError::UnavailableColumn));
    }
}
#[test]
fn stored_initialization_and_appended_current_operands_remain_admitted() {
    for expression in [
        Expression::Source(0),
        numeric(0),
        function(0),
        Expression::Source(3),
        numeric(3),
        function(3),
    ] {
        assert!(model(expression).is_ok());
    }
}

fn ranged_donor(
    current: CurrentRead,
    runtime_kind: Option<ColumnType>,
    layout_kind: ValueType,
) -> Result<DatasetPlan, PlanError> {
    let fields = schema(&["CURRENT", "SELECTED"]);
    DatasetPlan::new_donor(
        SourceSchemas {
            primary: fields.clone(),
            secondary: vec![SecondarySource {
                name: "DONOR".into(),
                schema: schema(&["LOW", "VALUE"]),
            }],
        },
        vec![Intermediate {
            identifier: "SELECTOR".into(),
            path: "intermediates[0]".into(),
            source: IntermediateSource::Input(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "intermediates[0].match_by.LOW".into(),
                value: MatchValue::Literal(Value::Int(5)),
            }],
            filter: None,
            selection: None,
            range: Some(
                RelationRange::new(
                    "intermediates[0].between".into(),
                    CurrentField {
                        read: current,
                        identifier: "CURRENT".into(),
                    },
                    Some(layout_kind),
                    Some(RelationRangeBound {
                        column: 0,
                        identifier: "LOW".into(),
                        kind: Some(ValueType::Int),
                    }),
                    None,
                )
                .unwrap(),
            ),
            columns: Some(vec![1]),
            no_match: Some(Value::Missing),
            require_match: false,
            unique: vec![],
            derivations: None,
            pending_schema: None,
        }],
        fields,
        vec![RowTemplate {
            mode: RowMode::Records,
            filter: None,
            assignments: vec![assignment(0, Expression::Source(0))],
        }],
        vec![assignment(
            1,
            Expression::Intermediate {
                index: 0,
                column: 1,
            },
        )],
        vec![runtime_kind, Some(ColumnType::Int)],
    )
}

#[test]
fn donor_ranges_admit_untyped_output_prefix_and_original_record_fields() {
    for current in [
        CurrentRead::Output(0),
        CurrentRead::Record {
            source: 0,
            column: 0,
        },
    ] {
        let plan = ranged_donor(current, None, ValueType::Int)
            .expect("a physical layout does not establish an untyped donor's runtime kind");
        assert_eq!(plan.output_kind(0), None);
        assert_eq!(
            plan.intermediates()[0].range.as_ref().unwrap().value_kind(),
            Some(ValueType::Int)
        );
        assert!(ranged_donor(current, Some(ColumnType::Int), ValueType::Int).is_ok());
        assert_eq!(
            ranged_donor(current, Some(ColumnType::Int), ValueType::Float),
            Err(PlanError::InvalidIntermediate),
            "known current and declared range kinds must still agree",
        );
    }
}
