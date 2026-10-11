//! Derive template-local prewindow and window-dependent stages from admitted reads.
use super::*;
use crate::bound_expression::Read;
fn read_deferred(read: Read, deferred: &[bool], intermediates: &[Intermediate]) -> bool {
    match read {
        Read::Column(column) => deferred[column],
        Read::Intermediate { index, .. } => intermediates
            .get(index)
            .is_some_and(|item| item.output_dependencies().any(|column| deferred[column])),
        Read::Source(_) => false,
    }
}
fn expression_deferred(
    expression: &Expression,
    deferred: &[bool],
    intermediates: &[Intermediate],
) -> bool {
    match expression {
        Expression::Window(_) => true,
        Expression::Column(column) => deferred[*column],
        Expression::Compute(plan) => plan.bindings().iter().any(|binding| read_deferred(binding.read, deferred, intermediates)),
        Expression::Flag(plan) => plan.bindings().iter().any(|binding| matches!(&binding.read, FlagRead::Read(read) if read_deferred(*read,deferred,intermediates))),
        Expression::Function(function) => function.arguments().iter().any(|argument| matches!(&argument.input, FunctionInput::Read(read) if read_deferred(*read,deferred,intermediates))),
        Expression::ProjectFunction(function) => function.arguments().iter().any(|argument| matches!(&argument.input, FunctionInput::Read(read) if read_deferred(*read,deferred,intermediates))),
        Expression::Case(branches) => branches.iter().any(|branch| branch.when.as_ref().is_some_and(|predicate| predicate.bindings().iter().any(|binding| read_deferred(binding.read,deferred,intermediates))) || expression_deferred(&branch.assignment.expression,deferred,intermediates)),
        Expression::Extreme {operands,..} | Expression::StrConcat {operands,..} | Expression::StrTemplate {operands,..} | Expression::Imputation {operands,..} | Expression::Temporal {operands,..} => operands.iter().any(|operand| expression_deferred(&operand.expression,deferred,intermediates)),
        Expression::StrContains {source,..} | Expression::StrExtract {source,..} | Expression::StrCase {source,..} | Expression::StrPad {source,..} | Expression::Round {source,..} => expression_deferred(&source.expression,deferred,intermediates),
        Expression::SourceBinding { source, .. } => expression_deferred(&source.expression, deferred, intermediates),
        Expression::FirstAvailable(selection) => selection.sources().iter().any(|operand| match &operand.read {
            SelectionRead::Column(column) => deferred[*column],
            SelectionRead::Expression(source) => expression_deferred(&source.expression, deferred, intermediates),
            SelectionRead::Source(_) | SelectionRead::Collect { .. } => false,
        }),
        Expression::SourceLookup { lookup, .. } => lookup.keys.iter().any(|key| deferred[key.output_column]),
        Expression::Intermediate {index,..} => intermediates.get(*index).is_some_and(|item| item.output_dependencies().any(|column| deferred[column])),
        Expression::Lookup(lookup) => lookup.keys.iter().any(|key| deferred[key.output_column]),
        Expression::Literal(_) | Expression::Source(_) | Expression::Collect {..} | Expression::RowLookup(_) | Expression::Reduce {..} | Expression::Count {..} => false,
    }
}
/// Dependency order is already admitted; a window and every transitive reader
/// complete over all retained rows after the template's prewindow filter.
pub fn window_stages(
    assignments: &[Assignment],
    width: usize,
    intermediates: &[Intermediate],
) -> Vec<bool> {
    let mut deferred = vec![false; width];
    for assignment in assignments {
        deferred[assignment.column] =
            expression_deferred(&assignment.expression, &deferred, intermediates);
    }
    deferred
}
