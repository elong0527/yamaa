//! A causal component uses ordinary expression evaluation and conversion once
//! per cell, in the order its backward row reads make available.
use super::{windows, Fail, Frame, Run, Table};
use crate::dataset::resolved::{RecurrenceNode, RegionId, Step};

impl<'p, 'a, E> Run<'p, 'a, E> {
    pub(super) fn complete_recurrence<'s>(
        &mut self,
        frame: &mut Frame,
        secondary: &[Table<'s, E>],
        group: &RecurrenceNode,
    ) -> Result<(), Fail<E>>
    where
        'a: 's,
    {
        let plan = self.plan;
        let region = RegionId::DATASET;
        let step = Step::Column(group.columns[0]);
        self.poll(step)?;
        self.windows.step();
        let bindings = group
            .columns
            .iter()
            .flat_map(|&column| plan.column_bindings(column))
            .collect::<Vec<_>>();
        let first = plan.binding(bindings[0]);
        let rows = {
            let table = self.source_table(region, first.driver.source, secondary);
            let mut cx = self.template_cx(frame, region, table, secondary, frame.rows());
            cx.output = 0..frame.rows();
            windows::row_order(self, &cx, group.window)?
        };
        // No speculative batch: a failure retains exactly the handlers of
        // the cells completed before it in this causal order.
        for row in rows {
            self.poll(step)?;
            for &binding in &bindings {
                self.complete_column(frame, binding, secondary, &[row], false)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Seam: reuse one resolved plan across cancellation and different input
    // tables. The file-fixture reader owns the ordinary one-run examples.
    use crate::bound_expression::{Binding, BoundNumeric, BoundPredicate, Read};
    use crate::dataset::{
        recurrence::Recurrence, Assignment, CaseBranch, Dataset, DatasetPlan, Expression,
        OrderTerm, RowMode, RowTemplate, Window, WindowKind,
    };
    use crate::expression::{compile, Context};
    use crate::function_invocation::NoFunctions;
    use crate::table::{Column, TableSchema};
    use crate::value::{ColumnType, Value};
    use std::cell::Cell;

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

    fn assign(column: usize, expression: Expression) -> Assignment {
        Assignment {
            column,
            expression,
            path: format!("columns.C{column}.derivation"),
        }
    }

    fn plan() -> crate::dataset::ResolvedPlan {
        let prior = Binding {
            name: "P".into(),
            read: Read::Column(2),
        };
        let predicate = BoundPredicate::new(
            compile(
                "P IS NULL",
                Context::Predicate,
                "columns.C3.derivation.case[0].when",
            )
            .unwrap(),
            vec![prior.clone()],
        )
        .unwrap();
        let sum = BoundNumeric::new(
            compile(
                "P + X",
                Context::Derivation,
                "columns.C3.derivation.case[1].otherwise",
            )
            .unwrap(),
            vec![
                prior,
                Binding {
                    name: "X".into(),
                    read: Read::Column(1),
                },
            ],
        )
        .unwrap();
        DatasetPlan::new(
            schema(&["ID", "X"]),
            schema(&["ID", "X", "P", "STATE"]),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                filter: None,
            }],
            vec![
                assign(
                    2,
                    Expression::Window(Window {
                        kind: WindowKind::RowValue {
                            column: 3,
                            offset: -1,
                        },
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 0,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                ),
                assign(
                    3,
                    Expression::Case(vec![
                        CaseBranch {
                            when: Some(predicate),
                            assignment: Box::new(assign(3, Expression::Column(1))),
                        },
                        CaseBranch {
                            when: None,
                            assignment: Box::new(assign(3, Expression::Compute(sum))),
                        },
                    ]),
                ),
            ],
            vec![0],
            vec![],
        )
        .with_recurrences(vec![Recurrence {
            columns: vec![2, 3],
            window_path: "columns.C2.derivation".into(),
        }])
        .resolve()
        .unwrap()
    }

    fn table(values: &[i64]) -> Dataset {
        Dataset::from_rows(
            schema(&["ID", "X"]),
            values
                .iter()
                .enumerate()
                .map(|(i, &x)| vec![Value::Int(i as i64 + 1), Value::Int(x)])
                .collect(),
        )
    }

    #[test]
    fn a_cancelled_feedback_attempt_leaves_no_state_for_a_later_run() {
        let plan = plan();
        let polls = Cell::new(0);
        let cancelled = plan.execute(
            &table(&[1; 64]),
            &[],
            &mut NoFunctions::new(),
            None,
            &|| {
                polls.set(polls.get() + 1);
                polls.get() == 32
            },
        );
        assert!(matches!(
            *cancelled.result.unwrap_err(),
            crate::dataset::ExecutionError::Cancelled
        ));
        assert_eq!(
            (
                cancelled.handler_counts,
                cancelled.retained_verifications,
                cancelled.template_rows
            ),
            (vec![], vec![], vec![])
        );
        let later = plan
            .execute(
                &table(&[2, 2, 2]),
                &[],
                &mut NoFunctions::new(),
                None,
                &|| false,
            )
            .result
            .unwrap();
        assert_eq!(
            later.dataset.rows(),
            &[
                vec![Value::Int(1), Value::Int(2), Value::Missing, Value::Int(2)],
                vec![Value::Int(2), Value::Int(2), Value::Int(2), Value::Int(4)],
                vec![Value::Int(3), Value::Int(2), Value::Int(4), Value::Int(6)],
            ],
            "the next attempt starts with no history"
        );
        let again = plan
            .execute(&table(&[7]), &[], &mut NoFunctions::new(), None, &|| false)
            .result
            .unwrap();
        assert_eq!(
            again.dataset.rows(),
            &[vec![
                Value::Int(1),
                Value::Int(7),
                Value::Missing,
                Value::Int(7)
            ]],
            "a smaller input starts independently too"
        );
    }

    #[test]
    fn a_feedback_step_is_global_even_when_a_partition_keeps_all_its_keys() {
        let plan = plan();
        assert_eq!(
            plan.whole_table_step(1),
            Some(crate::dataset::resolved::Step::Column(
                crate::dataset::resolved::ColumnId::try_from(2usize).unwrap()
            ))
        );
    }
}
