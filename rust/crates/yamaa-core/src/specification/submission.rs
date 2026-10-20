//! Family-independent submission admission and completed-column length binding,
//! read from the typed tree. Standard, document and terminology composition
//! remains the Define use case.
use super::*;
use crate::json::Value as V;
use crate::{
    ast::{self, ColumnCheckKind, Constant, Literal, SubmissionMetadata},
    dataset::{Check, InventoryScope, RowScope, Verification, WhereClause},
    diagnostic::{ConditionCode as C, Diagnostic},
    specification_submission_document::OriginType,
    submission_origin::{self as origin, Reads},
    submission_values as values,
    value::Value,
};
use std::collections::BTreeSet;

fn scalar(value: &Literal) -> V {
    match value {
        Literal::Int(value) => V::Number((value.clone()).parse().expect("canonical integer")),
        Literal::Bool(value) => V::Bool(*value),
        Literal::Text(value) => s(value),
        Literal::Float(value) => V::from(Value::float(*value)),
        _ => V::Null,
    }
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn finding(code: C, path: &str, fields: &[(&str, V)]) -> PreflightFinding {
    PreflightFinding::Scalar(Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context: fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
        source_span: None,
        operand_route: None,
    })
}
/// The submission data type a column declares, or the one its type implies.
fn data_type<'a>(kind: &'a str, declared: Option<&'a ast::Text>) -> &'a str {
    declared.map_or(
        match kind {
            "str" => "text",
            "int" => "integer",
            other => other,
        },
        |declared| declared.text.as_str(),
    )
}
/// A written length or significant-digit count: a non-negative integer, and
/// a length is never zero.
fn valid_count(field: &str, value: &Constant) -> bool {
    matches!(&value.value, Literal::Int(value) if !value.starts_with('-') && (field != "length" || value != "0"))
}
/// The `max` each of a `str` column's `max_length` verifications writes.
fn maxima(column: &ast::Column) -> Vec<&Constant> {
    if column.kind != ColumnType::Str {
        return Vec::new();
    }
    column
        .checks
        .iter()
        .filter_map(|check| match &check.kind {
            ColumnCheckKind::MaxLength(max) if max.value != Literal::Missing => Some(max),
            _ => None,
        })
        .collect()
}
fn reserved(
    spec: &ast::Spec,
    metadata: &[(ast::Named, ast::Text)],
    column: Option<&str>,
    result: &mut Vec<PreflightFinding>,
) {
    let governed = if column.is_some() {
        crate::specification_submission_document::COLUMN_METADATA_FIELDS
    } else {
        crate::specification_submission_document::DATASET_METADATA_FIELDS
    };
    for (key, _) in metadata {
        if governed.iter().any(|field| field.name() == key.name) {
            let mut fields = vec![
                ("key", s(&key.name)),
                ("level", s(if column.is_some() { "column" } else { "root" })),
            ];
            if let Some(column) = column {
                fields.push(("column", s(column)));
            }
            result.push(finding(
                C::SubmissionReservedMetadata,
                &spec.cite(key.meta),
                &fields,
            ));
        }
    }
}
/// A declared origin's type and description.
struct Declared<'a> {
    kind: &'a str,
    description: Option<&'a str>,
}
impl Declared<'_> {
    /// The column a Predecessor origin names, when its description has the form.
    fn predecessor(&self) -> Option<origin::Predecessor<'_>> {
        (self.kind == OriginType::Predecessor.name())
            .then_some(self.description?)
            .and_then(origin::predecessor)
    }
}
fn origins<'a>(
    metadata: &'a SubmissionMetadata,
    name: &str,
    entry: Option<&str>,
    path: &str,
    inherited_method: bool,
    result: &mut Vec<PreflightFinding>,
) -> Option<Declared<'a>> {
    let mut fields = vec![("column", s(name))];
    if let Some(entry) = entry {
        fields.push(("entry", s(entry)));
    }
    let Some(origin) = metadata.origins.first() else {
        result.push(finding(C::SubmissionOriginMissing, path, &fields));
        return None;
    };
    let declaration = Declared {
        kind: &origin.kind.text,
        description: origin
            .description
            .as_ref()
            .map(|description| description.text.as_str()),
    };
    let kind = declaration.kind;
    if ["Predecessor", "Other", "Not Available"].contains(&kind)
        && declaration.description.is_none_or(str::is_empty)
    {
        let mut context = fields.clone();
        context.push(("origin_type", s(kind)));
        result.push(finding(C::SubmissionOriginDescription, path, &context));
    } else if kind == OriginType::Predecessor.name() && declaration.predecessor().is_none() {
        // SUB-16: a Predecessor description names `<dataset>.<column>`.
        let mut context = fields.clone();
        context.push(("origin_type", s(kind)));
        context.push(("reason", s("predecessor_form")));
        result.push(finding(
            C::SubmissionOriginDescription,
            &format!("{path}.origin.description"),
            &context,
        ));
    }
    if kind == "Derived" && metadata.method.is_none() && !inherited_method {
        result.push(finding(C::SubmissionMethodMissing, path, &fields));
    }
    Some(declaration)
}

