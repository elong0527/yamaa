//! Literal historical declarations share the XML writer, without making
//! assertions about current derivations or data. This mode cannot publish a
//! transport or claim Submission context (DEFINE-30).
use super::*;

pub(super) fn dataset(
    section: &ast::Section,
    dataset: &spec::Dataset,
    path: &str,
    findings: &mut Vec<Finding>,
) {
    let historical = dataset.mode.value == spec::SubmissionMode::Historical;
    let refused = if historical {
        section.context != DefineContext::Other
            || section.transport.is_some()
            || section.define_version != ast::project::DefineVersion::V2_0
    } else {
        dataset.artifact.is_some()
    };
    if refused {
        findings.push(crate::define_diagnostics::xml_invalid(
            &format!("{path}.submission"),
            "historical metadata requires Define-XML 2.0, context Other and no transport; a literal artifact requires historical mode",
            crate::json::Value::String(dataset.mode.value.name().into()),
        ));
    }
}

pub(super) fn column(
    admission: &ColumnAdmission<'_>,
    column: &spec::Column,
    path: &str,
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
    let mut complete = true;
    for (field, absent) in [
        ("data_type", metadata.data_type.is_none()),
        ("mandatory", metadata.mandatory.is_none()),
        ("label", column.label.is_none()),
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
    let key_sequence = metadata.key_sequence.as_ref().and_then(|declared| {
        match declared
            .value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
        {
            Some(value) => Some(value),
            None => {
                findings.push(crate::define_diagnostics::xml_invalid(
                    &format!("{path}.key_sequence"),
                    "positive key sequence",
                    crate::json::Value::String(declared.value.clone()),
                ));
                None
            }
        }
    });
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
        values: Vec::new(),
        transport_length: None,
        has_no_data: metadata
            .has_no_data
            .as_ref()
            .is_some_and(|field| field.value),
    })
}
