//! Column checks at their declared prefix, output keys, dataset checks and
//! presentation order, over the completed frame.
use super::frame::Frame;
use super::{Fail, Run, Table};
use crate::bound_expression::{Binding, BoundPredicate, Read as BoundRead};
use crate::dataset::execution::{
    CheckRecord, CodelistObservation, ExecutionError, ExecutionEvent, ExecutionPhase, RowIdentity,
};
use crate::dataset::resolved::{CheckId, CheckPayload, CheckScope, Read, RegionId, ResolvedPlan};
use crate::dataset::{Check, InventoryScope, RowScope, VerificationSeverity, WhereClause};
use crate::expression::{Condition, ErrorKind, Truth};
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::value::{ColumnType, Value};
use std::convert::Infallible;

/// The completed frame as a table of output rows.
pub(super) struct Completed<'f> {
    pub frame: &'f Frame,
    pub schema: &'f TableSchema,
}
impl TableAccess for Completed<'_> {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        self.frame.rows()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        if row < self.frame.rows() && column < self.frame.width() {
            Ok(self.frame.get(row, column))
        } else {
            Err(CellError::OutOfBounds { row, column })
        }
    }
}

fn selects(clause: &WhereClause, frame: &Frame, row: usize) -> bool {
    clause.terms.iter().all(|term| {
        term.selects(match frame.get(row, term.column) {
            ValueRef::Str(value) => Some(value),
            _ => None,
        })
    })
}

fn scope_selects(scope: &RowScope, frame: &Frame, row: usize) -> bool {
    scope
        .clauses
        .iter()
        .any(|clause| selects(clause, frame, row))
        == scope.include
}

fn inventory_selects(scope: &InventoryScope, frame: &Frame, row: usize) -> bool {
    scope
        .overrides
        .iter()
        .find(|(clause, _)| selects(clause, frame, row))
        .map_or(scope.default, |(_, enabled)| *enabled)
}

/// The output keys of the dataset region, as frame columns.
fn key_columns(plan: &ResolvedPlan) -> Vec<usize> {
    let first = plan
        .region_columns(RegionId::DATASET)
        .next()
        .map_or(0, |column| column.index());
    plan.keys().iter().map(|key| key.index() - first).collect()
}

/// A row's output keys once every key is complete.
fn failure_identity(frame: &Frame, keys: &[usize], row: usize) -> Option<RowIdentity> {
    if keys.iter().any(|&key| !frame.is_completed(row, key)) {
        return None;
    }
    Some(RowIdentity {
        position: row,
        values: keys.iter().map(|&key| frame.value(row, key)).collect(),
    })
}

/// Retain original output keys without substituting row-number identity.
fn identities(
    frame: &Frame,
    keys: &[usize],
    rows: impl IntoIterator<Item = usize>,
) -> Vec<RowIdentity> {
    rows.into_iter()
        .map(|position| RowIdentity {
            position,
            values: keys.iter().map(|&key| frame.value(position, key)).collect(),
        })
        .collect()
}

/// Check only the available declared prefix, after every key is complete.
pub(super) fn check_columns<E>(
    run: &mut Run<'_, '_, E>,
    frame: &Frame,
    completed: &[bool],
    next: &mut usize,
    records: &mut Vec<CheckRecord>,
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let keys = key_columns(plan);
    if keys.iter().any(|&column| !completed[column]) {
        return Ok(());
    }
    let first = plan
        .region_columns(RegionId::DATASET)
        .next()
        .map_or(0, |column| column.index());
    while *next < completed.len() && completed[*next] {
        let group: Vec<CheckId> = plan
            .checks()
            .iter()
            .enumerate()
            .filter(|(_, check)| matches!(check.scope, CheckScope::Column(column) if column.index() - first == *next))
            .map(|(index, _)| CheckId::try_from(index).expect("a check slot"))
            .collect();
        if !group.is_empty() {
            // A check reads the completed columns it selects its rows by: a
            // value-level check's where columns and a `not_missing`'s `when`
            // columns. A later one delays the declared check prefix until its
            // value is complete; it never silently selects from a partial row.
            if group.iter().any(|&check| {
                plan.check_reads(check).iter().any(|read| {
                    matches!(read, Read::Column(column) if !completed[column.index() - first])
                })
            }) {
                return Ok(());
            }
            let start = records.len();
            for &check in &group {
                column_check(plan, frame, &keys, check, *next, records)?;
            }
            if records[start..].iter().any(|record| {
                record.failed_count != 0
                    && severity(plan, &record.path) == VerificationSeverity::Error
            }) {
                return Err(Box::new(ExecutionError::VerificationFailures(
                    std::mem::take(records),
                )));
            }
        }
        *next += 1;
    }
    Ok(())
}

