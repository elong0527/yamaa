//! The root checks, over the tree's typed fields: declaration limits, then
//! duplicates, keys, drivers and row templates, then the inputs.
use super::*;
use crate::diagnostic::{ConditionCode as C, Context as DiagnosticContext, Diagnostic};
use crate::limits::Limits;
use std::collections::BTreeSet;

/// The declaration counts preparation admits before it compiles anything.
pub(in crate::specification) fn limits(spec: &Spec) -> Result<(), PrepareError> {
    for (resource, actual, limit) in [
        ("columns", spec.columns.len(), Limits::COLUMNS),
        ("keys", spec.keys.len(), Limits::KEYS),
    ] {
        if actual > limit {
            return Err(PrepareError::limit(
                resource,
                limit,
                Some(actual),
                Some(resource),
            ));
        }
    }
    if spec.inputs.len() > Limits::INPUTS {
        return Err(PrepareError::limit(
            "inputs",
            Limits::INPUTS,
            Some(spec.inputs.len()),
            Some("input"),
        ));
    }
    if spec.output.columns.len() > Limits::COLUMNS {
        return Err(PrepareError::limit(
            "projected_columns",
            Limits::COLUMNS,
            Some(spec.output.columns.len()),
            Some("output.columns"),
        ));
    }
    for column in &spec.columns {
        let Some(derivation) = &column.derivation else {
            continue;
        };
        if let ExprKind::FirstAvailable { sources, .. } = &spec.expr(derivation.expr).kind {
            if sources.len() > Limits::SOURCE_OPERANDS {
                return Err(PrepareError::limit(
                    "source_operands",
                    Limits::SOURCE_OPERANDS,
                    Some(sources.len()),
                    Some(&format!(
                        "columns.{}.derivation.first_available.sources",
                        column.name.name
                    )),
                ));
            }
        }
    }
    // Row shapes are admitted before filters or default expansion allocate a plan.
    for (index, template) in spec.rows.templates.iter().enumerate() {
        for (resource, name, actual, limit) in [
            (
                "group_keys",
                "group_by",
                template.group_by.as_ref().map_or(0, Vec::len),
                Limits::KEYS,
            ),
            (
                "row_columns",
                "derivations",
                template.overrides.len(),
                Limits::COLUMNS,
            ),
        ] {
            if actual > limit {
                return Err(PrepareError::limit(
                    resource,
                    limit,
                    Some(actual),
                    Some(&format!("rows[{index}].{name}")),
                ));
            }
        }
    }
    Ok(())
}

