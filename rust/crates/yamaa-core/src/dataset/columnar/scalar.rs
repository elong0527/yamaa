//! The scalar operation families over buffers: each operand is evaluated
//! once over the selection, then the family's own operation runs over each
//! row's values.
use super::buffer::{Assembly, Values};
use super::eval::owned;
use super::{Cx, Fail, Run};
use crate::conversion::convert;
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{BindingId, ExprId, Op, OperandRole};
use crate::dataset::{ImputationStep, TemporalStep};
use crate::numeric_lifecycle::HandlerKind;
use crate::table::ValueRef;
use crate::value::Value;

/// Convert each completed result to its column's type, recovering through
/// the binding's declared replacement and counting it.
pub(super) fn finish<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    binding: BindingId,
    raw: Values,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let bound = plan.binding(binding);
    let region = plan.region(cx.region);
    let local = cx.local(plan, bound.column);
    // An untyped donor result keeps its raw value.
    if let crate::dataset::resolved::RegionKind::Donor { typed_columns, .. } = &region.kind {
        if !typed_columns.contains(&bound.column) {
            return Ok(raw);
        }
    }
    let column = &region.output.columns()[local];
    let kind = column.kind;
    // Values that already have the column's type convert to themselves.
    if raw.has_type(kind) {
        return Ok(raw);
    }
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        let value = raw.get(index);
        if fits(value, kind) {
            values.push(value);
            continue;
        }
        let owned_value = owned(value);
        match convert(&owned_value, kind) {
            Ok(converted) => values.push_value(converted),
            Err(error) => values.push_value(recover(run, cx, binding, error, row)?),
        }
    }
    Ok(values)
}

/// A value that already has the column's type converts to itself.
fn fits(value: ValueRef<'_>, kind: crate::value::ColumnType) -> bool {
    use crate::value::ColumnType as K;
    matches!(
        (value, kind),
        (ValueRef::Missing, _)
            | (ValueRef::Str(_), K::Str)
            | (ValueRef::Int(_), K::Int)
            | (ValueRef::Float(_), K::Float)
            | (ValueRef::Date(_), K::Date)
            | (ValueRef::DateTime(_), K::DateTime)
    )
}

/// Recover one failed conversion through the declared replacement.
fn recover<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    binding: BindingId,
    original: crate::conversion::ConversionError,
    row: usize,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let bound = plan.binding(binding);
    let region = plan.region(cx.region);
    let local = cx.local(plan, bound.column);
    let column = &region.output.columns()[local];
    let column_path = format!("columns.{}", column.name);
    let Some(handler) = &bound.handler else {
        return Err(Box::new(ExecutionError::Conversion {
            path: column_path,
            output_row: row,
            error: original,
            identity: run.failure_identity(cx, row),
        }));
    };
    if let Some(site) = bound.unconvertible {
        run.fire(site)?;
    }
    convert(&handler.value, column.kind).map_err(|replacement| {
        Box::new(ExecutionError::Conversion {
            path: handler.spec_path.clone(),
            output_row: row,
            error: replacement,
            identity: run.failure_identity(cx, row),
        })
    })
}

/// The node's operand of the value role at a position.
fn value_operand<E>(run: &Run<'_, '_, E>, expr: ExprId, position: usize) -> ExprId {
    run.operand(expr, OperandRole::Value, position)
}