/// A column's or a value-level entry's effective submission metadata: the
/// entry's own field, or the column's where the entry declares none.
#[derive(Clone, Copy)]
struct Effective<'a> {
    data_type: Option<&'a ast::Text>,
    length: Option<&'a Constant>,
    digits: Option<&'a Constant>,
}
impl<'a> Effective<'a> {
    fn of(metadata: &'a SubmissionMetadata) -> Self {
        Self {
            data_type: metadata.data_type.as_ref(),
            length: metadata.length.as_ref(),
            digits: metadata.significant_digits.as_ref(),
        }
    }
    fn over(self, base: Self) -> Self {
        Self {
            data_type: self.data_type.or(base.data_type),
            length: self.length.or(base.length),
            digits: self.digits.or(base.digits),
        }
    }
}

/// The type, count and length findings a column's or an entry's effective
/// metadata raises at `path`.
fn admit_counts(
    spec: &ast::Spec,
    column: &ast::Column,
    path: &str,
    effective: Effective<'_>,
    result: &mut Vec<PreflightFinding>,
) {
    let name = column.name.name.as_str();
    let kind = column.kind.name();
    let resolved = data_type(kind, effective.data_type);
    let maxima = maxima(column);
    if !crate::specification_submission_document::admits_data_type_names(kind, resolved) {
        result.push(finding(
            C::SubmissionDataType,
            path,
            &[
                ("column", s(name)),
                ("declared_type", s(kind)),
                ("data_type", s(resolved)),
            ],
        ));
    }
    for (field, count) in [
        ("length", effective.length),
        ("significant_digits", effective.digits),
    ] {
        if let Some(count) = count.filter(|count| !valid_count(field, count)) {
            result.push(finding(
                C::SubmissionLengthInvalid,
                &format!("{path}.{field}"),
                &[("column", s(name)), (field, scalar(&count.value))],
            ));
        }
    }
    if let Some(length) = effective.length {
        if let Some(max) = maxima.iter().find(|max| max.value != length.value) {
            result.push(finding(
                C::SubmissionLengthConflict,
                path,
                &[
                    ("column", s(name)),
                    ("length", scalar(&length.value)),
                    ("max_length", scalar(&max.value)),
                ],
            ));
        }
    }
    if spec.submission.is_none() {
        return;
    }
    // SUB-7: a date or datetime never declares a length, whatever type it is
    // submitted as.
    let length_type =
        !["date", "datetime"].contains(&kind) && ["text", "integer", "float"].contains(&resolved);
    if length_type && effective.length.is_none() && maxima.is_empty() {
        result.push(finding(
            C::SubmissionLengthMissing,
            path,
            &[("column", s(name)), ("data_type", s(resolved))],
        ));
    }
    if !length_type && effective.length.is_some() && maxima.is_empty() {
        result.push(finding(
            C::SubmissionLengthNotApplicable,
            path,
            &[("column", s(name)), ("data_type", s(resolved))],
        ));
    }
    if resolved == "float" && effective.digits.is_none() {
        result.push(finding(
            C::SubmissionLengthMissing,
            path,
            &[("column", s(name)), ("field", s("significant_digits"))],
        ));
    }
    if resolved != "float" && effective.digits.is_some() {
        result.push(finding(
            C::SubmissionLengthNotApplicable,
            path,
            &[("column", s(name)), ("field", s("significant_digits"))],
        ));
    }
}

/// An authored length a `str` column without a `max_length` verification
/// is bound by.
fn authored_length<'a>(column: &ast::Column, length: Option<&'a Constant>) -> Option<&'a str> {
    match &length?.value {
        Literal::Int(value)
            if column.kind == ColumnType::Str
                && maxima(column).is_empty()
                && !value.starts_with('-')
                && value != "0" =>
        {
            Some(value)
        }
        _ => None,
    }
}

