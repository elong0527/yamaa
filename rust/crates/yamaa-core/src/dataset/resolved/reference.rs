//! The reference order: a structural mirror of today's executor loops.
//!
//! [`reference_order`] follows `DatasetPlan::execute_admitted` and
//! [`donor_reference_order`] follows donor materialization in
//! `execution/intermediates`, step for step, over the bound plan alone. The
//! resolved schedule must equal it until the executor over the resolved plan
//! replaces both (#2843).
use crate::dataset::{read_requires_window, window_stages, Check, DatasetPlan, RowMode};

/// One step in the bound plan's own coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceStep {
    Admit,
    VerifyIntermediates { self_rows: bool },
    KeyGrain,
    Driver { template: usize },
    Row { template: usize, assignment: usize },
    Filter { template: usize },
    Window { template: usize, assignment: usize },
    PostwindowFilter { template: usize },
    PublishSelf { template: usize },
    Column { assignment: usize },
    ColumnCheck { column: usize, check: usize },
    DatasetCheck { verification: usize },
    UnobservedFilterTypes,
    OutputKeys,
    Order,
}

/// `execute_admitted`: intermediate verification, candidates (key grain or
/// every template in order), the column phase with its prefix checks, the
/// unobserved filter-type check, output keys, dataset checks and ordering.
pub(crate) fn reference_order(plan: &DatasetPlan) -> Vec<ReferenceStep> {
    use ReferenceStep as S;
    let width = plan.output().columns().len();
    let mut steps = vec![S::Admit, S::VerifyIntermediates { self_rows: false }];
    if matches!(plan.templates()[0].mode, RowMode::Keys) {
        // key_grain::construct: the source filter over every record before
        // any key lifecycle, then the key assignments row by row.
        if plan.templates()[0].filter.is_some() {
            steps.push(S::Filter { template: 0 });
        }
        for assignment in 0..plan.templates()[0].assignments.len() {
            steps.push(S::Row {
                template: 0,
                assignment,
            });
        }
        steps.push(S::KeyGrain);
    } else {
        // process_template, for each template in order.
        for (template, declared) in plan.templates().iter().enumerate() {
            steps.push(S::Driver { template });
            let deferred = window_stages(&declared.assignments, width, plan.intermediates());
            let postwindow = matches!(declared.mode.local_mode(), RowMode::Groups(_))
                && declared.filter.as_ref().is_some_and(|filter| {
                    filter.bindings().iter().any(|binding| {
                        read_requires_window(&binding.read, &deferred, plan.intermediates())
                    })
                });
            for (assignment, row) in declared.assignments.iter().enumerate() {
                if !deferred[row.column] {
                    steps.push(S::Row {
                        template,
                        assignment,
                    });
                }
            }
            if declared.filter.is_some() && !postwindow {
                steps.push(S::Filter { template });
            }
            for (assignment, row) in declared.assignments.iter().enumerate() {
                if deferred[row.column] {
                    steps.push(S::Window {
                        template,
                        assignment,
                    });
                }
            }
            if postwindow {
                steps.push(S::PostwindowFilter { template });
            }
            steps.push(S::PublishSelf { template });
            steps.push(S::VerifyIntermediates { self_rows: true });
        }
    }
    // complete_columns.
    let mut completed = vec![true; width];
    for template in plan.templates() {
        let mut present = vec![false; width];
        for assignment in &template.assignments {
            present[assignment.column] = true;
        }
        for (column, done) in completed.iter_mut().enumerate() {
            *done &= present[column];
        }
    }
    let mut next = 0;
    check_columns(plan, &completed, &mut next, &mut steps);
    for (assignment, column) in plan.columns().iter().enumerate() {
        steps.push(S::Column { assignment });
        completed[column.column] = true;
        check_columns(plan, &completed, &mut next, &mut steps);
    }
    steps.push(S::UnobservedFilterTypes);
    // verify_output.
    steps.push(S::OutputKeys);
    for verification in 0..plan.verifications().len() {
        steps.push(S::DatasetCheck { verification });
    }
    steps.push(S::Order);
    steps
}

/// `check_columns`: the available declared prefix, after every key.
fn check_columns(
    plan: &DatasetPlan,
    completed: &[bool],
    next: &mut usize,
    steps: &mut Vec<ReferenceStep>,
) {
    if plan.keys().iter().any(|&column| !completed[column]) {
        return;
    }
    while *next < completed.len() && completed[*next] {
        if let Some(group) = plan
            .column_verifications()
            .iter()
            .find(|group| group.column == *next)
        {
            if group
                .checks
                .iter()
                .any(|verification| match &verification.check {
                    Check::ValueCodelists { discriminator, .. }
                    | Check::ValueMaxLength { discriminator, .. } => !completed[*discriminator],
                    Check::Inventory {
                        scope: Some(scope), ..
                    } => !completed[scope.discriminator],
                    _ => false,
                })
            {
                return;
            }
            for check in 0..group.checks.len() {
                steps.push(ReferenceStep::ColumnCheck {
                    column: *next,
                    check,
                });
            }
        }
        *next += 1;
    }
}

/// Donor materialization: the stored fields copied row by row, then each
/// derivation over every donor row in declaration order.
pub(crate) fn donor_reference_order(plan: &DatasetPlan, stored: usize) -> Vec<ReferenceStep> {
    use ReferenceStep as S;
    let mut steps = vec![S::Driver { template: 0 }];
    if let Some(template) = plan.templates().first() {
        for column in 0..stored {
            if let Some(assignment) = template
                .assignments
                .iter()
                .position(|assignment| assignment.column == column)
            {
                steps.push(S::Row {
                    template: 0,
                    assignment,
                });
            }
        }
    }
    for assignment in 0..plan.columns().len() {
        steps.push(S::Column { assignment });
    }
    steps
}