/// Native mapping/cut selection with reached handlers and retained failure
/// identity. A missing source is counted under `missing` whether its handler
/// is declared or its default substitution gives missing.
pub(super) fn mapping_cut<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
    retained: bool,
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Op::MappingCut {
        operation,
        column,
        handled,
    } = &plan.expr(expr).op
    else {
        unreachable!("a mapping/cut node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        let selected = match operation.select(source.get(index)) {
            Ok(selected) => selected,
            Err(diagnostic) => return Err(run.retained_diagnostic(cx, diagnostic, true, row)),
        };
        let result = match selected.value {
            crate::mapping_cut::SelectedValue::Literal(value) => value.clone(),
            crate::mapping_cut::SelectedValue::Label(text) => Value::Str(text.into()),
            crate::mapping_cut::SelectedValue::Missing => Value::Missing,
        };
        if let Some(handler) = selected.handler {
            let kind = match handler {
                "missing" => HandlerKind::Missing,
                "unmapped" => HandlerKind::Unmapped,
                _ => unreachable!("closed mapping/cut handler"),
            };
            run.fire_node(expr, kind, || format!("{}.{}", operation.path(), handler))?;
            // A reached literal has no eager conversion. At the completed result
            // site, retain its original handler alongside a fatal conversion context.
            if retained && !handled {
                let output = &plan.region(cx.region).output.columns()[*column];
                if let Err(error) = convert(&result, output.kind) {
                    let mut diagnostic = error.into_diagnostic(format!("columns.{}", output.name));
                    diagnostic
                        .spec_paths
                        .push(format!("{}.{}", operation.path(), handler));
                    diagnostic.context.insert(
                        "variable".into(),
                        crate::json::Value::String(operation.source().into()),
                    );
                    diagnostic
                        .context
                        .insert("handler".into(), crate::json::Value::String(handler.into()));
                    return Err(run.retained_diagnostic(cx, diagnostic, true, row));
                }
            }
        }
        values.push_value(result);
    }
    Ok(values)
}

/// Declarative rounding over one operand.
pub(super) fn round<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let Op::Round(rounding) = &run.plan.expr(expr).op else {
        unreachable!("a rounding node holds its rounding")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for index in 0..rows.len() {
        let value = rounding
            .apply(source.get(index))
            .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)))?;
        values.push_value(value);
    }
    Ok(values)
}

/// The larger or smaller of the present operands; every operand is read.
pub(super) fn extreme<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let Op::Extreme { largest } = run.plan.expr(expr).op else {
        unreachable!("an extreme node holds its direction")
    };
    let count = run.plan.operands(expr).len();
    let mut operands = Vec::with_capacity(count);
    for position in 0..count {
        let operand = run.plan.operands(expr)[position].expr;
        operands.push(run.eval(cx, operand, rows, false)?);
    }
    let mut values = Values::missing(0);
    for index in 0..rows.len() {
        let mut selected = Value::Missing;
        for operand in &operands {
            let value = operand.value(index);
            if crate::scalar_language::replace_extreme(&selected, &value, largest)
                .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?
            {
                selected = value;
            }
        }
        values.push_value(selected);
    }
    Ok(values)
}

/// Ordered placeholder reads: the first missing placeholder answers the
/// declared replacement, counted, or fails; later placeholders are not read.
pub(super) fn str_template<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_template::{Part, StringTemplate};
    let plan = run.plan;
    let Op::StrTemplate(operation) = &plan.expr(expr).op else {
        unreachable!("a template node holds its template")
    };
    let mut parts: Vec<Vec<Value>> = vec![Vec::new(); rows.len()];
    let mut done = Assembly::new(rows.len());
    let mut remaining: Vec<usize> = (0..rows.len()).collect();
    for part in operation.parts() {
        let Part::Placeholder(index) = part else {
            continue;
        };
        if remaining.is_empty() {
            break;
        }
        let operand = value_operand(run, expr, *index);
        let subset: Vec<usize> = remaining.iter().map(|&position| rows[position]).collect();
        let values = run.eval(cx, operand, &subset, false)?;
        let mut answered = Vec::new();
        let mut rest = Vec::new();
        for (offset, &position) in remaining.iter().enumerate() {
            let value = values.get(offset);
            let row = rows[position];
            if matches!(value, ValueRef::Missing) {
                if operation.missing().is_some() {
                    run.fire_node(expr, HandlerKind::Missing, || {
                        format!("{}.missing", operation.path())
                    })?;
                    answered.push(position);
                    continue;
                }
                return Err(run.retained_diagnostic(
                    cx,
                    operation
                        .missing_diagnostic(*index)
                        .expect("admitted index"),
                    true,
                    row,
                ));
            }
            if let Some(actual) = StringTemplate::input_kind(value) {
                return Err(run.retained_diagnostic(
                    cx,
                    operation
                        .input_diagnostic(*index, actual)
                        .expect("admitted index"),
                    false,
                    row,
                ));
            }
            parts[position].push(owned(value));
            rest.push(position);
        }
        if !answered.is_empty() {
            let replacement = operation.missing().expect("declared replacement");
            done.part(
                &answered,
                Values::repeat(ValueRef::from(replacement), answered.len()),
            );
        }
        remaining = rest;
    }
    let mut rendered = Values::missing(0);
    for &position in &remaining {
        let value = operation.materialize(&parts[position]).map_err(|error| {
            Box::new(match error {
                crate::text_template::MaterializeError::Shape => ExecutionError::SchemaMismatch,
                crate::text_template::MaterializeError::Limit(limit) => {
                    ExecutionError::text(limit, operation.path())
                }
                crate::text_template::MaterializeError::Allocation => ExecutionError::Allocation,
            })
        })?;
        rendered.push(ValueRef::Str(&value));
    }
    if !remaining.is_empty() {
        done.part(&remaining, rendered);
    }
    Ok(done.finish())
}