fn declared(plan: &ResolvedPlan, check: CheckId) -> &Check {
    match &plan.check(check).payload {
        CheckPayload::Declared(check) => check,
        _ => unreachable!("a column or dataset check holds its declaration"),
    }
}

/// The severity of the check at a path.
fn severity(plan: &ResolvedPlan, path: &str) -> VerificationSeverity {
    plan.checks()
        .iter()
        .find(|check| &*check.path == path)
        .map_or(VerificationSeverity::Error, |check| check.severity)
}

fn column_check<E>(
    plan: &ResolvedPlan,
    frame: &Frame,
    keys: &[usize],
    check: CheckId,
    column: usize,
    records: &mut Vec<CheckRecord>,
) -> Result<(), Fail<E>> {
    let node = plan.check(check);
    let path = node.path.to_string();
    let rows = frame.rows();
    match declared(plan, check) {
        Check::InvalidDiagnostic(diagnostic) => {
            return Err(Box::new(ExecutionError::VerificationDiagnostic {
                diagnostic: diagnostic.clone(),
                records: std::mem::take(records),
            }));
        }
        Check::InvalidDeclaration { code, reason } => {
            return Err(Box::new(ExecutionError::VerificationDeclaration {
                path,
                code: *code,
                reason: reason.clone(),
                records: std::mem::take(records),
            }));
        }
        Check::ValueEntries(entries) => {
            // A row two entries select has two definitions.
            let mut offending = Vec::new();
            let mut matched = Vec::new();
            for row in 0..rows {
                let ids: Vec<String> = entries
                    .iter()
                    .filter(|(_, clause)| selects(clause, frame, row))
                    .map(|(id, _)| id.clone())
                    .collect();
                if ids.len() > 1 {
                    offending.push(
                        failure_identity(frame, keys, row)
                            .expect("all keys are complete before column checks"),
                    );
                    matched.push(ids);
                }
            }
            records.push(CheckRecord {
                path,
                code: crate::diagnostic::ConditionCode::ValueLevelMultipleMatches,
                evaluated_count: rows,
                failed_count: offending.len(),
                output_rows: rows,
                offending_rows: offending,
                codelist: None,
                inventory: None,
                row_count: None,
                entries: Some(matched),
                denominator: None,
            });
        }
        Check::ValueEntryMatch { clause, .. } => {
            // The whole artifact is the one unit this check examines.
            let matched = (0..rows).any(|row| selects(clause, frame, row));
            records.push(CheckRecord {
                path,
                code: crate::diagnostic::ConditionCode::ValueLevelUnmatched,
                evaluated_count: 1,
                failed_count: usize::from(!matched),
                output_rows: rows,
                offending_rows: Vec::new(),
                codelist: None,
                inventory: None,
                row_count: None,
                entries: None,
                denominator: None,
            });
        }
        Check::NotMissing { when, max_fraction } => {
            let schema = &plan.region(RegionId::DATASET).output;
            let dataset = Completed { frame, schema };
            // `when` selects the rows checked: a `TRUE` row is checked, and a
            // `FALSE` or `UNKNOWN` one passes (CHECK-5, CHECK-12).
            let checked: Vec<usize> = match when {
                Some(when) => {
                    validate_predicate(plan, when, &dataset, records)?;
                    let mut checked = Vec::new();
                    for row in 0..rows {
                        if completed_truth(when, &dataset, row, records)? == Truth::True {
                            checked.push(row);
                        }
                    }
                    checked
                }
                None => (0..rows).collect(),
            };
            let missing: Vec<usize> = checked
                .iter()
                .copied()
                .filter(|&row| matches!(frame.get(row, column), ValueRef::Missing))
                .collect();
            // The share of the checked rows that are missing is compared as a
            // `row_count`'s share is, zero when no row is checked (CHECK-13);
            // a violated check offends at every one of those rows (D209).
            let violated = match max_fraction {
                Some(max) => crate::dataset::share(missing.len(), checked.len()) > *max,
                None => !missing.is_empty(),
            };
            let offending_rows: Vec<RowIdentity> = if violated {
                missing
                    .iter()
                    .map(|&row| {
                        failure_identity(frame, keys, row)
                            .expect("all keys are complete before column checks")
                    })
                    .collect()
            } else {
                Vec::new()
            };
            records.push(CheckRecord {
                path,
                code: crate::diagnostic::ConditionCode::VerificationNotMissingFailed,
                // Rows are its unit, every output row its total (CHECK-24).
                evaluated_count: rows,
                failed_count: offending_rows.len(),
                output_rows: rows,
                offending_rows,
                codelist: None,
                inventory: None,
                row_count: None,
                entries: None,
                denominator: max_fraction.map(|_| checked.len()),
            });
        }
        declared_check @ (Check::Mandatory(_)
        | Check::DataType { .. }
        | Check::AllowedValues(_)
        | Check::Codelist { .. }
        | Check::ValueCodelists { .. }
        | Check::Inventory { .. }
        | Check::ValueMaxLength { .. }
        | Check::Range { .. }
        | Check::MaxLength(_)
        | Check::WideMaxLength(_)
        | Check::Matches(_)) => {
            let bindings = match declared_check {
                Check::ValueCodelists { bindings, .. } => {
                    bindings.iter().map(Some).collect::<Vec<_>>()
                }
                _ => vec![None],
            };
            for binding in bindings {
                let positions: Vec<usize> = (0..rows)
                    .filter(|&row| match (declared_check, binding) {
                        (Check::ValueCodelists { overrides, .. }, Some(binding)) => {
                            binding
                                .clauses
                                .iter()
                                .any(|clause| selects(clause, frame, row))
                                || binding.default
                                    && !overrides.iter().any(|clause| selects(clause, frame, row))
                        }
                        (
                            Check::Inventory {
                                scope: Some(scope), ..
                            },
                            _,
                        ) => inventory_selects(scope, frame, row),
                        (Check::ValueMaxLength { scope, .. }, _)
                        | (Check::Mandatory(Some(scope)), _)
                        | (
                            Check::DataType {
                                scope: Some(scope), ..
                            },
                            _,
                        ) => scope_selects(scope, frame, row),
                        _ => true,
                    })
                    .collect();
                let values: Vec<Value> = positions
                    .iter()
                    .map(|&position| frame.value(position, column))
                    .collect();
                let offending = if let Some(binding) = binding {
                    values
                        .iter()
                        .enumerate()
                        .filter_map(|(position, value)| {
                            (!crate::dataset_checks::codelist_accepts(&binding.values, value))
                                .then_some(position)
                        })
                        .collect()
                } else if let Check::Matches(pattern) = declared_check {
                    crate::dataset_checks::matches_offenders(pattern, values.iter())
                } else {
                    crate::dataset_checks::column_offenders(declared_check, values.iter())
                };
                let offending: Vec<usize> = offending
                    .into_iter()
                    .map(|index| positions[index])
                    .collect();
                let code = crate::dataset_checks::column_code(declared_check)
                    .expect("admitted column check");
                let identifier = binding.map(|binding| binding.id.as_str()).or({
                    if let Check::Codelist { id, .. } = declared_check {
                        Some(id.as_str())
                    } else {
                        None
                    }
                });
                let codelist = identifier.map(|id| CodelistObservation {
                    id: id.into(),
                    values: offending
                        .iter()
                        .map(|&position| frame.value(position, column))
                        .collect(),
                });
                let inventory = matches!(declared_check, Check::Inventory { .. }).then(|| {
                    offending
                        .iter()
                        .map(|&position| frame.value(position, column))
                        .collect()
                });
                let offending_rows: Vec<RowIdentity> = offending
                    .iter()
                    .map(|&position| {
                        failure_identity(frame, keys, position)
                            .expect("all keys are complete before column checks")
                    })
                    .collect();
                records.push(CheckRecord {
                    path: path.clone(),
                    code,
                    evaluated_count: positions.len(),
                    failed_count: offending_rows.len(),
                    output_rows: rows,
                    codelist,
                    inventory,
                    row_count: None,
                    offending_rows,
                    entries: None,
                    denominator: None,
                });
            }
        }
        _ => unreachable!("column check vocabulary is admitted in core"),
    }
    Ok(())
}

