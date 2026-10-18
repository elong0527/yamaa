//! Family-independent submission admission and completed-column length binding.
//! Standard, document and terminology composition remains the Define use case.
use super::*;
use crate::json::Value as V;
use crate::{
    dataset::{Check, InventoryScope, RowScope, Verification, WhereClause},
    diagnostic::{ConditionCode as C, Diagnostic},
    specification_submission_document::OriginType,
    submission_origin::{self as origin, Reads},
    submission_values as values,
    value::Value,
};
use std::collections::BTreeSet;

fn present_field(d: &Document, node: usize, name: &str) -> Option<usize> {
    d.field(node, name)
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
}
fn string<'a>(d: &'a Document, node: usize, name: &str) -> Option<&'a str> {
    match &d.nodes()[present_field(d, node, name)?] {
        N::Text(value) => Some(value),
        _ => None,
    }
}
fn scalar(d: &Document, node: usize) -> V {
    match &d.nodes()[node] {
        N::Integer(value) => V::Number((value.clone()).parse().expect("canonical integer")),
        N::Boolean(value) => V::Bool(*value),
        N::Text(value) => s(value),
        N::Float(value) => V::from(Value::float(*value)),
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
fn checks(d: &Document, column: usize, name: &str) -> Vec<usize> {
    let Some(id) = present_field(d, column, "verifications") else {
        return Vec::new();
    };
    let N::Sequence(entries) = &d.nodes()[id] else {
        return Vec::new();
    };
    entries.iter().filter_map(|&id| d.field(id, name)).collect()
}
fn reserved(
    d: &Document,
    node: usize,
    column: Option<&str>,
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let Some(metadata) = present_field(d, node, "metadata") else {
        return Ok(());
    };
    let governed = if column.is_some() {
        crate::specification_submission_document::COLUMN_METADATA_FIELDS
    } else {
        crate::specification_submission_document::DATASET_METADATA_FIELDS
    };
    for &(key, _) in mapping(d, metadata)? {
        let key = text(d, key)?;
        if governed.iter().any(|field| field.name() == key) {
            let mut fields = vec![
                ("key", s(key)),
                ("level", s(if column.is_some() { "column" } else { "root" })),
            ];
            if let Some(column) = column {
                fields.push(("column", s(column)));
            }
            result.push(finding(
                C::SubmissionReservedMetadata,
                &column.map_or_else(
                    || format!("metadata.{key}"),
                    |name| format!("columns.{name}.metadata.{key}"),
                ),
                &fields,
            ));
        }
    }
    Ok(())
}
/// The origin types SUB-19 leaves unrefuted by one derivation.
fn kinds(d: &Document, node: usize, reads: &Reads) -> Vec<&'static str> {
    origin::classify(d, node, reads).admitted()
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
fn declared(d: &Document, origin: usize) -> Option<Declared<'_>> {
    Some(Declared {
        kind: string(d, origin, "type")?,
        description: string(d, origin, "description"),
    })
}
fn origins<'a>(
    d: &'a Document,
    submission: usize,
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
    let Some(origin) = present_field(d, submission, "origin") else {
        result.push(finding(C::SubmissionOriginMissing, path, &fields));
        return None;
    };
    let declaration = declared(d, origin)?;
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
    if kind == "Derived" && present_field(d, submission, "method").is_none() && !inherited_method {
        result.push(finding(C::SubmissionMethodMissing, path, &fields));
    }
    Some(declaration)
}
/// SUB-20: the first derivation that is not the copy a Predecessor claims.
fn copy_failure(
    d: &Document,
    derivations: &[usize],
    predecessor: &origin::Predecessor<'_>,
) -> Option<origin::CopyFailure> {
    derivations
        .iter()
        .find_map(|&node| origin::copies(d, node, predecessor).err())
}