/// Extraction: a missing source is counted under `missing`, declared or by
/// default; a declared `no_match` answers an unmatched value, counted.
pub(super) fn str_extract<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_extract::{Error, Step};
    let plan = run.plan;
    let Op::StrExtract(operation) = &plan.expr(expr).op else {
        unreachable!("an extraction node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        match operation.apply(source.get(index)) {
            Ok(Step::Capture(Some(text))) => values.push(ValueRef::Str(text)),
            Ok(Step::Capture(None)) => values.push(ValueRef::Missing),
            Ok(Step::Recovered(name)) => {
                let kind = if name == "missing" {
                    HandlerKind::Missing
                } else {
                    HandlerKind::NoMatch
                };
                run.fire_node(expr, kind, || format!("{}.{}", operation.path(), name))?;
                values.push_value(operation.substitute(name));
            }
            Ok(step @ Step::NoMatch(_)) => {
                let diagnostic = operation
                    .failure_diagnostic(step)
                    .expect("admitted complete context");
                return Err(run.retained_diagnostic(cx, diagnostic, true, row));
            }
            Err(Error::Diagnostic(diagnostic)) => {
                return Err(run.retained_diagnostic(cx, diagnostic, false, row))
            }
        }
    }
    Ok(values)
}

/// Replacement: a missing source is counted under `missing`, declared or by default.
pub(super) fn str_replace<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_replace::{Error, RenderError, Step};
    let plan = run.plan;
    let Op::StrReplace(operation) = &plan.expr(expr).op else {
        unreachable!("a replacement node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        match operation.apply(source.get(index)) {
            Ok(Step::Matched { subject, spans }) => {
                let text = operation.render(subject, &spans).map_err(|error| {
                    Box::new(match error {
                        RenderError::Limit(limit) => ExecutionError::text(limit, operation.path()),
                        RenderError::Allocation => ExecutionError::Allocation,
                    })
                })?;
                values.push(ValueRef::Str(&text));
            }
            Ok(Step::Recovered) => {
                run.fire_node(expr, HandlerKind::Missing, || {
                    format!("{}.missing", operation.path())
                })?;
                values.push_value(operation.missing_substitute());
            }
            Err(Error::Diagnostic(diagnostic)) => {
                return Err(run.retained_diagnostic(cx, diagnostic, false, row))
            }
        }
    }
    Ok(values)
}

/// Case substitution: a missing source is counted under `missing`.
pub(super) fn str_case<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_case::{Error, Step};
    let plan = run.plan;
    let Op::StrCase(operation) = &plan.expr(expr).op else {
        unreachable!("a case-substitution node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        match operation.apply(source.get(index)) {
            Ok(Step::Value(value)) => values.push_value(value),
            Ok(Step::Recovered) => {
                run.fire_node(expr, HandlerKind::Missing, || {
                    format!("{}.missing", operation.path())
                })?;
                values.push_value(operation.missing_substitute());
            }
            // Only a source of another type fails, a validation finding without keys.
            Err(Error::Diagnostic(diagnostic)) => {
                return Err(run.retained_diagnostic(cx, diagnostic, false, row))
            }
        }
    }
    Ok(values)
}