/// Every independent root finding, in the reference preflight order.
pub(in crate::specification) fn findings(spec: &Spec) -> Vec<PreflightFinding> {
    let mut findings = Vec::new();
    let names = spec
        .columns
        .iter()
        .map(|column| column.name.name.as_str())
        .collect::<Vec<_>>();
    for (position, &name) in names.iter().enumerate() {
        if names[..position].contains(&name) {
            let mut finding = LookupFinding::new(
                &format!("columns[{position}].name"),
                C::PreflightDuplicateColumn,
            );
            finding.text("identifier", name);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
    }
    let rows = &spec.rows.templates;
    for (index, template) in rows.iter().enumerate() {
        for (column, _) in &template.overrides {
            if !names.contains(&column.name.as_str()) {
                findings.push(PreflightFinding::UndeclaredRowColumn {
                    index,
                    column: column.name.clone(),
                });
            }
        }
    }
    let overrides = |template: &ast::Template, name: &str| {
        template
            .overrides
            .iter()
            .any(|(column, _)| column.name == name)
    };
    if !rows.is_empty() {
        for (column, &name) in spec.columns.iter().zip(&names) {
            match &column.derivation {
                // Aggregate column results belong to the completed dataset and
                // cannot be promoted into row-local defaults (SPEC-6).
                Some(derivation) if aggregate(spec, derivation.expr) => {
                    let overridden = rows
                        .iter()
                        .filter(|template| overrides(template, name))
                        .map(|template| template.id.name.clone())
                        .collect::<Vec<_>>();
                    if !overridden.is_empty() {
                        findings.push(PreflightFinding::DuplicateRowDefault {
                            column: name.into(),
                            rows: overridden,
                            not_row_local: "aggregate".into(),
                        });
                    }
                }
                Some(_) => {}
                None => {
                    let missing = rows
                        .iter()
                        .filter(|template| !overrides(template, name))
                        .map(|template| template.id.name.clone())
                        .collect::<Vec<_>>();
                    if !missing.is_empty() {
                        findings.push(PreflightFinding::MissingRowDerivation {
                            column: name.into(),
                            rows: missing,
                        });
                    }
                }
            }
        }
    } else {
        for (column, &name) in spec.columns.iter().zip(&names) {
            if column.derivation.is_none() {
                findings.push(PreflightFinding::MissingDerivation {
                    column: name.into(),
                });
            }
        }
    }
    let inputs = spec
        .inputs
        .iter()
        .map(|input| input.name.name.as_str())
        .collect::<Vec<_>>();
    if inputs.contains(&spec.domain.name.as_str()) {
        findings.push(PreflightFinding::DomainInputCollision {
            domain: spec.domain.name.clone(),
        });
    }
    if spec.keys.is_empty() {
        let mut finding = LookupFinding::new("keys", C::PreflightEmptyKeys);
        finding.text("expected", "at least one key column");
        finding.text("actual", "empty list");
        findings.push(PreflightFinding::Scalar(finding.diagnostic()));
    }
    for (position, key) in spec.keys.iter().enumerate() {
        let name = key.name.as_str();
        if spec.keys[..position].iter().any(|prior| prior.name == name) {
            let mut finding =
                LookupFinding::new(&format!("keys[{position}]"), C::PreflightDuplicateKey);
            finding.text("identifier", name);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
        if !names.contains(&name) {
            findings.push(PreflightFinding::UndeclaredKey {
                position,
                column: name.into(),
            });
        }
    }
    if rows.is_empty() && default_driver(spec).is_none() {
        findings.push(PreflightFinding::DriverUnavailable { dataset: None });
    }
    if let Some(base) = &spec.rows.base {
        if !inputs.contains(&base.name.as_str()) {
            findings.push(PreflightFinding::DriverUnavailable {
                dataset: Some(base.name.clone()),
            });
        }
    }
    if !rows.is_empty() && spec.rows.filter.is_some() {
        findings.push(PreflightFinding::ConflictingRowConstruction);
    }
    for (index, template) in rows.iter().enumerate() {
        let identity = template.id.name.as_str();
        if let Some(previous) = rows[..index]
            .iter()
            .position(|prior| prior.id.name == identity)
        {
            let mut finding =
                LookupFinding::new(&format!("rows[{index}].id"), C::PreflightDuplicateRow)
                    .diagnostic();
            finding.spec_paths.push(format!("rows[{previous}].id"));
            finding.context.insert(
                "identifier".into(),
                crate::json::Value::String(identity.into()),
            );
            findings.push(PreflightFinding::Scalar(finding));
        }
        let driver = template_driver(spec, template);
        if !driver.is_some_and(|name| {
            inputs.contains(&name) || spec.intermediates.iter().any(|item| item.id.name == name)
        }) {
            findings.push(PreflightFinding::RowDriverUnavailable {
                index,
                row: identity.into(),
                dataset: driver.map(String::from),
            });
        }
        let Some(group) = &template.group_by else {
            continue;
        };
        let groups = group
            .iter()
            .map(|&e| super::name(spec, e).unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        if groups.is_empty()
            || groups
                .iter()
                .enumerate()
                .any(|(position, name)| groups[..position].contains(name))
        {
            findings.push(PreflightFinding::InvalidGroup {
                index,
                row: identity.into(),
                groups,
            });
        } else if let Some(driver) = driver {
            for name in groups {
                if !name.starts_with(&format!("{driver}.")) || name.matches('.').count() != 1 {
                    findings.push(PreflightFinding::GroupReference {
                        index,
                        row: identity.into(),
                        name,
                        dataset: driver.into(),
                    });
                }
            }
        }
    }
    findings
}

/// Whether a derivation is an aggregate, as written.
fn aggregate(spec: &Spec, e: ExprId) -> bool {
    match &spec.expr(e).kind {
        ExprKind::Aggregate(_) => true,
        ExprKind::Unsupported(operation) => operation == "aggregate",
        _ => false,
    }
}

/// Each input's declaration findings: its written path, the encoding its
/// profile requires, the empty-string convention, its profile, what an ODM
/// input may not declare and the types a Parquet input restates.
pub(in crate::specification) fn source_findings(
    spec: &Spec,
    odm_inputs: &BTreeSet<&str>,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let mut findings = Vec::new();
    for input in &spec.inputs {
        let name = input.name.name.as_str();
        let written = input.source.path.text.as_str();
        if let Some(failure) = crate::resource::written_path_failure(written) {
            findings.push(PreflightFinding::Scalar(failure.diagnostic(name, written)));
            continue;
        }
        let profile = input.source.profile;
        let sas = matches!(profile, Some(ast::Profile::Xport | ast::Profile::Sas7bdat));
        if sas != input.source.encoding.is_some() {
            findings.push(PreflightFinding::Scalar(Diagnostic {
                code: C::SasSourceEncoding,
                spec_paths: vec![format!("input.{name}.encoding")],
                context: DiagnosticContext::from([(
                    "dataset".into(),
                    crate::json::Value::String(name.into()),
                )]),
                source_span: None,
                operand_route: None,
            }));
        }
        if profile == Some(ast::Profile::Csv)
            && input.source.empty_string == ast::EmptyString::Present
        {
            findings.push(PreflightFinding::EmptyStringPresentUnsupported {
                dataset: name.into(),
            });
        }
        if profile.is_none() {
            findings.push(PreflightFinding::UnknownSourceProfile {
                dataset: name.into(),
                path: written.into(),
            });
        }
        if odm_inputs.contains(name) {
            // A written `types` declares types even when it is empty (NAME-8).
            let types = input.source.written.contains("types");
            let declared = [
                (types, crate::diagnostic::OdmDeclared::Types),
                (
                    input.source.schema.is_some(),
                    crate::diagnostic::OdmDeclared::Schema,
                ),
                (
                    input.source.ordinal.is_some(),
                    crate::diagnostic::OdmDeclared::Ordinal,
                ),
            ]
            .into_iter()
            .find(|&(declared, _)| declared);
            if let Some((_, declared)) = declared {
                findings.push(PreflightFinding::Odm(
                    crate::diagnostic::OdmFinding::Declared {
                        dataset: name,
                        declared,
                    }
                    .diagnostic(),
                ));
            }
            continue;
        }
        if profile == Some(ast::Profile::Parquet) && !input.source.types.is_empty() {
            if input.source.types.len() > Limits::COLUMNS {
                return Err(PrepareError::limit(
                    "source_fields",
                    Limits::COLUMNS,
                    Some(input.source.types.len()),
                    Some(&format!("input.{name}.types")),
                ));
            }
            for (field, kind) in &input.source.types {
                findings.push(PreflightFinding::RedundantSourceType {
                    dataset: name.into(),
                    field: field.name.clone(),
                    kind: *kind,
                });
            }
        }
    }
    Ok(findings)
}