/// Check output key completeness and uniqueness, then run every declared
/// dataset check over the completed rows.
pub(super) fn verify_output<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    frame: &Frame,
    secondary: &[Table<'s, E>],
    records: &mut Vec<CheckRecord>,
    observer: &mut dyn FnMut(ExecutionEvent),
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    observer(ExecutionEvent::Exit {
        phase: ExecutionPhase::Derivation,
        outcome: crate::application_observer::Outcome::Succeeded,
    });
    observer(ExecutionEvent::Phase(ExecutionPhase::OutputKeys));
    run.poll(crate::dataset::resolved::Step::OutputKeys)?;
    let keys = key_columns(plan);
    let rows = frame.rows();
    let schema = &plan.region(RegionId::DATASET).output;
    let dataset = Completed { frame, schema };
    let checked = CheckRows {
        dataset: &dataset,
        secondary,
    };
    let mut failures = Vec::new();
    for (position, &column) in keys.iter().enumerate() {
        let missing: Vec<usize> = (0..rows)
            .filter(|&row| matches!(frame.get(row, column), ValueRef::Missing))
            .collect();
        if !missing.is_empty() {
            failures.push(CheckRecord {
                path: format!("keys[{position}]"),
                code: crate::diagnostic::ConditionCode::OutputMissingKeyFailed,
                evaluated_count: rows,
                failed_count: missing.len(),
                output_rows: rows,
                codelist: None,
                inventory: None,
                row_count: None,
                offending_rows: identities(frame, &keys, missing),
                entries: None,
                denominator: None,
            });
        }
    }
    let (evaluated, duplicates) = match run.key_groups.take() {
        Some(key_groups) if key_groups.len() == rows => repeated_keys(&key_groups)?,
        _ => {
            let groups = partition(frame, &keys, run.output_partition)
                .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
            let duplicates = groups
                .iter()
                .filter(|members| members.len() > 1)
                .map(|members| members[0])
                .collect();
            (groups.len(), duplicates)
        }
    };
    if !duplicates.is_empty() {
        failures.push(CheckRecord {
            path: "keys".into(),
            code: crate::diagnostic::ConditionCode::OutputDuplicateKeyFailed,
            evaluated_count: evaluated,
            failed_count: duplicates.len(),
            output_rows: rows,
            codelist: None,
            inventory: None,
            row_count: None,
            offending_rows: identities(frame, &keys, duplicates),
            entries: None,
            denominator: None,
        });
    }
    if !failures.is_empty() {
        return Err(Box::new(ExecutionError::KeyFailures(failures)));
    }
    observer(ExecutionEvent::Exit {
        phase: ExecutionPhase::OutputKeys,
        outcome: crate::application_observer::Outcome::Succeeded,
    });
    observer(ExecutionEvent::Phase(ExecutionPhase::Verification));
    let checks: Vec<CheckId> = plan
        .checks()
        .iter()
        .enumerate()
        .filter(|(_, check)| check.scope == CheckScope::Dataset)
        .map(|(index, _)| CheckId::try_from(index).expect("a check slot"))
        .collect();
    for check in checks {
        run.poll(crate::dataset::resolved::Step::Checks(check))?;
        let node = plan.check(check);
        let path = node.path.to_string();
        let record = match declared(plan, check) {
            Check::InvalidDiagnostic(diagnostic) => {
                return Err(Box::new(ExecutionError::VerificationDiagnostic {
                    diagnostic: diagnostic.clone(),
                    records: std::mem::take(records),
                }));
            }
            Check::InvalidDeclaration { code, reason } => {
                return Err(Box::new(ExecutionError::VerificationDeclaration {
                    path,
                    code: *code,
                    reason: reason.clone(),
                    records: std::mem::take(records),
                }));
            }
            check_declared @ (Check::Assert { .. } | Check::PredicateDeclaration(_)) => {
                let (when, require, code) = match check_declared {
                    Check::Assert { when, require } => (
                        when.as_ref(),
                        require,
                        Some(crate::diagnostic::ConditionCode::VerificationAssertFailed),
                    ),
                    Check::PredicateDeclaration(predicate) => (None, predicate, None),
                    _ => unreachable!("a predicate check"),
                };
                for predicate in when.into_iter().chain([require]) {
                    validate_predicate(plan, predicate, &dataset, records)?;
                }
                let Some(code) = code else {
                    continue;
                };
                let predicates = node.predicates.clone();
                let (when_expr, require_expr) = match predicates.as_slice() {
                    [when, require] => (Some(*when), *require),
                    [require] => (None, *require),
                    _ => unreachable!("an assertion has one or two predicates"),
                };
                let mut offending = Vec::new();
                for row in 0..rows {
                    let binds = match (when, when_expr) {
                        (Some(when), Some(expr)) => {
                            evaluate_predicate(run, &checked, when, expr, &path, row, records)?
                        }
                        _ => Truth::True,
                    };
                    let holds = evaluate_predicate(
                        run,
                        &checked,
                        require,
                        require_expr,
                        &path,
                        row,
                        records,
                    )?;
                    if binds == Truth::True && holds != Truth::True {
                        offending.push(row);
                    }
                }
                CheckRecord {
                    path,
                    code,
                    evaluated_count: rows,
                    failed_count: offending.len(),
                    output_rows: rows,
                    codelist: None,
                    inventory: None,
                    row_count: None,
                    offending_rows: identities(frame, &keys, offending),
                    entries: None,
                    denominator: None,
                }
            }
            Check::Unique(columns) => {
                // Repeated references do not change tuple equality.
                let mut distinct = Vec::new();
                for &column in columns {
                    if !distinct.contains(&column) {
                        distinct.push(column);
                    }
                }
                let groups = partition(frame, &distinct, run.output_partition)
                    .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
                let repeated: Vec<&[usize]> =
                    groups.iter().filter(|members| members.len() > 1).collect();
                CheckRecord {
                    path,
                    code: crate::diagnostic::ConditionCode::VerificationUniqueFailed,
                    evaluated_count: groups.len(),
                    failed_count: repeated.len(),
                    output_rows: rows,
                    codelist: None,
                    inventory: None,
                    row_count: None,
                    offending_rows: identities(
                        frame,
                        &keys,
                        repeated
                            .into_iter()
                            .flat_map(|members| members.iter().copied()),
                    ),
                    entries: None,
                    denominator: None,
                }
            }
            Check::AllOrNone(columns) => {
                let offending: Vec<usize> = (0..rows)
                    .filter(|&row| {
                        let missing = matches!(frame.get(row, columns[0]), ValueRef::Missing);
                        columns.iter().any(|&column| {
                            matches!(frame.get(row, column), ValueRef::Missing) != missing
                        })
                    })
                    .collect();
                let offending_rows = identities(frame, &keys, offending);
                CheckRecord {
                    path,
                    code: crate::diagnostic::ConditionCode::VerificationAllOrNoneFailed,
                    evaluated_count: rows,
                    failed_count: offending_rows.len(),
                    output_rows: rows,
                    codelist: None,
                    inventory: None,
                    row_count: None,
                    offending_rows,
                    entries: None,
                    denominator: None,
                }
            }
            Check::ScopedRowCount(count) => {
                row_count_check(run, &checked, check, count, &path, records)?
            }
            check_declared @ (Check::RowCount { .. } | Check::WideRowCount { .. }) => {
                let failed = !check_declared.row_count_accepts(rows).unwrap_or(false);
                CheckRecord {
                    path,
                    code: crate::diagnostic::ConditionCode::VerificationRowCountFailed,
                    evaluated_count: 1,
                    failed_count: usize::from(failed),
                    output_rows: rows,
                    codelist: None,
                    inventory: None,
                    row_count: None,
                    offending_rows: Vec::new(),
                    entries: None,
                    denominator: None,
                }
            }
            _ => unreachable!("column-only check is rejected at admission"),
        };
        records.push(record);
    }
    if records.iter().any(|record| {
        record.failed_count != 0 && severity(plan, &record.path) == VerificationSeverity::Error
    }) {
        return Err(Box::new(ExecutionError::VerificationFailures(
            std::mem::take(records),
        )));
    }
    Ok(())
}