/// The path of the `max_length` check a column's authored length implies.
fn implied_max_length(spec: &ast::Spec, column: &ast::Column) -> String {
    format!("{}.verifications.max_length", spec.cite(column.meta))
}

/// One entry's authored length bound over the rows its where clause selects.
struct ValueLength {
    column: usize,
    clause: WhereClause,
    max: String,
    path: String,
}

/// Admit every entry's own metadata over its merged metadata, refute each
/// entry's own origin against the derivations that can produce its rows
/// (SUB-13), and collect the length bounds entries author.
fn admit_entries(
    spec: &ast::Spec,
    output: &TableSchema,
    reads: &Reads,
    result: &mut Vec<PreflightFinding>,
) -> Vec<ValueLength> {
    let mut lengths = Vec::new();
    for (index, column) in spec.columns.iter().enumerate() {
        let name = output.columns()[index].name.as_str();
        let Some(submission) = &column.submission else {
            continue;
        };
        let base = &submission.metadata;
        for entry in &submission.value_level {
            let path = spec.cite(entry.meta);
            let own = &entry.metadata;
            // An entry without its own origin answers with the column's,
            // which the column's own refutation already covers.
            let declaration = if own.origins.is_empty() {
                None
            } else {
                origins(
                    own,
                    name,
                    Some(&entry.id.name),
                    &path,
                    base.method.is_some(),
                    result,
                )
            };
            admit_counts(
                spec,
                column,
                &path,
                Effective::of(own).over(Effective::of(base)),
                result,
            );
            // An explicit verification binds the entire column once. Only
            // authored entry lengths without such a verification introduce a
            // scoped check.
            if let (Some(max), Some(clause)) = (
                authored_length(column, own.length.as_ref()),
                super::value_level::clause(entry, output, index),
            ) {
                lengths.push(ValueLength {
                    column: index,
                    clause,
                    max: max.into(),
                    path: format!("{path}.length"),
                });
            }
            let Some(declaration) = declaration else {
                continue;
            };
            let derivations = values::entry_expressions(spec, column, entry);
            let refuting = derivations
                .iter()
                .map(|&expr| origin::classify_expr(spec, expr, reads))
                .find(|kind| !kind.admitted().contains(&declaration.kind));
            let copy = if refuting.is_some() {
                None
            } else {
                declaration.predecessor().and_then(|predecessor| {
                    derivations.iter().find_map(|&expr| {
                        origin::copies_expr(spec, expr, &predecessor, reads)
                            .err()
                            .map(|failure| (expr, failure))
                    })
                })
            };
            let kind = match (refuting, copy) {
                (Some(kind), _) => kind,
                (None, Some((expr, _))) => origin::classify_expr(spec, expr, reads),
                (None, None) => continue,
            };
            let mut context = vec![
                ("column", s(name)),
                ("entry", s(&entry.id.name)),
                ("derivation_kind", s(kind.name())),
                ("origin_type", s(declaration.kind)),
            ];
            if let Some((_, failure)) = copy {
                context.push(("reason", s(failure.reason())));
            }
            result.push(finding(C::SubmissionValueOriginRefuted, &path, &context));
        }
    }
    lengths
}
fn apply_value_bounds_and_inventory(
    spec: &ast::Spec,
    output: &TableSchema,
    inventory: &[String],
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
    lengths: &[ValueLength],
) -> Result<(), PrepareError> {
    // Apply the final effective bounds over the rows each entry selects.
    // Equal entry bounds share the column's existing check; differing ones
    // replace the column's bound only on the rows their entries select.
    for (column, group) in groups.iter_mut().enumerate() {
        let authored = lengths
            .iter()
            .filter(|entry| entry.column == column)
            .collect::<Vec<_>>();
        if authored.is_empty() {
            continue;
        }
        let default_path = implied_max_length(spec, &spec.columns[column]);
        let default_index = group
            .checks
            .iter()
            .position(|check| check.path == default_path);
        let default = default_index.and_then(|index| match &group.checks[index].check {
            Check::MaxLength(max) => Some(max.to_string()),
            Check::WideMaxLength(max) => Some(max.decimal().to_owned()),
            _ => None,
        });
        let overrides = authored
            .into_iter()
            .filter(|entry| default.as_ref() != Some(&entry.max))
            .collect::<Vec<_>>();
        if overrides.is_empty() {
            continue;
        }
        if let (Some(index), Some(max)) = (default_index, default) {
            group.checks[index].check = Check::ValueMaxLength {
                max: crate::dataset::RowCountBound::new(&max)
                    .map_err(|_| PrepareError::Internal)?,
                scope: RowScope {
                    clauses: overrides.iter().map(|entry| entry.clause.clone()).collect(),
                    include: false,
                },
            };
        }
        for entry in overrides {
            group.append_metadata(Verification {
                path: entry.path.clone(),
                check: Check::ValueMaxLength {
                    max: crate::dataset::RowCountBound::new(&entry.max)
                        .map_err(|_| PrepareError::Internal)?,
                    scope: RowScope {
                        clauses: vec![entry.clause.clone()],
                        include: true,
                    },
                },
            });
        }
    }
    for (index, column) in spec.columns.iter().enumerate() {
        let Some(submission) = &column.submission else {
            continue;
        };
        let name = output.columns()[index].name.as_str();
        let default = submission.metadata.inventory_vocabulary == Some(true);
        let mut overrides = Vec::new();
        for entry in &submission.value_level {
            let Some(enabled) = entry.metadata.inventory_vocabulary else {
                continue;
            };
            let Some(clause) = super::value_level::clause(entry, output, index) else {
                continue;
            };
            overrides.push((clause, enabled));
        }
        if !default && !overrides.iter().any(|(_, enabled)| *enabled) {
            continue;
        }
        let path = format!("{}.inventory_vocabulary", spec.cite(submission.meta));
        if column.kind != ColumnType::Str {
            result.push(finding(
                C::SubmissionDataType,
                &path,
                &[
                    ("column", s(name)),
                    ("declared_type", s(column.kind.name())),
                    ("data_type", s("text")),
                ],
            ));
            continue;
        }
        let scope = (!overrides.is_empty()).then_some(InventoryScope { default, overrides });
        groups[index].append_codelist(Verification {
            path,
            check: Check::Inventory {
                values: inventory.iter().cloned().map(Value::Str).collect(),
                scope,
            },
        });
    }
    Ok(())
}
pub(super) fn admit(
    spec: &ast::Spec,
    output: &TableSchema,
    inventory: &[String],
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let carried = spec
        .output
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<BTreeSet<_>>();
    let reads = Reads::of_spec(spec);
    reserved(spec, &spec.metadata, None, result);
    if let Some(dataset) = &spec.submission {
        if dataset.reference_data && dataset.repeating {
            result.push(finding(
                C::SubmissionReferenceRepeating,
                &spec.cite(dataset.meta),
                &[
                    ("reference_data", V::Bool(true)),
                    ("repeating", V::Bool(true)),
                ],
            ));
        }
    }
    for (index, column) in spec.columns.iter().enumerate() {
        let name = column.name.name.as_str();
        reserved(spec, &column.metadata, Some(name), result);
        let Some(submission) = &column.submission else {
            continue;
        };
        let metadata = &submission.metadata;
        let path = spec.cite(submission.meta);
        if !carried.contains(name) {
            result.push(finding(
                C::SubmissionColumnNotOutput,
                &path,
                &[("column", s(name))],
            ));
        }
        admit_counts(spec, column, &path, Effective::of(metadata), result);
        if spec.submission.is_some() && metadata.core.is_none() {
            result.push(finding(
                C::SubmissionCoreMandatory,
                &path,
                &[("column", s(name)), ("reason", s("core_missing"))],
            ));
        }
        if let Some(value) = authored_length(column, metadata.length.as_ref()) {
            // A bound beyond the native text capacity cannot be exceeded;
            // retaining that bound must not narrow an authored integer.
            let check = match value.parse::<usize>() {
                Ok(maximum) => Check::MaxLength(maximum),
                Err(_) => Check::WideMaxLength(
                    crate::dataset::RowCountBound::new(value)
                        .map_err(|_| PrepareError::Internal)?,
                ),
            };
            groups[index].append_metadata(Verification {
                path: implied_max_length(spec, column),
                check,
            });
        }
        let Some(origin) = origins(metadata, name, None, &path, false, result) else {
            continue;
        };
        // The column's origin answers for every construction whose rows an
        // entry declaring its own origin does not all select (SUB-19).
        let derivations = values::column_expressions(spec, column);
        if !derivations.is_empty() {
            let mut admitted = origin::classify_expr(spec, derivations[0], &reads).admitted();
            for &expr in &derivations[1..] {
                let kinds = origin::classify_expr(spec, expr, &reads).admitted();
                admitted.retain(|kind| kinds.contains(kind));
            }
            if admitted.is_empty() {
                result.push(finding(
                    C::SubmissionOriginRefuted,
                    &path,
                    &[
                        ("column", s(name)),
                        ("reason", s("row_derivations_disagree")),
                    ],
                ));
            } else if !admitted.contains(&origin.kind) {
                result.push(finding(
                    C::SubmissionOriginRefuted,
                    &path,
                    &[
                        ("column", s(name)),
                        ("origin_type", s(origin.kind)),
                        (
                            "admitted",
                            V::Array(admitted.iter().map(|kind| s(kind)).collect()),
                        ),
                    ],
                ));
            } else if let Some((predecessor, failure)) =
                origin.predecessor().and_then(|predecessor| {
                    derivations
                        .iter()
                        .find_map(|&expr| {
                            origin::copies_expr(spec, expr, &predecessor, &reads).err()
                        })
                        .map(|failure| (predecessor.name(), failure))
                })
            {
                // SUB-20: a Predecessor is a bare `source` of the column it names.
                result.push(finding(
                    C::SubmissionOriginRefuted,
                    &path,
                    &[
                        ("column", s(name)),
                        ("origin_type", s(origin.kind)),
                        ("predecessor", s(&predecessor)),
                        ("reason", s(failure.reason())),
                    ],
                ));
            }
        }
    }
    let lengths = admit_entries(spec, output, &reads, result);
    apply_value_bounds_and_inventory(spec, output, inventory, groups, result, &lengths)?;
    data_type_checks(spec, output, groups);
    reject_datetime_columns(spec, &carried, result);
    Ok(())
}

