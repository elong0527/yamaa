//! Original flag declaration and post-capture binding; no predicate text is invented.
use super::*;
use crate::{
    dataset::{Flag, FlagBinding, FlagRead},
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    predicate::Plan,
    predicate_compiler,
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    condition: Result<Plan, Diagnostic>,
    path: String,
    true_value: crate::value::Value,
    false_value: Option<crate::value::Value>,
    missing: Option<crate::value::Value>,
}
fn finding(code: C, path: &str, context: crate::diagnostic::Context) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: vec![path.into()],
        context,
        source_span: None,
        operand_route: None,
    }
}
fn value_text(value: &str) -> V {
    V::Scalar(crate::value::Value::Str(value.into()))
}
impl Declaration {
    pub(super) fn has_findings(&self) -> bool {
        self.condition.is_err() || self.false_value.is_some() && self.missing.is_none()
    }
    pub(super) fn findings(&self, findings: &mut Vec<BindFinding>) {
        if let Err(diagnostic) = &self.condition {
            findings.push(BindFinding::Predicate(diagnostic.clone()));
        }
        if let Some(false_value) = self.false_value.as_ref().filter(|_| self.missing.is_none()) {
            findings.push(BindFinding::Predicate(finding(
                C::FlagMissingValueRequired,
                &format!("{}.missing", self.path),
                [("false_value".into(), V::Scalar(false_value.clone()))].into(),
            )));
        }
    }
}
fn condition(d: &Document, payload: usize) -> Result<&str, PrepareError> {
    if matches!(&d.nodes()[payload], N::Text(_)) {
        text(d, payload)
    } else {
        text(d, field(d, payload, "condition")?)
    }
}
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
) -> Result<Declaration, PrepareError> {
    let expression = condition(d, payload)?;
    let compiled = match predicate_compiler::compile(
        expression,
        &format!("{path}.condition"),
        Default::default(),
    ) {
        Ok(plan) => Ok(plan),
        Err(predicate_compiler::Error::Parse(crate::predicate_parser::ParseError::Grammar {
            position,
            failure,
            ..
        })) => {
            let mut diagnostic =
                failure.diagnostic(&format!("{path}.condition"), expression, position.character);
            // The flag's established public grammar context carries the original
            // predicate and character position. Portable parser pattern geometry
            // remains internal and generic predicate consumers keep their mapping.
            if matches!(
                failure,
                crate::predicate_parser::GrammarFailure::InvalidRegex { .. }
            ) {
                diagnostic.context.remove("pattern_byte");
                diagnostic.context.remove("reason");
            }
            Err(diagnostic)
        }
        Err(predicate_compiler::Error::Parse(
            crate::predicate_parser::ParseError::UnsupportedRegex { .. },
        )) => {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "predicate_regex".into(),
                path: format!("{path}.condition"),
            }]))
        }
        Err(predicate_compiler::Error::Internal) => return Err(PrepareError::Internal),
        Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
    };
    let branch = |name: &str| {
        d.field(payload, name)
            .map(|id| literal(d, id, &format!("{path}.{name}")))
            .transpose()
    };
    Ok(Declaration {
        condition: compiled,
        path: path.into(),
        true_value: branch("true_value")?.unwrap_or(crate::value::Value::Str("Y".into())),
        false_value: branch("false_value")?,
        missing: branch("missing")?,
    })
}
/// Admit the full authored occurrence tree before any flag is prepared or any
/// expanded flag finding/name/branch context is copied, including row defaults.
pub(super) fn declaration_admission(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
) -> Result<(), PrepareError> {
    fn visit(
        d: &Document,
        value: usize,
        path: &str,
        depth: usize,
        copies: usize,
        total: &mut usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 || path.len() > 65_536 {
            return Err(PrepareError::Limit("case_depth"));
        }
        if let Some(flag) = d.field(value, "flag") {
            let mut size = condition(d, flag)?
                .len()
                .checked_add(path.len())
                .and_then(|n| n.checked_add(256))
                .ok_or(PrepareError::Limit("flag_metadata"))?;
            for name in ["true_value", "false_value", "missing"] {
                if let Some(id) = d.field(flag, name) {
                    size = size
                        .checked_add(match &d.nodes()[id] {
                            N::Text(s) | N::Integer(s) => s.len(),
                            _ => 32,
                        })
                        .ok_or(PrepareError::Limit("flag_metadata"))?;
                }
            }
            *total = total
                .checked_add(
                    size.checked_mul(16)
                        .and_then(|n| n.checked_mul(copies))
                        .ok_or(PrepareError::Limit("flag_metadata"))?,
                )
                .filter(|&n| n <= crate::dataset::MAX_FLAG_METADATA_BYTES)
                .ok_or(PrepareError::Limit("flag_metadata"))?;
        }
        if let Some(case) = d.field(value, "case") {
            for (index, &branch) in sequence(d, case)?.iter().enumerate() {
                let (name, child) = d
                    .field(branch, "then")
                    .map(|id| ("then", id))
                    .or_else(|| d.field(branch, "otherwise").map(|id| ("otherwise", id)))
                    .ok_or(PrepareError::Internal)?;
                visit(
                    d,
                    child,
                    &format!("{path}.case[{index}].{name}"),
                    depth + 1,
                    copies,
                    total,
                )?;
            }
        }
        Ok(())
    }
    let mut total = 0;
    // Shared defaults may be prepared once for each declared template. Admit
    // that complete worst-case ownership before selecting row-local defaults.
    let copies = d
        .field(d.root(), "rows")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| sequence(d, id).map(|rows| rows.len().max(1)))
        .transpose()?
        .unwrap_or(1);
    for (index, &column) in columns.iter().enumerate() {
        if let Some(value) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            visit(
                d,
                value,
                &format!("columns.{}.derivation", output.columns()[index].name),
                0,
                copies,
                &mut total,
            )?;
        }
    }
    if let Some(rows) = d
        .field(d.root(), "rows")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        for (index, &row) in sequence(d, rows)?.iter().enumerate() {
            for &(name, derivation) in mapping(d, field(d, row, "derivations")?)? {
                visit(
                    d,
                    field(d, derivation, "value")?,
                    &format!("rows[{index}].derivations.{}", text(d, name)?),
                    0,
                    1,
                    &mut total,
                )?;
            }
        }
    }
    Ok(())
}
fn unknown(path: &str, name: &str) -> BindFinding {
    BindFinding::Predicate(finding(
        C::PredicateUnknownField,
        path,
        [("identifier".into(), value_text(name))].into(),
    ))
}
#[allow(clippy::too_many_arguments)]
fn output_binding(
    name: &str,
    path: &str,
    catalog: &Catalog,
    driver: &str,
    available: Option<&[usize]>,
    row: Option<&str>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<FlagRead>, BindError> {
    if let Some(cause) = catalog
        .validate_output(name, None, available, &[0])
        .map_err(BindError::Catalog)?
    {
        match cause {
            reference_binding::Diagnostic::UnknownField => findings.push(unknown(path, name)),
            reference_binding::Diagnostic::UnresolvableName { .. } => {
                findings.push(BindFinding::Predicate(finding(
                    C::PredicateUnresolvableName,
                    path,
                    [
                        ("identifier".into(), value_text(name)),
                        ("suggestion".into(), value_text(&format!("{driver}.{name}"))),
                    ]
                    .into(),
                )))
            }
            _ => findings.push(BindFinding::OutputReference {
                path: path.into(),
                name: name.into(),
                finding: cause,
            }),
        }
        let _ = row;
        return Ok(None);
    }
    let Some(reference_binding::Binding::Output { column, .. }) =
        catalog.bind(name).map_err(BindError::Catalog)?
    else {
        return Err(BindError::Internal);
    };
    edges.push(column);
    Ok(Some(FlagRead::Read(Read::Column(column))))
}
fn finish(
    operation: &Declaration,
    bindings: Vec<FlagBinding>,
) -> Result<Option<Expression>, BindError> {
    let Ok(condition) = &operation.condition else {
        return Ok(None);
    };
    if operation.has_findings() {
        return Ok(None);
    }
    Flag::new(
        condition.clone(),
        bindings,
        operation.true_value.clone(),
        operation.false_value.clone(),
        operation.missing.clone(),
    )
    .map(|flag| Some(Expression::Flag(flag)))
    .map_err(|_| BindError::Internal)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &Declaration,
    column: usize,
    keys: &[usize],
    driver: &str,
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
) -> Result<Option<Expression>, BindError> {
    if !declarations_reported {
        operation.findings(findings);
    }
    let Ok(condition) = &operation.condition else {
        return Ok(None);
    };
    let start = findings.len();
    let mut bindings = Vec::new();
    let mut seen = alloc::collections::BTreeSet::new();
    for name in condition.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        let read = if let Some((index, field)) = declarations.reference(name) {
            if let Some(item) = &intermediates[index] {
                edges.extend(row_intermediates::dependencies(index, intermediates)?);
                match item.source_column(secondary, field) {
                    Some(column) => Some(FlagRead::Read(Read::Intermediate { index, column })),
                    None => {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            condition.spec_path(),
                            name,
                            None,
                            ReferenceCause::Value,
                            None,
                        )));
                        continue;
                    }
                }
            } else {
                None
            }
        } else if !name.contains('.') {
            output_binding(
                name,
                condition.spec_path(),
                catalog,
                driver,
                None,
                None,
                edges,
                findings,
            )?
        } else {
            match catalog.bind(name).map_err(BindError::Catalog)? {
                Some(reference_binding::Binding::Dataset { field, .. }) => {
                    Some(if keys.contains(&column) {
                        FlagRead::Read(Read::Source(field))
                    } else {
                        FlagRead::Collect {
                            column: field,
                            identifier: name.into(),
                        }
                    })
                }
                _ => None,
            }
        };
        if let Some(read) = read {
            bindings.push(FlagBinding {
                name: name.into(),
                read,
            });
        } else if name.contains('.') {
            findings.push(unknown(condition.spec_path(), name));
        }
    }
    if findings.len() != start {
        Ok(None)
    } else {
        finish(operation, bindings)
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &Declaration,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    declarations_reported: bool,
) -> Result<Option<Expression>, BindError> {
    if !declarations_reported {
        operation.findings(findings);
    }
    let Ok(condition) = &operation.condition else {
        return Ok(None);
    };
    let start = findings.len();
    let mut bindings = Vec::new();
    let mut seen = alloc::collections::BTreeSet::new();
    for name in condition.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        let read = if !name.contains('.') {
            output_binding(
                name,
                condition.spec_path(),
                catalog,
                driver,
                available,
                row,
                edges,
                findings,
            )?
        } else {
            use crate::reference_scope::{self, Reach, Scope};
            let causes = reference_scope::validate(
                catalog,
                name,
                None,
                Scope {
                    drivers: &[driver],
                    current_driver: false,
                    reach: Reach::Scalar,
                    joined: false,
                    phase,
                },
                Default::default(),
            )
            .map_err(BindError::Catalog)?;
            if !causes.is_empty() {
                for cause in causes {
                    if cause == reference_scope::Finding::UnknownField {
                        findings.push(unknown(condition.spec_path(), name));
                    } else {
                        findings.push(BindFinding::QualifiedReference {
                            path: condition.spec_path().into(),
                            name: name.into(),
                            row: row.map(String::from),
                            finding: cause,
                        });
                    }
                }
                None
            } else {
                match catalog.bind(name).map_err(BindError::Catalog)? {
                    Some(reference_binding::Binding::Dataset { field, .. }) => {
                        Some(FlagRead::Read(Read::Source(field)))
                    }
                    _ => None,
                }
            }
        };
        if let Some(read) = read {
            bindings.push(FlagBinding {
                name: name.into(),
                read,
            });
        }
    }
    if findings.len() != start {
        Ok(None)
    } else {
        finish(operation, bindings)
    }
}