/// Nonmissing representatives expose invalid declarations even for empty output.
fn sample(kind: ColumnType) -> Value {
    use crate::temporal::{Date, DateTime, Precision};
    match kind {
        ColumnType::Str => Value::Str("sample".into()),
        ColumnType::Int => Value::Int(0),
        ColumnType::Float => Value::float(0.0),
        ColumnType::Date => Value::Date(Date::new(2000, 1, 1, Precision::Day).expect("a date")),
        ColumnType::DateTime => Value::DateTime(
            DateTime::new(
                Date::new(2000, 1, 1, Precision::Day).expect("a date"),
                0,
                0,
                0,
                Precision::Second,
            )
            .expect("a datetime"),
        ),
    }
}

/// Validate a predicate's types over samples from immutable schemas, even
/// for an empty output; samples never select donors, run handlers or read
/// source cells.
fn validate_predicate<E>(
    plan: &ResolvedPlan,
    predicate: &BoundPredicate,
    dataset: &Completed<'_>,
    records: &mut Vec<CheckRecord>,
) -> Result<(), Fail<E>> {
    let region = plan.region(RegionId::DATASET);
    let mut samples = Vec::new();
    let mut bindings = Vec::new();
    for binding in predicate.bindings() {
        let kind = match binding.read {
            BoundRead::Column(column) => Some(region.output.columns()[column].kind),
            BoundRead::Intermediate { index, column } => {
                let relation = plan
                    .named(RegionId::DATASET, index)
                    .expect("a verification reads a declared intermediate");
                super::relations::item(plan, relation).source_kind(&region.secondary, column)
            }
            _ => unreachable!(
                "verification admission permits completed columns and declared lookups"
            ),
        };
        bindings.push(Binding {
            name: binding.name.clone(),
            read: BoundRead::Column(samples.len()),
        });
        samples.push(kind.map_or(Value::Missing, sample));
    }
    let sample_predicate = BoundPredicate::new(predicate.plan().clone(), bindings)
        .expect("admitted verification names");
    let table = DatasetTable(dataset);
    match super::predicates::row_truth(
        &sample_predicate,
        Table::<Infallible>::Source(&table),
        0,
        |column| ValueRef::from(&samples[column]),
    ) {
        // Samples expose type faults; an arithmetic failure depends on values.
        Err(error) if matches!(error.kind, ErrorKind::Condition(Condition::Arithmetic(_))) => {
            Ok(())
        }
        Err(error) => Err(Box::new(ExecutionError::VerificationPredicate {
            error: *error,
            records: std::mem::take(records),
        })),
        Ok(_) => Ok(()),
    }
}