fn effective_field(
    d: &Document,
    override_node: usize,
    base: Option<usize>,
    name: &str,
) -> Option<usize> {
    present_field(d, override_node, name)
        .or_else(|| base.and_then(|base| present_field(d, base, name)))
}
fn admit_value_fields(
    d: &Document,
    entry: usize,
    column: usize,
    name: &str,
    path: &str,
    result: &mut Vec<PreflightFinding>,
) -> Result<Option<String>, PrepareError> {
    let base = present_field(d, column, "submission");
    let kind = text(d, field(d, column, "type")?)?;
    let resolved = effective_field(d, entry, base, "data_type")
        .map(|id| text(d, id))
        .transpose()?
        .unwrap_or(if kind == "str" {
            "text"
        } else if kind == "int" {
            "integer"
        } else {
            kind
        });
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
    let length = effective_field(d, entry, base, "length");
    let digits = effective_field(d, entry, base, "significant_digits");
    for (field_name, node) in [("length", length), ("significant_digits", digits)] {
        if let Some(node) = node {
            let valid = matches!(&d.nodes()[node], N::Integer(value) if !value.starts_with('-') && (field_name != "length" || value != "0"));
            if !valid {
                result.push(finding(
                    C::SubmissionLengthInvalid,
                    &format!("{path}.{field_name}"),
                    &[("column", s(name)), (field_name, scalar(d, node))],
                ));
            }
        }
    }
    let maxima = if kind == "str" {
        checks(d, column, "max_length")
            .into_iter()
            .filter_map(|id| present_field(d, id, "max"))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if let Some(length) = length {
        if let Some(&max) = maxima
            .iter()
            .find(|&&max| d.nodes()[length] != d.nodes()[max])
        {
            result.push(finding(
                C::SubmissionLengthConflict,
                path,
                &[
                    ("column", s(name)),
                    ("length", scalar(d, length)),
                    ("max_length", scalar(d, max)),
                ],
            ));
        }
    }
    if present_field(d, d.root(), "submission").is_some() {
        // SUB-7: a date or datetime never declares a length, whatever type it
        // is submitted as.
        let length_type = !["date", "datetime"].contains(&kind)
            && ["text", "integer", "float"].contains(&resolved);
        if length_type && length.is_none() && maxima.is_empty() {
            result.push(finding(
                C::SubmissionLengthMissing,
                path,
                &[("column", s(name)), ("data_type", s(resolved))],
            ));
        }
        if !length_type && length.is_some() && maxima.is_empty() {
            result.push(finding(
                C::SubmissionLengthNotApplicable,
                path,
                &[("column", s(name)), ("data_type", s(resolved))],
            ));
        }
        if resolved == "float" && digits.is_none() {
            result.push(finding(
                C::SubmissionLengthMissing,
                path,
                &[("column", s(name)), ("field", s("significant_digits"))],
            ));
        }
        if resolved != "float" && digits.is_some() {
            result.push(finding(
                C::SubmissionLengthNotApplicable,
                path,
                &[("column", s(name)), ("field", s("significant_digits"))],
            ));
        }
    }
    // An explicit verification binds the entire column once. Only authored
    // entry lengths without such a verification introduce a scoped check.
    if kind == "str" && maxima.is_empty() {
        if let Some(node) = present_field(d, entry, "length") {
            if let N::Integer(value) = &d.nodes()[node] {
                if !value.starts_with('-') && value != "0" {
                    return Ok(Some(value.clone()));
                }
            }
        }
    }
    Ok(None)
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
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    reads: &Reads,
    result: &mut Vec<PreflightFinding>,
) -> Result<Vec<ValueLength>, PrepareError> {
    let declared_columns = values::declared_columns(d);
    let mut lengths = Vec::new();
    for (index, &column) in columns.iter().enumerate() {
        let name = output.columns()[index].name.as_str();
        let base = present_field(d, column, "submission");
        for entry in values::entries(d, column) {
            let path = values::path(name, entry.id);
            // An entry without its own origin answers with the column's,
            // which the column's own refutation already covers.
            let declaration = if entry.declares(d, "origin") {
                origins(
                    d,
                    entry.node,
                    name,
                    Some(entry.id),
                    &path,
                    base.is_some_and(|id| present_field(d, id, "method").is_some()),
                    result,
                )
            } else {
                None
            };
            let length = admit_value_fields(d, entry.node, column, name, &path, result)?;
            if let (Some(max), Some(clause)) =
                (length, super::value_level::clause(&entry, output, index))
            {
                lengths.push(ValueLength {
                    column: index,
                    clause,
                    max,
                    path: format!("{path}.length"),
                });
            }
            let Some(declaration) = declaration else {
                continue;
            };
            let derivations = values::entry_derivations(d, column, name, &declared_columns, &entry);
            let refuting = derivations
                .iter()
                .map(|&node| origin::classify(d, node, reads))
                .find(|kind| !kind.admitted().contains(&declaration.kind));
            let copy = if refuting.is_some() {
                None
            } else {
                declaration.predecessor().and_then(|predecessor| {
                    derivations.iter().find_map(|&node| {
                        origin::copies(d, node, &predecessor)
                            .err()
                            .map(|failure| (node, failure))
                    })
                })
            };
            let kind = match (refuting, copy) {
                (Some(kind), _) => kind,
                (None, Some((node, _))) => origin::classify(d, node, reads),
                (None, None) => continue,
            };
            let mut context = vec![
                ("column", s(name)),
                ("entry", s(entry.id)),
                ("derivation_kind", s(kind.name())),
                ("origin_type", s(declaration.kind)),
            ];
            if let Some((_, failure)) = copy {
                context.push(("reason", s(failure.reason())));
            }
            result.push(finding(C::SubmissionValueOriginRefuted, &path, &context));
        }
    }
    Ok(lengths)
}
fn apply_value_bounds_and_inventory(
    d: &Document,
    columns: &[usize],
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
        let name = output.columns()[column].name.as_str();
        let default_path = format!("columns.{name}.verifications.max_length");
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
    for (column, &node) in columns.iter().enumerate() {
        let name = output.columns()[column].name.as_str();
        let base = present_field(d, node, "submission");
        let default = base
            .and_then(|id| present_field(d, id, "inventory_vocabulary"))
            .is_some_and(|id| d.nodes()[id] == N::Boolean(true));
        let mut overrides = Vec::new();
        for entry in values::entries(d, node) {
            let Some(value) = present_field(d, entry.node, "inventory_vocabulary") else {
                continue;
            };
            let N::Boolean(enabled) = d.nodes()[value] else {
                continue;
            };
            let Some(clause) = super::value_level::clause(&entry, output, column) else {
                continue;
            };
            overrides.push((clause, enabled));
        }
        if !default && !overrides.iter().any(|(_, enabled)| *enabled) {
            continue;
        }
        if text(d, field(d, node, "type")?)? != "str" {
            result.push(finding(
                C::SubmissionDataType,
                &format!("columns.{name}.submission.inventory_vocabulary"),
                &[
                    ("column", s(name)),
                    ("declared_type", s(text(d, field(d, node, "type")?)?)),
                    ("data_type", s("text")),
                ],
            ));
            continue;
        }
        let scope = (!overrides.is_empty()).then_some(InventoryScope { default, overrides });
        groups[column].append_codelist(Verification {
            path: format!("columns.{name}.submission.inventory_vocabulary"),
            check: Check::Inventory {
                values: inventory.iter().cloned().map(Value::Str).collect(),
                scope,
            },
        });
    }
    Ok(())
}
pub(super) fn admit(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    inventory: &[String],
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let root = d.root();
    let carried = sequence(d, field(d, field(d, root, "output")?, "columns")?)?
        .iter()
        .map(|&id| text(d, id))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let reads = Reads::of(d);
    let declared_columns = values::declared_columns(d);
    reserved(d, root, None, result)?;
    let has_root = present_field(d, root, "submission");
    if let Some(metadata) = has_root {
        if d.field(metadata, "reference_data")
            .is_some_and(|id| d.nodes()[id] == N::Boolean(true))
            && d.field(metadata, "repeating")
                .is_some_and(|id| d.nodes()[id] == N::Boolean(true))
        {
            result.push(finding(
                C::SubmissionReferenceRepeating,
                "submission",
                &[
                    ("reference_data", V::Bool(true)),
                    ("repeating", V::Bool(true)),
                ],
            ));
        }
    }
    for (index, &column) in columns.iter().enumerate() {
        let name = text(d, field(d, column, "name")?)?;
        reserved(d, column, Some(name), result)?;
        let Some(submission) = present_field(d, column, "submission") else {
            continue;
        };
        let path = format!("columns.{name}.submission");
        if !carried.contains(name) {
            result.push(finding(
                C::SubmissionColumnNotOutput,
                &path,
                &[("column", s(name))],
            ));
        }
        let kind = text(d, field(d, column, "type")?)?;
        let resolved = string(d, submission, "data_type").unwrap_or(match kind {
            "str" => "text",
            "int" => "integer",
            other => other,
        });
        let admitted =
            crate::specification_submission_document::admits_data_type_names(kind, resolved);
        if !admitted {
            result.push(finding(
                C::SubmissionDataType,
                &path,
                &[
                    ("column", s(name)),
                    ("declared_type", s(kind)),
                    ("data_type", s(resolved)),
                ],
            ));
        }
        let length = present_field(d, submission, "length");
        for (field_name, node) in [
            ("length", length),
            (
                "significant_digits",
                present_field(d, submission, "significant_digits"),
            ),
        ] {
            if let Some(node) = node {
                let valid = matches!(&d.nodes()[node], N::Integer(value) if !value.starts_with('-') && (field_name != "length" || value != "0"));
                if !valid {
                    result.push(finding(
                        C::SubmissionLengthInvalid,
                        &format!("{path}.{field_name}"),
                        &[("column", s(name)), (field_name, scalar(d, node))],
                    ));
                }
            }
        }
        let maxima = if kind == "str" {
            checks(d, column, "max_length")
                .into_iter()
                .filter_map(|id| present_field(d, id, "max"))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if let Some(length) = length {
            if let Some(&max) = maxima
                .iter()
                .find(|&&max| d.nodes()[length] != d.nodes()[max])
            {
                result.push(finding(
                    C::SubmissionLengthConflict,
                    &path,
                    &[
                        ("column", s(name)),
                        ("length", scalar(d, length)),
                        ("max_length", scalar(d, max)),
                    ],
                ));
            }
            if kind == "str" && maxima.is_empty() {
                let N::Integer(value) = &d.nodes()[length] else {
                    continue;
                };
                if !value.starts_with('-') && value != "0" {
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
                        path: format!("columns.{name}.verifications.max_length"),
                        check,
                    });
                }
            }
        }
        if has_root.is_some() {
            // SUB-7: a date or datetime never declares a length, whatever
            // type it is submitted as.
            let length_type = !["date", "datetime"].contains(&kind)
                && ["text", "integer", "float"].contains(&resolved);
            if length_type && length.is_none() && maxima.is_empty() {
                result.push(finding(
                    C::SubmissionLengthMissing,
                    &path,
                    &[("column", s(name)), ("data_type", s(resolved))],
                ));
            }
            if !length_type && length.is_some() && maxima.is_empty() {
                result.push(finding(
                    C::SubmissionLengthNotApplicable,
                    &path,
                    &[("column", s(name)), ("data_type", s(resolved))],
                ));
            }
            let digits = present_field(d, submission, "significant_digits");
            if resolved == "float" && digits.is_none() {
                result.push(finding(
                    C::SubmissionLengthMissing,
                    &path,
                    &[("column", s(name)), ("field", s("significant_digits"))],
                ));
            }
            if resolved != "float" && digits.is_some() {
                result.push(finding(
                    C::SubmissionLengthNotApplicable,
                    &path,
                    &[("column", s(name)), ("field", s("significant_digits"))],
                ));
            }
            if present_field(d, submission, "core").is_none() {
                result.push(finding(
                    C::SubmissionCoreMandatory,
                    &path,
                    &[("column", s(name)), ("reason", s("core_missing"))],
                ));
            }
        }
        let Some(origin) = origins(d, submission, name, None, &path, false, result) else {
            continue;
        };
        // The column's origin answers for every construction whose rows an
        // entry declaring its own origin does not all select (SUB-19).
        let entries = values::entries(d, column);
        let derivations = values::column_derivations(d, column, name, &declared_columns, &entries);
        if !derivations.is_empty() {
            let mut admitted = kinds(d, derivations[0], &reads);
            for &node in &derivations[1..] {
                admitted.retain(|kind| kinds(d, node, &reads).contains(kind));
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
                    copy_failure(d, &derivations, &predecessor)
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
    let lengths = admit_entries(d, columns, output, &reads, result)?;
    apply_value_bounds_and_inventory(d, columns, output, inventory, groups, result, &lengths)?;
    Ok(())
}
