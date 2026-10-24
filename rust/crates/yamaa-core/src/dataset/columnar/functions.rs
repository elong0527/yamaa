//! Host callbacks: every signature admitted before any table is read, then
//! one invocation per row with its arguments resolved in authored order.
use super::buffer::Values;
use super::relations::Site;
use super::{Cx, Fail, Run};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, Op, ResolvedPlan};
use crate::dataset::{FunctionArgument, FunctionInput};
use crate::function_invocation::FunctionPort;
use crate::value::Value;
use std::collections::BTreeMap;

/// Check every callback site, including unselected branches and donor
/// plans, before the first table observation.
pub(super) fn admit<E>(
    plan: &ResolvedPlan,
    functions: &dyn FunctionPort<Error = E>,
) -> Result<(), Fail<E>> {
    for slot in plan.functions() {
        if functions.project_signature(slot.slot) != Some(&slot.signature) {
            return Err(Box::new(ExecutionError::FunctionBinding {
                slot: slot.slot,
            }));
        }
    }
    Ok(())
}

/// One callback node over rows, invoked row by row. The run polls its
/// cancellation before each such batch of calls (#2822): a unit with
/// callbacks runs in row mode, so between the calls of successive rows.
pub(super) fn evaluate<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    run.stop_if_cancelled()?;
    let plan = run.plan;
    let node = plan.expr(expr);
    let mut values = Values::missing(0);
    for &row in rows {
        let value = match &node.op {
            Op::ProjectFunction(function) => {
                let supplied = supplied(run, cx, expr, function.arguments(), row)?;
                match crate::function_invocation::invoke_project_at(
                    function.signature(),
                    &supplied,
                    function.slot(),
                    &mut *run.functions,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(Box::new(ExecutionError::ProjectFunction {
                            path: node.path.to_string(),
                            identity: run.failure_identity(cx, row),
                            error,
                        }))
                    }
                }
            }
            _ => unreachable!("a callback node holds its invocation"),
        };
        values.push_value(value);
    }
    Ok(values)
}

/// Resolve every supplied argument in authored order.
fn supplied<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    arguments: &[FunctionArgument],
    row: usize,
) -> Result<BTreeMap<String, Value>, Fail<E>> {
    let plan = run.plan;
    let path = plan.expr(expr).path.to_string();
    let mut supplied = BTreeMap::new();
    let mut read_index = 0;
    for argument in arguments {
        let value = match &argument.input {
            FunctionInput::Literal(value) => value.clone(),
            FunctionInput::Collect { column, identifier } => {
                read_index += 1;
                super::predicates::collect_row(
                    run,
                    cx,
                    super::predicates::Collection {
                        column: *column,
                        identifier,
                        filter: None,
                        selection: None,
                        path: &path,
                        site: None,
                    },
                    row,
                )?
            }
            FunctionInput::Read(bound) => {
                let read = plan.reads(expr)[read_index];
                read_index += 1;
                super::predicates::resolve_read(
                    run,
                    cx,
                    Site::node(plan, expr),
                    (read, bound),
                    &path,
                    row,
                    super::predicates::Observe::Operand,
                )?
            }
        };
        supplied.insert(argument.name.clone(), value);
    }
    Ok(supplied)
}