/// The completed rows as an infallible table port.
struct DatasetTable<'d, 'f>(&'d Completed<'f>);
impl TableAccess for DatasetTable<'_, '_> {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        self.0.schema()
    }
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.0.cell(row, column)
    }
}

/// One row's truth of a verification predicate that reads completed columns
/// alone.
fn completed_truth<E>(
    predicate: &BoundPredicate,
    dataset: &Completed<'_>,
    row: usize,
    records: &mut Vec<CheckRecord>,
) -> Result<Truth, Fail<E>> {
    let table = DatasetTable(dataset);
    super::predicates::row_truth(
        predicate,
        Table::<Infallible>::Source(&table),
        0,
        |column| dataset.frame.get(row, column),
    )
    .map_err(|error| {
        Box::new(ExecutionError::VerificationPredicate {
            error: *error,
            records: std::mem::take(records),
        })
    })
}

/// The rows a dataset check reads: the completed rows, as a table over the
/// output schema, and the secondary inputs a predicate that reads a lookup
/// reaches through the case evaluator.
struct CheckRows<'c, 's, E> {
    dataset: &'c Completed<'c>,
    secondary: &'c [Table<'s, E>],
}

/// One row's truth of a verification predicate: over the completed values,
/// or through the case evaluator when it reads a lookup.
fn evaluate_predicate<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    checked: &CheckRows<'_, 's, E>,
    predicate: &BoundPredicate,
    expr: crate::dataset::resolved::ExprId,
    path: &str,
    row: usize,
    records: &mut Vec<CheckRecord>,
) -> Result<Truth, Fail<E>> {
    let CheckRows { dataset, secondary } = *checked;
    let frame = dataset.frame;
    if !predicate
        .bindings()
        .iter()
        .any(|binding| matches!(binding.read, BoundRead::Intermediate { .. }))
    {
        return completed_truth(predicate, dataset, row, records);
    }
    let mut cx = run.template_cx(
        frame,
        RegionId::DATASET,
        Table::Source(run.primary),
        secondary,
        frame.rows(),
    );
    cx.output = 0..frame.rows();
    super::predicates::verification_truth(run, &cx, expr, path, row).map_err(|error| match *error {
        ExecutionError::Predicate { source_row, error } => match error.kind {
            ErrorKind::Condition(condition) => Box::new(ExecutionError::VerificationPredicate {
                error: crate::expression::EvaluationError {
                    spec_path: error.spec_path,
                    expression: error.expression,
                    route: error.route,
                    source_span: error.source_span,
                    kind: ErrorKind::Condition(condition),
                },
                records: std::mem::take(records),
            }),
            _ => Box::new(ExecutionError::Predicate { source_row, error }),
        },
        other => Box::new(other),
    })
}

