//! Family-independent submission admission and completed-column length binding.
//! Standard, document and terminology composition remains the Define use case.
use super::*;
use crate::{
    dataset::{Check, Verification},
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    value::Value,
};
use alloc::{
    borrow::ToOwned,
    collections::{BTreeMap, BTreeSet},
    string::ToString,
};

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
        N::Integer(value) => V::Integer(value.clone()),
        N::Boolean(value) => V::Scalar(Value::Bool(*value)),
        N::Text(value) => s(value),
        N::Float(value) => V::Scalar(Value::float(*value)),
        _ => V::Scalar(Value::Missing),
    }
}
fn s(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
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
    let entries = match &d.nodes()[id] {
        N::Sequence(entries) => entries.as_slice(),
        _ => core::slice::from_ref(&id),
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
fn kinds(d: &Document, node: usize, intermediates: &BTreeSet<String>) -> &'static [&'static str] {
    match crate::submission_composition::derivation_kind(d, node, intermediates) {
        "literal" => &["Assigned", "Other", "Protocol"],
        "source" => &[
            "Assigned",
            "Collected",
            "Not Available",
            "Other",
            "Predecessor",
            "Protocol",
        ],
        _ => &["Assigned", "Derived", "Other"],
    }
}
fn origins(
    d: &Document,
    submission: usize,
    name: &str,
    row: Option<&str>,
    path: &str,
    inherited_method: bool,
    result: &mut Vec<PreflightFinding>,
) -> Option<String> {
    let mut fields = vec![("column", s(name))];
    if let Some(row) = row {
        fields.push(("row", s(row)));
    }
    let Some(origin) = present_field(d, submission, "origin") else {
        result.push(finding(C::SubmissionOriginMissing, path, &fields));
        return None;
    };
    let kind = string(d, origin, "type")?;
    if ["Predecessor", "Other", "Not Available"].contains(&kind)
        && string(d, origin, "description").is_none_or(str::is_empty)
    {
        let mut context = fields.clone();
        context.push(("origin_type", s(kind)));
        result.push(finding(C::SubmissionOriginDescription, path, &context));
    }
    if kind == "Derived" && present_field(d, submission, "method").is_none() && !inherited_method {
        result.push(finding(C::SubmissionMethodMissing, path, &fields));
    }
    Some(kind.into())
}
// Normalized metadata is compared directly, including defaulted fields; arena
// indices and mapping order cannot invent a different declaration identity.
fn same(d: &Document, a: usize, b: usize) -> bool {
    match (&d.nodes()[a], &d.nodes()[b]) {
        (N::Mapping(a), N::Mapping(b)) => {
            a.len() == b.len()
                && a.iter().all(|&(key, value)| {
                    b.iter()
                        .find(|&&(other, _)| d.nodes()[key] == d.nodes()[other])
                        .is_some_and(|&(_, other)| same(d, value, other))
                })
        }
        (N::Sequence(a), N::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(&a, &b)| same(d, a, b))
        }
        (a, b) => a == b,
    }
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
        let length_type = ["text", "integer", "float"].contains(&resolved);
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
    if effective_field(d, entry, base, "mandatory")
        .is_some_and(|id| d.nodes()[id] == N::Boolean(true))
    {
        let keys = sequence(d, field(d, d.root(), "keys")?)?;
        if !keys
            .iter()
            .any(|&id| text(d, id).is_ok_and(|key| key == name))
            && checks(d, column, "not_missing").is_empty()
        {
            result.push(finding(
                C::SubmissionMandatoryUnenforced,
                path,
                &[("column", s(name))],
            ));
        }
    }
    // An explicit verification binds the entire column once. Only authored
    // value overrides without such a verification introduce a scoped check.
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

pub(super) fn admit(
    d: &Document,
    columns: &[usize],
    inventory: &[String],
    groups: &mut [verifications::Verifications],
    result: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let mut metadata_budget = crate::project_limits::Budget::new(Default::default());
    for node in d.nodes() {
        if let N::Text(text) | N::Integer(text) = node {
            metadata_budget
                .text(text)
                .map_err(|_| PrepareError::Limit("submission_metadata"))?;
        }
    }
    let root = d.root();
    let output = sequence(d, field(d, field(d, root, "output")?, "columns")?)?
        .iter()
        .map(|&id| text(d, id))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let keys = sequence(d, field(d, root, "keys")?)?
        .iter()
        .map(|&id| text(d, id))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let rows = present_field(d, root, "rows")
        .map(|id| sequence(d, id))
        .transpose()?
        .unwrap_or(&[]);
    let intermediates = present_field(d, root, "intermediates")
        .map(|id| sequence(d, id))
        .transpose()?
        .unwrap_or(&[])
        .iter()
        .map(|&id| text(d, field(d, id, "id")?).map(String::from))
        .collect::<Result<BTreeSet<_>, _>>()?;
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
                    ("reference_data", V::Scalar(Value::Bool(true))),
                    ("repeating", V::Scalar(Value::Bool(true))),
                ],
            ));
        }
    }
    let mut by_name = BTreeMap::new();
    for (index, &column) in columns.iter().enumerate() {
        let name = text(d, field(d, column, "name")?)?;
        by_name.insert(name, column);
        reserved(d, column, Some(name), result)?;
        let Some(submission) = present_field(d, column, "submission") else {
            continue;
        };
        let path = format!("columns.{name}.submission");
        if !output.contains(name) {
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
                                .map_err(|_| PrepareError::Limit("submission_length"))?,
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
            let length_type = ["text", "integer", "float"].contains(&resolved);
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
        if d.field(submission, "mandatory")
            .is_some_and(|id| d.nodes()[id] == N::Boolean(true))
            && !keys.contains(name)
            && checks(d, column, "not_missing").is_empty()
        {
            result.push(finding(
                C::SubmissionMandatoryUnenforced,
                &path,
                &[("column", s(name))],
            ));
        }
        let Some(origin) = origins(d, submission, name, None, &path, false, result) else {
            continue;
        };
        let mut derivations = rows
            .iter()
            .filter_map(|&row| {
                if present_field(d, row, "submission")
                    .and_then(|id| present_field(d, id, name))
                    .is_some_and(|id| present_field(d, id, "origin").is_some())
                {
                    return None;
                }
                d.field(row, "derivations")
                    .and_then(|node| present_field(d, node, name))
                    .or_else(|| present_field(d, column, "derivation"))
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            derivations.extend(present_field(d, column, "derivation"));
        }
        if !derivations.is_empty() {
            let mut admitted = kinds(d, derivations[0], &intermediates).to_vec();
            for &node in &derivations[1..] {
                admitted.retain(|kind| kinds(d, node, &intermediates).contains(kind));
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
            } else if !admitted.contains(&origin.as_str()) {
                result.push(finding(
                    C::SubmissionOriginRefuted,
                    &path,
                    &[
                        ("column", s(name)),
                        ("origin_type", s(&origin)),
                        (
                            "admitted",
                            V::Sequence(admitted.iter().map(|kind| s(kind)).collect()),
                        ),
                    ],
                ));
            }
        }
    }
    // Value declarations are admitted as a whole before execution. Origin and
    // conflict decisions retain their written row/column identities.
    let domain = text(d, field(d, root, "domain")?)?;
    let discriminator = format!("{domain}TESTCD");
    let mut seen = BTreeMap::new();
    let mut value_lengths = Vec::new();
    for &row in rows {
        let Some(submission) = present_field(d, row, "submission") else {
            continue;
        };
        let entries = mapping(d, submission)?;
        if entries.is_empty() {
            continue;
        }
        let row_name = text(d, field(d, row, "id")?)?;
        let path = format!("rows.{row_name}.submission");
        for &(name, entry) in entries {
            let name = text(d, name)?;
            for _ in 0..4 {
                metadata_budget
                    .text(row_name)
                    .and_then(|_| metadata_budget.text(name))
                    .map_err(|_| PrepareError::Limit("submission_metadata"))?;
            }
            if let Some(&column) = by_name.get(name) {
                let inherits_origin = present_field(d, entry, "origin").is_none()
                    && present_field(d, column, "submission")
                        .is_some_and(|base| present_field(d, base, "origin").is_some());
                if !inherits_origin {
                    origins(
                        d,
                        entry,
                        name,
                        Some(row_name),
                        &format!("{path}.{name}"),
                        present_field(d, column, "submission")
                            .is_some_and(|id| present_field(d, id, "method").is_some()),
                        result,
                    );
                }
            }
        }
        if !by_name.contains_key(discriminator.as_str()) {
            result.push(finding(
                C::SubmissionValueTestcdMissing,
                &path,
                &[("domain", s(domain))],
            ));
            continue;
        }
        let row_declarations = field(d, row, "derivations")?;
        let testcd = present_field(d, row_declarations, &discriminator)
            .and_then(|node| d.field(node, "value"))
            .and_then(|node| string(d, node, "literal"));
        let Some(testcd) = testcd else {
            result.push(finding(
                C::SubmissionValueTestcdDynamic,
                &path,
                &[("discriminator", s(&discriminator))],
            ));
            continue;
        };
        for &(name, entry) in entries {
            let name = text(d, name)?;
            let field_path = format!("{path}.{name}");
            let effective = by_name.get(name).and_then(|&column| {
                present_field(d, row_declarations, name)
                    .or_else(|| present_field(d, column, "derivation"))
            });
            let Some(effective) = effective else {
                result.push(finding(
                    C::SubmissionValueColumnUnknown,
                    &field_path,
                    &[("column", s(name)), ("row", s(row_name))],
                ));
                continue;
            };
            if !output.contains(name) {
                result.push(finding(
                    C::SubmissionValueColumnNotOutput,
                    &field_path,
                    &[("column", s(name))],
                ));
            }
            let column = by_name[name];
            let length = admit_value_fields(d, entry, column, name, &field_path, result)?;
            if let Some(&first) = seen.get(&(name, testcd)) {
                if !same(d, first, entry) {
                    result.push(finding(
                        C::SubmissionValueMetadataConflict,
                        &field_path,
                        &[("column", s(name)), ("testcd", s(testcd))],
                    ));
                    continue;
                }
            } else {
                seen.insert((name, testcd), entry);
                if let Some(max) = length {
                    value_lengths.push((
                        columns
                            .iter()
                            .position(|&id| id == column)
                            .expect("declared output column"),
                        testcd.to_owned(),
                        max,
                        format!("{field_path}.length"),
                    ));
                }
            }
            if let Some(origin) = present_field(d, entry, "origin")
                .or_else(|| {
                    by_name
                        .get(name)
                        .and_then(|&column| present_field(d, column, "submission"))
                        .and_then(|base| present_field(d, base, "origin"))
                })
                .and_then(|id| string(d, id, "type"))
            {
                if !kinds(d, effective, &intermediates).contains(&origin) {
                    result.push(finding(
                        C::SubmissionValueOriginRefuted,
                        &field_path,
                        &[
                            ("column", s(name)),
                            ("testcd", s(testcd)),
                            (
                                "derivation_kind",
                                s(crate::submission_composition::derivation_kind(
                                    d,
                                    effective,
                                    &intermediates,
                                )),
                            ),
                            ("origin_type", s(origin)),
                        ],
                    ));
                }
            }
        }
    }
    // Apply the final effective bounds over each completed discriminator value.
    // Equal overrides share the column's existing check; differing overrides
    // replace the inherited synthetic bound only for their declared test codes.
    for (column, group) in groups.iter_mut().enumerate() {
        let authored = value_lengths
            .iter()
            .filter(|entry| entry.0 == column)
            .collect::<Vec<_>>();
        if authored.is_empty() {
            continue;
        }
        let name = text(d, field(d, columns[column], "name")?)?;
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
            .filter(|entry| default.as_ref() != Some(&entry.2))
            .collect::<Vec<_>>();
        if overrides.is_empty() {
            continue;
        }
        let Some(discriminator) = columns
            .iter()
            .position(|&node| string(d, node, "name") == Some(discriminator.as_str()))
        else {
            continue;
        };
        if let (Some(index), Some(max)) = (default_index, default) {
            group.checks[index].check = Check::ValueMaxLength {
                max: crate::dataset::RowCountBound::new(&max)
                    .map_err(|_| PrepareError::Limit("submission_length"))?,
                discriminator,
                codes: overrides.iter().map(|entry| entry.1.clone()).collect(),
                include: false,
            };
        }
        for entry in overrides {
            group.append_metadata(Verification {
                path: entry.3.clone(),
                check: Check::ValueMaxLength {
                    max: crate::dataset::RowCountBound::new(&entry.2)
                        .map_err(|_| PrepareError::Limit("submission_length"))?,
                    discriminator,
                    codes: vec![entry.1.clone()],
                    include: true,
                },
            });
        }
    }
    let mut inventory_budget = crate::project_limits::Budget::new(Default::default());
    for (column, &node) in columns.iter().enumerate() {
        let name = text(d, field(d, node, "name")?)?;
        let base = present_field(d, node, "submission");
        let default = base
            .and_then(|id| present_field(d, id, "inventory_vocabulary"))
            .is_some_and(|id| d.nodes()[id] == N::Boolean(true));
        let mut overrides = BTreeMap::new();
        for &row in rows {
            let Some(value) = present_field(d, row, "submission")
                .and_then(|id| present_field(d, id, name))
                .and_then(|id| present_field(d, id, "inventory_vocabulary"))
            else {
                continue;
            };
            let Some(code) = present_field(d, row, "derivations")
                .and_then(|id| present_field(d, id, &discriminator))
                .and_then(|id| present_field(d, id, "value"))
                .and_then(|id| string(d, id, "literal"))
            else {
                continue;
            };
            let N::Boolean(enabled) = d.nodes()[value] else {
                continue;
            };
            overrides.insert(code.into(), enabled);
        }
        if !default && !overrides.values().any(|enabled| *enabled) {
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
        let scope = if overrides.is_empty() {
            None
        } else {
            columns
                .iter()
                .position(|&node| string(d, node, "name") == Some(discriminator.as_str()))
                .map(|discriminator| crate::dataset::InventoryScope {
                    discriminator,
                    default,
                    overrides: overrides.into_iter().collect(),
                })
        };
        for name in inventory {
            inventory_budget
                .text(name)
                .map_err(|_| PrepareError::Limit("inventory_bindings"))?;
        }
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