/// SUB-26: a `str` column whose submission data type is a date or time type
/// has each present completed value checked against that type's ISO 8601
/// form, after its length and before its terminology. A value-level entry
/// declaring a data type other than the column's decides the rows its where
/// clause selects, and the column's data type every other row.
fn data_type_checks(
    spec: &ast::Spec,
    output: &TableSchema,
    groups: &mut [verifications::Verifications],
) {
    use crate::iso8601::Form;
    for (index, column) in spec.columns.iter().enumerate() {
        if column.kind != ColumnType::Str {
            continue;
        }
        let declared = column
            .submission
            .as_ref()
            .and_then(|submission| submission.metadata.data_type.as_ref());
        let effective = declared.map_or("text", |declared| declared.text.as_str());
        // Entries declaring their own, different data type, each over the
        // rows its where clause selects. Value-level admission reports a
        // clause it cannot select by.
        let mut overrides = Vec::new();
        for entry in values::levels(column) {
            let Some(own) = &entry.metadata.data_type else {
                continue;
            };
            if own.text == effective {
                continue;
            }
            let Some(clause) = super::value_level::clause(entry, output, index) else {
                continue;
            };
            overrides.push((clause, Form::of(&own.text), spec.cite(own.meta)));
        }
        if let Some((declared, form)) =
            declared.and_then(|declared| Some((declared, Form::of(&declared.text)?)))
        {
            // Every row but those the overriding entries select.
            let scope = (!overrides.is_empty()).then(|| RowScope {
                clauses: overrides
                    .iter()
                    .map(|(clause, _, _)| clause.clone())
                    .collect(),
                include: false,
            });
            groups[index].append_metadata(Verification {
                path: spec.cite(declared.meta),
                check: Check::DataType { form, scope },
            });
        }
        for (clause, form, path) in overrides {
            if let Some(form) = form {
                groups[index].append_metadata(Verification {
                    path,
                    check: Check::DataType {
                        form,
                        scope: Some(RowScope {
                            clauses: vec![clause],
                            include: true,
                        }),
                    },
                });
            }
        }
    }
}

/// SUB-27: a dataset whose class is a tabulation class, so of the `sdtm` or
/// `send` family, writes no `datetime` column; its date and time text is a
/// `str` column with a date or time data type.
fn reject_datetime_columns(
    spec: &ast::Spec,
    output: &BTreeSet<&str>,
    result: &mut Vec<PreflightFinding>,
) {
    let Some(class) = spec
        .submission
        .as_ref()
        .map(|dataset| dataset.class.text.as_str())
    else {
        return;
    };
    if !crate::specification_submission_document::DatasetClass::tabulation_name(class) {
        return;
    }
    for column in &spec.columns {
        let name = column.name.name.as_str();
        if output.contains(name) && column.kind == ColumnType::DateTime {
            result.push(finding(
                C::SubmissionDatetimeColumn,
                &format!("{}.type", spec.cite(column.meta)),
                &[
                    ("column", s(name)),
                    ("value", s("datetime")),
                    ("class", s(class)),
                    ("permitted", V::Array(vec![s("str"), s("date")])),
                ],
            ));
        }
    }
}