/// Partition all artifact rows before applying predicates; a filtered-empty
/// group remains visible.
fn row_count_check<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    checked: &CheckRows<'_, 's, E>,
    check: CheckId,
    count: &crate::dataset::RowCount,
    path: &str,
    records: &mut Vec<CheckRecord>,
) -> Result<CheckRecord, Fail<E>> {
    let (dataset, frame) = (checked.dataset, checked.dataset.frame);
    let plan = run.plan;
    for predicate in count.filter.iter().chain(&count.when) {
        validate_predicate(plan, predicate, dataset, records)?;
    }
    let predicates = plan.check(check).predicates.clone();
    let mut exprs = predicates.into_iter();
    let filter_expr = count
        .filter
        .as_ref()
        .map(|_| exprs.next().expect("a filter node"));
    let when_expr = count
        .when
        .as_ref()
        .map(|_| exprs.next().expect("a when node"));
    let rows = frame.rows();
    let mut admitted = Vec::new();
    super::reserved(admitted.try_reserve_exact(rows))?;
    for row in 0..rows {
        admitted.push(match (&count.filter, filter_expr) {
            (Some(filter), Some(expr)) => {
                evaluate_predicate(run, checked, filter, expr, path, row, records)? == Truth::True
            }
            _ => true,
        });
    }
    let mut bound = Vec::new();
    super::reserved(bound.try_reserve_exact(rows))?;
    for row in 0..rows {
        bound.push(match (&count.when, when_expr) {
            (Some(when), Some(expr)) => {
                evaluate_predicate(run, checked, when, expr, path, row, records)? == Truth::True
            }
            _ => true,
        });
    }
    let groups = match &count.group_by {
        Some(columns) => partition(frame, columns, run.output_partition)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?,
        None => super::group::Groups::whole(rows),
    };
    let mut offenders = Vec::new();
    let mut counts = Vec::new();
    let mut denominators = Vec::new();
    for members in groups.iter() {
        if count.when.is_some() && !members.iter().any(|&row| bound[row]) {
            continue;
        }
        let admitted_count = members.iter().filter(|&&row| admitted[row]).count();
        if !count.accepts(admitted_count, members.len()) {
            let values = count
                .group_by
                .iter()
                .flatten()
                .map(|&column| frame.value(members[0], column))
                .collect();
            offenders.push(RowIdentity {
                position: members.first().copied().unwrap_or(0),
                values,
            });
            counts.push(admitted_count);
            denominators.push(members.len());
        }
    }
    Ok(CheckRecord {
        path: path.into(),
        code: crate::diagnostic::ConditionCode::VerificationRowCountFailed,
        evaluated_count: groups.len(),
        failed_count: offenders.len(),
        output_rows: rows,
        offending_rows: offenders,
        codelist: None,
        inventory: None,
        row_count: Some(crate::dataset::execution::RowCountObservation {
            counts,
            denominators: (count.min_fraction.is_some() || count.max_fraction.is_some())
                .then_some(denominators),
        }),
        entries: None,
        denominator: None,
    })
}

