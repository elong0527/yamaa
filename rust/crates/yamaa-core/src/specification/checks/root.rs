//! The root checks, over the tree's typed fields: declaration limits, then
//! duplicates, keys, drivers and row templates, then the inputs.
use super::*;
use crate::diagnostic::{ConditionCode as C, Context as DiagnosticContext, Diagnostic};
use crate::expression_table::Verb;
use crate::limits::Limits;
use crate::names::Positions;
use std::collections::BTreeSet;

/// The declaration counts preparation admits before it compiles anything.
pub(in crate::specification) fn limits(spec: &Spec) -> Result<(), PrepareError> {
    if spec.keys.len() > Limits::KEYS {
        return Err(PrepareError::limit(
            "keys",
            Limits::KEYS,
            Some(spec.keys.len()),
            Some("keys"),
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
                        "columns.{}.derivation.{}.sources",
                        column.name.name,
                        Verb::FirstAvailable.name()
                    )),
                ));
            }
        }
    }
    // Row shapes are admitted before filters or default expansion allocate a plan.
    for template in &spec.rows.templates {
        let group_keys = template.group_by.as_ref().map_or(0, Vec::len);
        if group_keys > Limits::KEYS {
            return Err(PrepareError::limit(
                "group_keys",
                Limits::KEYS,
                Some(group_keys),
                Some(&format!("{}.group_by", spec.cite(template.meta))),
            ));
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
    // Each name is looked up, never searched for: a specification declares
    // as many columns, and a row template as many derivations, as its bytes
    // hold (#4782).
    let declared = Positions::of(names.iter().copied());
    for (position, &name) in names.iter().enumerate() {
        if declared.repeats(position, name) {
            let mut finding = LookupFinding::new(
                &format!("columns[{position}].name"),
                C::PreflightDuplicateColumn,
            );
            finding.text("identifier", name);
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
    }
    let rows = &spec.rows.templates;
    for template in rows {
        for (column, _) in &template.overrides {
            if !declared.contains(&column.name) {
                findings.push(PreflightFinding::UndeclaredRowColumn {
                    row_path: spec.cite(template.meta),
                    column: column.name.clone(),
                });
            }
        }
    }
    // The columns each row template derives, in the templates' order.
    let derived = rows
        .iter()
        .map(|template| {
            let columns = template.overrides.iter();
            Positions::of(columns.map(|(column, _)| column.name.as_str()))
        })
        .collect::<Vec<_>>();
    let templates = |name: &str, overrides: bool| {
        let rows = rows.iter().zip(&derived);
        rows.filter(|(_, derived)| derived.contains(name) == overrides)
            .map(|(template, _)| template.id.name.clone())
            .collect::<Vec<_>>()
    };
    if !rows.is_empty() {
        for (column, &name) in spec.columns.iter().zip(&names) {
            match &column.derivation {
                // Aggregate column results belong to the completed dataset and
                // cannot be promoted into row-local defaults (SPEC-6).
                Some(derivation) if aggregate(spec, derivation.expr) => {
                    let overridden = templates(name, true);
                    if !overridden.is_empty() {
                        findings.push(PreflightFinding::DuplicateRowDefault {
                            column: name.into(),
                            rows: overridden,
                            not_row_local: Verb::Aggregate.name().into(),
                        });
                    }
                }
                Some(_) => {}
                None => {
                    let missing = templates(name, false);
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
        findings.push(PreflightFinding::EmptyKeys);
    }
    for (position, key) in spec.keys.iter().enumerate() {
        let name = key.name.as_str();
        if spec.keys[..position].iter().any(|prior| prior.name == name) {
            findings.push(PreflightFinding::DuplicateKey {
                position,
                identifier: name.into(),
            });
        }
        if !declared.contains(name) {
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
        findings.push(PreflightFinding::ConflictingRowConstruction {
            rows: rows
                .iter()
                .map(|template| template.id.name.clone())
                .collect(),
        });
    }
    for (index, template) in rows.iter().enumerate() {
        let identity = template.id.name.as_str();
        if let Some(previous) = rows[..index]
            .iter()
            .position(|prior| prior.id.name == identity)
        {
            let mut finding = LookupFinding::new(
                &format!("{}.id", spec.cite(template.meta)),
                C::PreflightDuplicateRow,
            )
            .diagnostic();
            finding
                .spec_paths
                .push(format!("{}.id", spec.cite(rows[previous].meta)));
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
                row_path: spec.cite(template.meta),
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
                row_path: spec.cite(template.meta),
                row: identity.into(),
                groups,
            });
        } else if let Some(driver) = driver {
            for name in groups {
                if !name.starts_with(&format!("{driver}.")) || name.matches('.').count() != 1 {
                    findings.push(PreflightFinding::GroupReference {
                        row_path: spec.cite(template.meta),
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
        ExprKind::Unsupported(operation) => operation == Verb::Aggregate.name(),
        _ => false,
    }
}

/// Each input's declaration findings: its written path, the encoding its
/// profile requires, the empty-string convention, its profile, what an ODM
/// input may not declare and the types a Parquet or ODM XML input restates.
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
        if (!sas && input.source.encoding.is_some())
            || (profile == Some(ast::Profile::Xport) && input.source.encoding.is_none())
        {
            findings.push(PreflightFinding::Scalar(Diagnostic::new(
                C::SasSourceEncoding,
                vec![format!("input.{name}.encoding")],
                DiagnosticContext::from([(
                    "dataset".into(),
                    crate::json::Value::String(name.into()),
                )]),
            )));
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
                positional: input.source.written.contains("fixed_width"),
            });
        }
        findings.extend(input_key_findings(spec, input)?);
        // A positional input stores exactly its `fixed_width` fields
        // (CSV-18), so a `types` entry naming another is refused as its
        // decoded header refuses it, before any byte is read (INPUT-6).
        if !input.source.fixed_width.is_empty() && input.source.schema.is_none() {
            let unknown = crate::typed_csv::unknown_field(
                input
                    .source
                    .types
                    .iter()
                    .map(|(field, _)| field.name.as_str()),
                |field| {
                    input
                        .source
                        .fixed_width
                        .iter()
                        .any(|positional| positional.name.name == field)
                },
            );
            if let Some(field) = unknown {
                findings.push(PreflightFinding::Scalar(
                    crate::diagnostic::SourceFinding::UnknownField {
                        dataset: name,
                        field,
                    }
                    .diagnostic(),
                ));
            }
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
        // A generated ordinal may not take the name of a field the input
        // stores (INPUT-4); where the declarations alone decide every stored
        // field, it is refused before any byte is read.
        if let (Some(ordinal), None) = (&input.source.ordinal, &input.source.schema) {
            let stored = crate::specification::source_declaration(input)
                .stored_fields(crate::specification::SourceInterpretation::Ordinary);
            if stored
                .iter()
                .flatten()
                .any(|(field, _)| *field == ordinal.name)
            {
                findings.push(PreflightFinding::Scalar(
                    crate::diagnostic::SourceFinding::OrdinalCollision {
                        dataset: name,
                        field: &ordinal.name,
                    }
                    .diagnostic(),
                ));
            }
        }
        // A self-describing profile types every field it stores: Parquet by
        // its schema, and ODM XML by the fixed item table (INPUT-6, ODM-1).
        if matches!(profile, Some(ast::Profile::Parquet | ast::Profile::OdmXml))
            && !input.source.types.is_empty()
        {
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

/// A 1.0 input's declared `key` names at least one column, and each once,
/// as a named intermediate's list does (LOOKUP-5). Whether the input stores
/// each column, and whether the current row has it, is decided where an
/// implicit join or a keyless qualified aggregate reads the input, so a key
/// that no read uses is admitted (P-KEY-1).
fn input_key_findings(
    spec: &Spec,
    input: &ast::Input,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let ast::KeyDecl::Declared(columns) = &input.key else {
        return Ok(Vec::new());
    };
    let name = input.name.name.as_str();
    let path = format!("input.{name}.key");
    if columns.len() > Limits::KEYS {
        return Err(PrepareError::limit(
            "input_keys",
            Limits::KEYS,
            Some(columns.len()),
            Some(&path),
        ));
    }
    if columns.is_empty() {
        return Ok(vec![PreflightFinding::Scalar(
            crate::diagnostic::lookup::empty_key(
                &path,
                crate::diagnostic::lookup::Owner::Unnamed,
                name,
            ),
        )]);
    }
    let mut findings = Vec::new();
    for (position, column) in columns.iter().enumerate() {
        if columns[..position]
            .iter()
            .any(|prior| prior.name == column.name)
        {
            let mut finding =
                LookupFinding::new(&render(spec, column.meta), C::IntermediateRepeatedKey);
            finding.text("dataset", name);
            finding.text("identifier", &column.name);
            finding.text("expected", "each column once");
            findings.push(PreflightFinding::Scalar(finding.diagnostic()));
        }
    }
    Ok(findings)
}
