//! Literal reproduced declarations share the XML writer, without making
//! assertions about current derivations or data. This mode cannot publish a
//! transport or claim Submission context (DEFINE-30).
use super::*;

/// A value-level entry's received `oid`, `name` and `where_oid` are a
/// reproduced record (DEFINE-30): validated metadata refuses each at the
/// entry at `path`, reporting its column.
pub(super) fn entry_fields(
    entry: &spec::ValueEntry,
    path: &str,
    column: &str,
    findings: &mut Vec<Finding>,
) {
    for (field, declared) in [
        ("entry oid", &entry.oid),
        ("entry name", &entry.name),
        ("where oid", &entry.where_oid),
    ] {
        if declared.is_some() {
            findings.push(crate::define_diagnostics::xml_invalid(
                path,
                &format!("reproduced {field} requires mode reproduced"),
                crate::json::Value::String(column.to_owned()),
            ));
        }
    }
}

/// Whether the listed specification `input` declares reproduced metadata
/// (DEFINE-30).
pub(super) fn listed(input: &Input<'_>) -> bool {
    input
        .tree
        .submission
        .as_ref()
        .is_some_and(|metadata| metadata.mode == spec::SubmissionMode::Reproduced)
}

/// A section's `metadata_version.oid` is the received document's
/// `MetaDataVersion` OID (DEFINE-9), a reproduced record: a section that
/// lists no reproduced specification among `inputs` refuses it at the field
/// (DEFINE-30).
pub(super) fn metadata_version_oid(
    section: &ast::Section,
    inputs: &[Input<'_>],
    findings: &mut Vec<Finding>,
) {
    let Some(oid) = section
        .metadata_version
        .as_ref()
        .and_then(|m| m.oid.as_ref())
    else {
        return;
    };
    if !inputs.iter().any(listed) {
        findings.push(crate::define_diagnostics::xml_invalid(
            &format!("{}.metadata_version.oid", family_name(section.family)),
            "reproduced metadata version oid requires mode reproduced",
            crate::json::Value::String(oid.text.clone()),
        ));
    }
}

/// The refusal of the listed specification at `path` whose `mode` the
/// section cannot carry: reproduced metadata outside a Define-XML 2.0 section
/// of context Other without a transport, or a literal artifact on validated
/// metadata (DEFINE-30).
pub fn mode_refused(path: &str, mode: spec::SubmissionMode) -> Finding {
    crate::define_diagnostics::xml_invalid(
        &format!("{path}.submission"),
        "reproduced metadata requires Define-XML 2.0, context Other and no transport; a literal artifact requires reproduced mode",
        crate::json::Value::String(mode.name().into()),
    )
}

pub(super) fn dataset(
    section: &ast::Section,
    dataset: &spec::Dataset,
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let reproduced = dataset.mode.value == spec::SubmissionMode::Reproduced;
    let refused = if reproduced {
        section.context != DefineContext::Other
            || section.transport.is_some()
            || section.define_version != ast::project::DefineVersion::V2_0
    } else {
        dataset.artifact.is_some()
    };
    if refused {
        findings.push(mode_refused(path, dataset.mode.value));
    }
    // A dataset's received CommentOID is a reproduced record (DEFINE-30).
    if !reproduced
        && dataset
            .comment
            .as_ref()
            .and_then(spec::Comment::oid)
            .is_some()
    {
        findings.push(crate::define_diagnostics::xml_invalid(
            &format!("{path}.submission"),
            "reproduced comment oid requires mode reproduced",
            crate::json::Value::String(dataset.mode.value.name().into()),
        ));
    }
}

/// The codelists a reproduced dataset's received document carries although
/// nothing binds them are written with the bound ones (CT-10, DEFINE-30): each
/// joins `bound`, and one the project does not declare is an unknown codelist
/// at its place in the list. Validated metadata refuses the list at the
/// dataset's `submission`, under the specification at `path`.
pub(super) fn codelists(
    dataset: &spec::Dataset,
    catalogue: &ct::Catalogue,
    path: &str,
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) {
    if dataset.codelists.is_empty() {
        return;
    }
    if dataset.mode.value != spec::SubmissionMode::Reproduced {
        findings.push(crate::define_diagnostics::xml_invalid(
            &format!("{path}.submission.codelists"),
            "reproduced codelists requires mode reproduced",
            crate::json::Value::String(dataset.mode.value.name().into()),
        ));
        return;
    }
    for (index, id) in dataset.codelists.iter().enumerate() {
        bound.insert(id.value.clone());
        if catalogue.get(&id.value).is_none() {
            findings.push(
                Finding::cause(
                    ConditionCode::CodelistUnknownBinding,
                    &format!("{path}.submission.codelists[{index}]"),
                    &id.value,
                )
                .with("codelist", id.value.as_str()),
            );
        }
    }
}

/// Whether reproduced metadata leaves `column` out of the document: its
/// `submission` declares `omitted: true` (DEFINE-30).
pub(super) fn omits(column: &spec::Column) -> bool {
    column
        .submission
        .as_ref()
        .and_then(|metadata| metadata.omitted.as_ref())
        .is_some_and(|field| field.value)
}

/// Whether the received document leaves `column` out, so that the document
/// writes no `ItemRef` or `ItemDef` for it. Nothing would write another field
/// of its `submission` (DEFINE-26), so one beside `omitted` is refused, once,
/// at the `omitted` that `path`, the column's `submission`, holds.
pub(super) fn omitted(column: &spec::Column, path: &str, findings: &mut Vec<Finding>) -> bool {
    if !omits(column) {
        return false;
    }
    let others = column.submission.as_ref().is_some_and(|metadata| {
        metadata
            .fields
            .iter()
            .any(|field| field.field != spec::FieldName::Omitted)
    });
    if others {
        findings.push(crate::define_diagnostics::xml_invalid(
            &format!("{path}.omitted"),
            "an omitted column declares no other submission field",
            crate::json::Value::Bool(true),
        ));
    }
    true
}

/// An `ItemGroupDef` requires an `ItemRef` (DEFINE-25): a dataset whose every
/// output column is omitted is refused at the `omitted` of the first, whose
/// `submission` path is `first`.
pub(super) fn no_item_ref(first: &str) -> Finding {
    crate::define_diagnostics::xml_invalid(
        &format!("{first}.omitted"),
        "an ItemGroupDef requires an ItemRef",
        crate::json::Value::Bool(true),
    )
}

/// `OrderNumber` is written on every `ItemRef` of an `ItemGroupDef` or on
/// none (DEFINE-18), and two equal numbers would leave the order undecided:
/// once a column the document describes declares `order`, each of them under
/// the dataset at `path` declares it, and no two declare the same number
/// (DEFINE-30). A column whose `order` is refused as no positive number
/// counts as declaring it, so it is reported once.
pub(super) fn orders(columns: &[Column], path: &str, findings: &mut Vec<Finding>) {
    let declared = |column: &Column| {
        column
            .declaration
            .submission
            .as_ref()
            .and_then(|metadata| metadata.order.as_ref())
            .map(|field| field.value.clone())
    };
    if columns.iter().all(|column| declared(column).is_none()) {
        return;
    }
    let mut seen = BTreeSet::new();
    for column in columns {
        let at = format!(
            "{path}.columns.{}.submission",
            column.declaration.name.value
        );
        match (declared(column), column.order) {
            (None, _) => findings.push(
                Finding::cause(ConditionCode::SubmissionMetadataMissing, &at, "order")
                    .with("field", "order"),
            ),
            (Some(text), Some(order)) if !seen.insert(order) => {
                findings.push(crate::define_diagnostics::xml_invalid(
                    &format!("{at}.order"),
                    "unique order",
                    crate::json::Value::String(text),
                ));
            }
            _ => {}
        }
    }
}

pub(super) fn column(
    admission: &ColumnAdmission<'_>,
    column: &spec::Column,
    path: &str,
    level: Level,
    findings: &mut Vec<Finding>,
    bound: &mut BTreeSet<String>,
) -> Option<Column> {
    let missing = |field: &str| {
        Finding::cause(ConditionCode::SubmissionMetadataMissing, path, field).with("field", field)
    };
    let Some(metadata) = column.submission.as_ref() else {
        findings.push(missing("submission"));
        return None;
    };
    // A per-value definition's description is its entry's own `label`,
    // written only when declared (DEFINE-30).
    let mut complete = true;
    for (field, absent) in [
        ("data_type", metadata.data_type.is_none()),
        ("mandatory", metadata.mandatory.is_none()),
        ("label", level == Level::Column && column.label.is_none()),
    ] {
        if absent {
            findings.push(missing(field));
            complete = false;
        }
    }
    if !complete {
        return None;
    }
    let data_type = metadata
        .data_type
        .as_ref()
        .expect("checked declaration")
        .value;
    for (name, field, zero_permitted) in [
        ("length", metadata.length.as_ref(), false),
        (
            "significant_digits",
            metadata.significant_digits.as_ref(),
            true,
        ),
    ] {
        if let Some(field) = field
            .filter(|field| field.value.starts_with('-') || (!zero_permitted && field.value == "0"))
        {
            findings.push(crate::define_diagnostics::xml_invalid(
                &format!("{path}.{name}"),
                "nonnegative integer, positive for Length",
                crate::json::Value::String(field.value.clone()),
            ));
        }
    }
    let mut positive = |declared: Option<&spec::Field<String>>, field: &str, facet: &str| {
        let declared = declared?;
        let value = declared
            .value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0);
        if value.is_none() {
            findings.push(crate::define_diagnostics::xml_invalid(
                &format!("{path}.{field}"),
                facet,
                crate::json::Value::String(declared.value.clone()),
            ));
        }
        value
    };
    let key_sequence = positive(
        metadata.key_sequence.as_ref(),
        "key_sequence",
        "positive key sequence",
    );
    let order = positive(metadata.order.as_ref(), "order", "positive order");
    if let Some(list) = &metadata.codelist {
        bound.insert(list.value.clone());
        if admission.catalogue.get(&list.value).is_none() {
            findings.push(
                Finding::cause(
                    ConditionCode::CodelistUnknownBinding,
                    &format!("{path}.codelist"),
                    &list.value,
                )
                .with("column", column.name.value.as_str())
                .with("codelist", list.value.as_str()),
            );
        }
    }
    let origins = metadata
        .origins
        .iter()
        .map(|declared| {
            references(findings, &declared.documents.value, admission.known, path);
            OriginDefinition {
                declaration: declared.clone(),
                source: declared.source.as_ref().map(|source| source.value),
                documents: declared.documents.value.clone(),
            }
        })
        .collect();
    if let Some(spec::Method::Class(method)) = &metadata.method {
        references(findings, &method.documents.value, admission.known, path);
    }
    comment_references(findings, metadata.comment.as_ref(), admission.known, path);
    Some(Column {
        declaration: column.clone(),
        data_type,
        length: metadata.length.as_ref().map(|field| field.value.clone()),
        display_format: metadata
            .display_format
            .as_ref()
            .map(|field| field.value.clone()),
        mandatory: metadata
            .mandatory
            .as_ref()
            .expect("checked declaration")
            .value,
        origins,
        key_sequence,
        order,
        values: Vec::new(),
        transport_length: None,
        has_no_data: metadata
            .has_no_data
            .as_ref()
            .is_some_and(|field| field.value),
        path: path.into(),
    })
}