/// Partition rows by columns in first-occurrence order: missing values
/// compare equal, text is exact, temporal precision and float zero signs do
/// not affect equality (ROW-4). The rows are admitted as the output is: each
/// partition of at most `partition` rows, or whole (D264).
pub(super) fn partition(
    frame: &Frame,
    columns: &[usize],
    partition: Option<std::num::NonZeroUsize>,
) -> Result<super::group::Groups, crate::table_grouping::GroupingError<Infallible>> {
    super::group::admit(columns, frame.width(), frame.rows(), partition)?;
    super::group::partition(frame.rows(), columns.len(), |row, position| {
        frame.get(row, columns[position])
    })
    .map_err(|_| crate::table_grouping::GroupingError::Allocation)
}

/// The output key groups the key grain found (#4188), given the group of
/// each row: how many there are, and the first row of each that repeats a
/// key, in row order, as `partition` over the keys would answer. A key-grain
/// candidate repeats its key only when the key is missing, as each
/// missing-key record is its own candidate.
fn repeated_keys<E>(key_groups: &[usize]) -> Result<(usize, Vec<usize>), Fail<E>> {
    let groups = key_groups.iter().max().map_or(0, |&group| group + 1);
    let mut sizes = super::filled(groups, 0usize)?;
    for &group in key_groups {
        sizes[group] += 1;
    }
    let mut duplicates = Vec::new();
    for (row, &group) in key_groups.iter().enumerate() {
        if sizes[group] > 1 {
            duplicates.push(row);
            // Only the group's first row names it.
            sizes[group] = 0;
        }
    }
    Ok((groups, duplicates))
}

/// Stable presentation order over completed rows, after all verification:
/// each output column has its declared type, so every two of its values
/// compare (VALUE-22).
pub(super) fn output_order<E>(plan: &ResolvedPlan, frame: &Frame) -> Result<Vec<usize>, Fail<E>> {
    let mut rows: Vec<usize> = (0..frame.rows()).collect();
    let terms = plan.output_order();
    if terms.is_empty() || rows.len() < 2 {
        return Ok(rows);
    }
    let first = plan
        .region_columns(RegionId::DATASET)
        .next()
        .map_or(0, |column| column.index());
    let terms: Vec<crate::dataset::OrderTerm> = terms
        .iter()
        .map(|term| crate::dataset::OrderTerm {
            column: term.column.index() - first,
            descending: term.descending,
            nulls_first: term.nulls_first,
        })
        .collect();
    // Rows are in construction order, which a tie keeps (VALUE-23).
    super::relations::sort(&mut rows, &terms, |&row, term| {
        frame.get(row, terms[term].column)
    })
    .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
    Ok(rows)
}