/// A piece by scalar position: a missing source is counted under `missing`,
/// declared or by default; a start past the text's end gives missing,
/// uncounted.
pub(super) fn str_substr<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_position::{Error, Step};
    let plan = run.plan;
    let Op::StrSubstr(operation) = &plan.expr(expr).op else {
        unreachable!("a substring node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        match operation.apply(source.get(index)) {
            Ok(Step::Value(value)) => values.push_value(value),
            Ok(Step::Recovered) => {
                run.fire_node(expr, HandlerKind::Missing, || {
                    format!("{}.missing", operation.path())
                })?;
                values.push_value(operation.missing_substitute());
            }
            // Only a source of another type fails, a validation finding without keys.
            Err(Error::Diagnostic(diagnostic)) => {
                return Err(run.retained_diagnostic(cx, diagnostic, false, row))
            }
        }
    }
    Ok(values)
}

/// A length in scalars: a missing source is counted under `missing`,
/// declared or by default.
pub(super) fn str_length<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_position::{Error, Step};
    let plan = run.plan;
    let Op::StrLength(operation) = &plan.expr(expr).op else {
        unreachable!("a length node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        match operation.apply(source.get(index)) {
            Ok(Step::Value(value)) => values.push_value(value),
            Ok(Step::Recovered) => {
                run.fire_node(expr, HandlerKind::Missing, || {
                    format!("{}.missing", operation.path())
                })?;
                values.push_value(operation.missing_substitute());
            }
            // Only a source of another type fails, a validation finding without keys.
            Err(Error::Diagnostic(diagnostic)) => {
                return Err(run.retained_diagnostic(cx, diagnostic, false, row))
            }
        }
    }
    Ok(values)
}

/// Padding: one read, shared canonical conversion and an admitted copy.
pub(super) fn str_pad<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Op::StrPad(operation) = &plan.expr(expr).op else {
        unreachable!("a padding node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        let value = source.value(index);
        values.push_value(pad(run, cx, operation, operation.path(), &value, row)?);
    }
    Ok(values)
}

/// Render one padded value; a value that cannot be text fails with the row's keys.
pub(super) fn pad<E>(
    run: &Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    operation: &crate::dataset::StringPad,
    path: &str,
    value: &Value,
    row: usize,
) -> Result<Value, Fail<E>> {
    let prepared = operation.prepare_text(value).map_err(|error| {
        run.retained_diagnostic(cx, error.into_diagnostic(path.into()), true, row)
    })?;
    // The bound holds for intermediate text too; a saved file's own byte
    // boundary belongs to its writer.
    prepared
        .render()
        .map_err(|limit| Box::new(ExecutionError::text(limit, path)))
}

/// Fixed-decimal text: a missing source is counted under `missing`, and
/// text longer than a text value may hold fails the run as padding's does.
pub(super) fn str_decimal<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    use crate::text_decimal::{Error, Step};
    let plan = run.plan;
    let Op::StrDecimal(operation) = &plan.expr(expr).op else {
        unreachable!("a decimal-text node holds its operation")
    };
    let source = run.eval(cx, value_operand(run, expr, 0), rows, false)?;
    let mut values = Values::missing(0);
    for (index, &row) in rows.iter().enumerate() {
        match operation.apply(source.get(index)) {
            Ok(Step::Value(value)) => values.push_value(value),
            Ok(Step::Recovered) => {
                run.fire_node(expr, HandlerKind::Missing, || {
                    format!("{}.missing", operation.path())
                })?;
                values.push_value(operation.missing_substitute());
            }
            // A source of another type fails, a validation finding without keys.
            Err(Error::Diagnostic(diagnostic)) => {
                return Err(run.retained_diagnostic(cx, diagnostic, false, row))
            }
            Err(Error::Limit(limit)) => {
                return Err(Box::new(ExecutionError::text(limit, operation.path())))
            }
        }
    }
    Ok(values)
}

