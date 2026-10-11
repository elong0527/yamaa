//! Resolve authored scalar arguments before the shared exact-type invocation lifecycle.
use super::*;
use crate::{dataset_predicate::Read, function_invocation::FunctionPort};

pub(super) struct Context<'a> {
    pub plan: &'a DatasetPlan,
    pub row: usize,
    pub assignment: &'a Assignment,
}

/// Resolve every supplied argument in authored order, then apply the shared signature rules.
pub(super) fn evaluate<T: TableAccess + ?Sized>(
    function: &BoundFunction,
    table: &T,
    candidate: &Candidate,
    context: Context<'_>,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
    bindings: &mut dyn FunctionPort<Error = T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let supplied = supplied(
        function.arguments(),
        table,
        candidate,
        &context,
        budget,
        handlers,
    )?;
    let result = crate::function_invocation::invoke_at(
        function.signature(),
        &supplied,
        function.slot(),
        bindings,
    );
    match result {
        Ok(value) => retain_result(value, budget),
        Err(error) => Err(Box::new(ExecutionError::Function {
            path: context.assignment.path.clone(),
            identity: failure_identity(candidate, context.plan.keys(), context.row, budget)?,
            error,
        })),
    }
}

pub(super) fn evaluate_project<T: TableAccess + ?Sized>(
    function: &BoundProjectFunction,
    table: &T,
    candidate: &Candidate,
    context: Context<'_>,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
    bindings: &mut dyn FunctionPort<Error = T::Error>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let supplied = supplied(
        function.arguments(),
        table,
        candidate,
        &context,
        budget,
        handlers,
    )?;
    let result = crate::function_invocation::invoke_project_at(
        function.signature(),
        &supplied,
        function.slot(),
        bindings,
    );
    match result {
        Ok(value) => retain_result(value, budget),
        Err(error) => Err(Box::new(ExecutionError::ProjectFunction {
            path: context.assignment.path.clone(),
            identity: failure_identity(candidate, context.plan.keys(), context.row, budget)?,
            error,
        })),
    }
}

fn supplied<T: TableAccess + ?Sized>(
    arguments: &[FunctionArgument],
    table: &T,
    candidate: &Candidate,
    context: &Context<'_>,
    budget: &mut Budget,
    handlers: &mut HandlerCounter,
) -> Result<BTreeMap<String, Value>, Box<ExecutionError<T::Error>>> {
    let mut supplied = BTreeMap::new();
    for argument in arguments {
        budget.work(1, 1)?;
        if let FunctionInput::Collect { column, identifier } = &argument.input {
            budget.work(candidate.members.len(), 1)?;
            let value = key_grain::collect_bound(
                table,
                key_grain::Collection {
                    column: *column,
                    identifier,
                    filter: None,
                    selection: None,
                },
                key_grain::CollectionContext {
                    path: &context.assignment.path,
                    candidate,
                    plan: context.plan,
                    row: context.row,
                },
                budget,
                handlers,
            )?;
            supplied.insert(argument.name.clone(), value);
            continue;
        }
        let value = match &argument.input {
            FunctionInput::Read(Read::Intermediate { .. }) => {
                unreachable!("function admission rejects intermediate reads")
            }
            FunctionInput::Collect { .. } => unreachable!("collected input was resolved above"),
            FunctionInput::Literal(value) => ValueRef::from(value),
            FunctionInput::Read(Read::Column(column)) => ValueRef::from(&candidate.values[*column]),
            FunctionInput::Read(Read::Source(column)) => {
                let source_row = candidate.members[0];
                table.cell(source_row, *column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: context.assignment.path.clone(),
                        source_row,
                        error,
                    })
                })?
            }
        };
        if let ValueRef::Str(text) = value {
            budget.scalar_text(text.len())?;
        }
        supplied.insert(argument.name.clone(), own(value));
    }
    // This charges a potential call; missing-value short circuit never executes host code.
    budget.work(1, 1)?;
    Ok(supplied)
}

fn retain_result<E>(value: Value, budget: &mut Budget) -> Result<Value, Box<ExecutionError<E>>> {
    if let Value::Str(text) = &value {
        budget.scalar_text(text.len())?;
    }
    Ok(value)
}

/// New source containers cannot hide a callback signature until source access.
pub(super) fn admit<E>(
    assignment: &Assignment,
    functions: &dyn FunctionPort<Error = E>,
) -> Result<(), Box<ExecutionError<E>>> {
    match &assignment.expression {
        Expression::Function(function) => {
            if functions.signature(function.slot()) != Some(function.signature()) {
                return Err(Box::new(ExecutionError::FunctionBinding {
                    slot: function.slot(),
                }));
            }
        }
        Expression::ProjectFunction(function) => {
            if functions.project_signature(function.slot()) != Some(function.signature()) {
                return Err(Box::new(ExecutionError::FunctionBinding {
                    slot: function.slot(),
                }));
            }
        }
        Expression::SourceBinding { source, .. } => admit(source, functions)?,
        Expression::FirstAvailable(selection) => {
            for operand in selection.sources() {
                if let yamaa_core::dataset::SelectionRead::Expression(source) = &operand.read {
                    admit(source, functions)?;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