/// Imputation reads its bounds and fallback only for the rows whose source
/// needs them, and counts default substitutions and crossed bounds like
/// declared handlers.
pub(super) fn imputation<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Op::Imputation(operation) = &plan.expr(expr).op else {
        unreachable!("an imputation node holds its operation")
    };
    let operands: Vec<ExprId> = plan
        .operands(expr)
        .iter()
        .map(|operand| operand.expr)
        .collect();
    let fail = |run: &Run<'_, '_, E>, diagnostic: crate::diagnostic::Diagnostic, row: usize| {
        let retain_keys = diagnostic.definition().phase == "impute";
        run.retained_diagnostic(cx, diagnostic, retain_keys, row)
    };
    let source = run.eval(cx, operands[0], rows, false)?;
    let mut assembly = Assembly::new(rows.len());
    let mut fallback = Vec::new();
    let mut bounded = Vec::new();
    let mut direct = Values::missing(0);
    let mut direct_positions = Vec::new();
    for (index, &row) in rows.iter().enumerate() {
        match operation.source_value(source.get(index)) {
            Ok(ImputationStep::Value(value)) => {
                direct_positions.push(index);
                direct.push_value(value);
            }
            Ok(ImputationStep::Recovered { handler }) => {
                run.fire_node(expr, HandlerKind::Invalid, || {
                    format!("{}.{handler}", operation.path())
                })?;
                direct_positions.push(index);
                direct.push_value(
                    operation
                        .handler(handler)
                        .expect("declared recovery")
                        .clone(),
                );
            }
            Ok(ImputationStep::Missing) => match operation.fallback() {
                Some(_) => {
                    run.fire_node(expr, HandlerKind::Missing, || {
                        format!("{}.fallback", operation.path())
                    })?;
                    fallback.push(index);
                }
                None => {
                    run.fire_node(expr, HandlerKind::Missing, || {
                        format!("{}.missing", operation.path())
                    })?;
                    direct_positions.push(index);
                    direct.push(ValueRef::Missing);
                }
            },
            Ok(ImputationStep::Partial) => {
                run.fire_node(expr, HandlerKind::Partial, || {
                    format!("{}.partial", operation.path())
                })?;
                direct_positions.push(index);
                direct.push(ValueRef::Missing);
            }
            Ok(ImputationStep::Bound(pending)) => bounded.push((index, pending)),
            Err(diagnostic) => return Err(fail(run, diagnostic, row)),
        }
    }
    if !direct_positions.is_empty() {
        assembly.part(&direct_positions, direct);
    }
    if !fallback.is_empty() {
        let last = *operands.last().expect("declared fallback operand");
        let subset: Vec<usize> = fallback.iter().map(|&position| rows[position]).collect();
        let values = run.eval(cx, last, &subset, false)?;
        let mut answered = Values::missing(0);
        for (offset, &position) in fallback.iter().enumerate() {
            let value = operation
                .fallback_value(values.get(offset))
                .map_err(|diagnostic| fail(run, diagnostic, rows[position]))?;
            answered.push_value(value);
        }
        assembly.part(&fallback, answered);
    }
    if !bounded.is_empty() {
        let positions: Vec<usize> = bounded.iter().map(|(position, _)| *position).collect();
        let subset: Vec<usize> = positions.iter().map(|&position| rows[position]).collect();
        let mut not_before = Values::missing(subset.len());
        let mut not_after = Values::missing(subset.len());
        for ((field, _), &operand) in operation.bounds().zip(&operands[1..]) {
            let values = run.eval(cx, operand, &subset, false)?;
            match field {
                "not_before" => not_before = values,
                _ => not_after = values,
            }
        }
        let mut answered = Values::missing(0);
        for (offset, (position, pending)) in bounded.into_iter().enumerate() {
            match operation.apply_bounds(pending, not_before.get(offset), not_after.get(offset)) {
                Ok((value, crossed)) => {
                    if crossed {
                        run.fire_node(expr, HandlerKind::CrossedBounds, || {
                            format!("{}.not_after", operation.path())
                        })?;
                    }
                    answered.push_value(value);
                }
                Err(diagnostic) => return Err(fail(run, diagnostic, rows[position])),
            }
        }
        assembly.part(&positions, answered);
    }
    Ok(assembly.finish())
}

/// One temporal operation: `dtc` reads its parts in order and stops at the
/// first missing one, so a later part, its lookups and its handlers are
/// never reached (TIME-19).
pub(super) fn temporal<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Op::Temporal(operation) = &plan.expr(expr).op else {
        unreachable!("a temporal node holds its operation")
    };
    let operands: Vec<ExprId> = plan
        .operands(expr)
        .iter()
        .map(|operand| operand.expr)
        .collect();
    let stops = operation.kind().stops_at_missing();
    let mut values: Vec<Vec<Value>> = vec![Vec::with_capacity(operands.len()); rows.len()];
    let mut reading: Vec<usize> = (0..rows.len()).collect();
    for (index, &operand) in operands.iter().enumerate() {
        if reading.is_empty() {
            break;
        }
        let subset: Vec<usize> = reading.iter().map(|&position| rows[position]).collect();
        let read = run.eval(cx, operand, &subset, false)?;
        let mut next = Vec::new();
        for (offset, &position) in reading.iter().enumerate() {
            let value = read.value(offset);
            operation
                .validate_operand(index, &value)
                .expect("admitted operand index")
                .map_err(|diagnostic| Box::new(ExecutionError::ScalarDiagnostic(diagnostic)))?;
            let last = stops && value == Value::Missing;
            values[position].push(value);
            if !last {
                next.push(position);
            }
        }
        reading = next;
    }
    let mut result = Values::missing(0);
    for (position, &row) in rows.iter().enumerate() {
        let parts = &mut values[position];
        // A part left unread stands missing; the operation reads none after it.
        parts.resize(operands.len(), Value::Missing);
        let step = operation
            .apply_values(parts)
            .expect("admitted temporal arity")
            .map_err(|diagnostic| {
                if diagnostic.definition().phase == "validation" {
                    Box::new(ExecutionError::ScalarDiagnostic(diagnostic))
                } else {
                    run.retained_diagnostic(cx, diagnostic, true, row)
                }
            })?;
        match step {
            TemporalStep::Value(value) => result.push_value(value),
            TemporalStep::Invalid => {
                run.fire_node(expr, HandlerKind::Invalid, || {
                    format!("{}.invalid", operation.path())
                })?;
                result.push_value(
                    operation
                        .invalid_handler()
                        .expect("declared recovery")
                        .clone(),
                );
            }
        }
    }
    Ok(result)
}

/// Distinct present raw readings across the records feeding each row,
/// before conversion; a declared selection chooses among several, counted.
pub(super) fn collect<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let node = plan.expr(expr);
    let Op::Collect {
        identifier,
        selection,
    } = &node.op
    else {
        unreachable!("a collection node holds its identity")
    };
    let column = field_of(plan.reads(expr)[0]);
    let filter = plan
        .operands(expr)
        .iter()
        .find(|operand| operand.role == OperandRole::Filter)
        .map(|operand| operand.expr);
    let mut values = Values::missing(0);
    for &row in rows {
        let value = super::predicates::collect_row(
            run,
            cx,
            super::predicates::Collection {
                column,
                identifier,
                filter,
                selection: selection.as_ref(),
                path: &node.path,
                site: Some(expr),
            },
            row,
        )?;
        values.push(ValueRef::from(&value));
    }
    Ok(values)
}

/// The field a record read addresses: of the driver record, or of the
/// donor record an aggregate derivation enriches.
fn field_of(read: crate::dataset::resolved::Read) -> usize {
    match read {
        crate::dataset::resolved::Read::Field(field)
        | crate::dataset::resolved::Read::Relation(_, field) => field as usize,
        _ => unreachable!("a record read addresses a field"),
    }
}
