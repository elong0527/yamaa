//! SELF preparation uses authored phases and output metadata, never study ports.
use super::*;
use crate::{
    dataset::{Intermediate, SecondarySource},
    value::Value,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Relation {
    Input(usize),
    SelfRows,
}

/// SELF is a reserved donor relation and requires explicit row construction.
pub(super) fn declaration_findings(
    d: &Document,
    sources: &[SourceDeclaration],
    items: &[usize],
) -> Result<Vec<PreflightFinding>, PrepareError> {
    use crate::diagnostic::{ConditionCode as C, ContextValue, Diagnostic};
    let mut findings = Vec::new();
    let mut bytes = 0usize;
    for (index, &item) in items.iter().enumerate() {
        let dataset = text(d, field(d, item, "dataset")?)?;
        if dataset != "SELF" {
            continue;
        }
        let name = text(d, field(d, item, "id")?)?;
        let path = format!("intermediates[{index}].dataset");
        bytes = bytes
            .checked_add(path.len())
            .and_then(|bytes| bytes.checked_add(name.len()))
            .and_then(|bytes| bytes.checked_add(128))
            .filter(|&bytes| bytes <= 16_777_216)
            .ok_or(PrepareError::Limit("self_findings"))?;
        let (code, paths, context) = if sources.iter().any(|source| source.name == "SELF") {
            (
                C::IntermediateIdentifierCollision,
                vec![path, "input.SELF".into()],
                vec![(
                    "identifier".into(),
                    ContextValue::Scalar(Value::Str("SELF".into())),
                )],
            )
        } else if !present(d, d.root(), "rows") {
            (
                C::LookupSelectionReference,
                vec![path],
                vec![
                    (
                        "intermediate".into(),
                        ContextValue::Scalar(Value::Str(name.into())),
                    ),
                    (
                        "identifier".into(),
                        ContextValue::Scalar(Value::Str("SELF".into())),
                    ),
                ],
            )
        } else {
            continue;
        };
        findings.push(PreflightFinding::Scalar(Diagnostic {
            code,
            spec_paths: paths,
            context: context.into_iter().collect(),
            source_span: None,
            operand_route: None,
        }));
    }
    Ok(findings)
}

/// Donor reads and selector fields promote row-local defaults through the normal graph.
pub(super) fn donor_defaults(
    d: &Document,
    output: &TableSchema,
    project: Option<&functions::Prepared>,
    named: &intermediates::Declarations,
    templates: &str_template::Prepared,
) -> Result<alloc::collections::BTreeSet<usize>, PrepareError> {
    let mut fields = alloc::collections::BTreeSet::new();
    let mut register = |field: &str| {
        if let Some(column) = output
            .columns()
            .iter()
            .position(|column| column.name == field)
        {
            fields.insert(column);
        }
    };
    for field in named.self_donor_fields() {
        register(field);
    }
    let mut inspect = |declaration: usize| -> Result<(), PrepareError> {
        let mut names = Vec::new();
        derivations::references(d, field(d, declaration, "value")?, project, &mut names, 0)?;
        template_references(d, field(d, declaration, "value")?, templates, &mut names, 0)?;
        for name in names {
            if let Some((index, field)) = named.reference(&name) {
                if named.is_self(index) {
                    register(field);
                }
            }
        }
        Ok(())
    };
    for &column in sequence(d, field(d, d.root(), "columns")?)? {
        if let Some(derivation) = d
            .field(column, "derivation")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            inspect(derivation)?;
        }
    }
    for &row in sequence(d, field(d, d.root(), "rows")?)? {
        for &(_, declaration) in mapping(d, field(d, row, "derivations")?)? {
            inspect(declaration)?;
        }
    }
    Ok(fields)
}

/// Supplemental template placeholders share the already-admitted parser metadata.
pub(super) fn template_references(
    d: &Document,
    value: usize,
    templates: &str_template::Prepared,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > 64 {
        return Err(PrepareError::Limit("expression_depth"));
    }
    if let Some(payload) = d.field(value, "str_template") {
        names.extend(templates.get(payload)?.identifiers().iter().cloned());
    }
    if let Some(payload) = d.field(value, "case") {
        for &branch in sequence(d, payload)? {
            if let Some(child) = d
                .field(branch, "then")
                .or_else(|| d.field(branch, "otherwise"))
            {
                template_references(d, child, templates, names, depth + 1)?;
            }
        }
    }
    if let Some(payload) = d.field(value, "str_concat") {
        for &child in sequence(d, field(d, payload, "sources")?)? {
            template_references(d, child, templates, names, depth + 1)?;
        }
    }
    Ok(())
}

/// Bound all diagnostic copies before retaining a first-template refusal.
pub(super) struct FirstTemplate<'a> {
    d: &'a Document,
    project: Option<&'a functions::Prepared>,
    named: &'a intermediates::Declarations,
    templates: &'a str_template::Prepared,
    findings: Vec<PreflightFinding>,
    bytes: usize,
}
impl<'a> FirstTemplate<'a> {
    pub(super) fn new(
        d: &'a Document,
        project: Option<&'a functions::Prepared>,
        named: &'a intermediates::Declarations,
        templates: &'a str_template::Prepared,
    ) -> Self {
        Self {
            d,
            project,
            named,
            templates,
            findings: Vec::new(),
            bytes: 0,
        }
    }
    pub(super) fn finish(self) -> Vec<PreflightFinding> {
        self.findings
    }
    fn register(&mut self, name: String, path: &str) -> Result<(), PrepareError> {
        if !self
            .named
            .reference(&name)
            .is_some_and(|(index, _)| self.named.is_self(index))
        {
            return Ok(());
        }
        let cost = path
            .len()
            .checked_add(name.len())
            .and_then(|bytes| bytes.checked_add(128))
            .and_then(|bytes| bytes.checked_mul(4))
            .ok_or(PrepareError::Limit("self_findings"))?;
        self.bytes = self
            .bytes
            .checked_add(cost)
            .filter(|&bytes| bytes <= 16_777_216)
            .ok_or(PrepareError::Limit("self_findings"))?;
        if self.findings.len() >= 65_536 {
            return Err(PrepareError::Limit("self_findings"));
        }
        self.findings
            .push(PreflightFinding::Scalar(crate::diagnostic::Diagnostic {
                code: crate::diagnostic::ConditionCode::IntermediateSelfPhaseBoundary,
                source_span: None,
                operand_route: None,
                spec_paths: vec![path.into()],
                context: [
                    (
                        "identifier".into(),
                        crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(name)),
                    ),
                    (
                        "available_phase".into(),
                        crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(
                            "completed_prior_row_template".into(),
                        )),
                    ),
                    (
                        "required_phase".into(),
                        crate::diagnostic::ContextValue::Scalar(crate::value::Value::Str(
                            "row_construction".into(),
                        )),
                    ),
                ]
                .into_iter()
                .collect(),
            }));
        Ok(())
    }
    /// Include unselected nested alternatives; first-template admission is static.
    pub(super) fn visit(
        &mut self,
        value: usize,
        prefix: &str,
        depth: usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 || prefix.len() > 65_536 {
            return Err(PrepareError::Limit("self_read_metadata"));
        }
        let d = self.d;
        let &[(operation, payload)] = mapping(d, value)? else {
            return Err(PrepareError::Internal);
        };
        let operation = text(d, operation)?;
        let path = format!("{prefix}.{operation}");
        if operation == "case" {
            for (index, &branch) in sequence(d, payload)?.iter().enumerate() {
                if let Some(condition) = d.field(branch, "when") {
                    if let Ok(parsed) = crate::predicate_parser::parse_predicate(
                        text(d, condition)?,
                        Default::default(),
                    ) {
                        for name in parsed.identifiers() {
                            self.register(name.into(), &format!("{path}[{index}].when"))?;
                        }
                    }
                }
                if let Some(child) = d
                    .field(branch, "then")
                    .or_else(|| d.field(branch, "otherwise"))
                {
                    let alternative = if d.field(branch, "then").is_some() {
                        "then"
                    } else {
                        "otherwise"
                    };
                    self.visit(child, &format!("{path}[{index}].{alternative}"), depth + 1)?;
                }
            }
            return Ok(());
        }
        if operation == "str_concat" {
            for (index, &child) in sequence(d, field(d, payload, "sources")?)?
                .iter()
                .enumerate()
            {
                self.visit(child, &format!("{path}.sources[{index}]"), depth + 1)?;
            }
            return Ok(());
        }
        if matches!(operation, "first_available" | "greatest" | "least") {
            for (index, &source) in sequence(d, field(d, payload, "sources")?)?
                .iter()
                .enumerate()
            {
                let name = match &d.nodes()[source] {
                    N::Text(name) => name.as_str(),
                    _ => text(d, field(d, source, "variable")?)?,
                };
                self.register(name.into(), &format!("{path}.sources[{index}]"))?;
            }
            return Ok(());
        }
        let mut names = Vec::new();
        derivations::references(d, value, self.project, &mut names, 0)?;
        template_references(d, value, self.templates, &mut names, 0)?;
        for name in names {
            self.register(
                name,
                &if operation == "compute" {
                    format!("{path}.expr")
                } else {
                    path.clone()
                },
            )?;
        }
        Ok(())
    }
}

/// One named-read hook shared by row numeric, cases and first-available operands.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_operand(
    reference: &str,
    path: &str,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
    output: &TableSchema,
    named: &intermediates::Declarations,
    secondary: &[SecondarySource],
    items: &[Option<Intermediate>],
) -> Result<Option<Expression>, BindError> {
    if named.reference(reference).is_some()
        && reference
            .split_once('.')
            .is_none_or(|(owner, _)| owner != driver)
    {
        row_intermediates::bind(
            reference, path, named, secondary, items, output, catalog, available, edges, findings,
        )
    } else {
        rounding::bind_row_operand(
            reference, path, catalog, driver, row, phase, available, edges, findings,
        )
    }
}

/// A default reading prior completed templates cannot run in the first template.
pub(super) fn is_reference(d: &Document, name: &str) -> Result<bool, PrepareError> {
    let Some((relation, _)) = name.split_once('.') else {
        return Ok(false);
    };
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(false);
    };
    for &item in sequence(d, items)? {
        if text(d, field(d, item, "id")?)? == relation
            && text(d, field(d, item, "dataset")?)? == "SELF"
        {
            return Ok(true);
        }
    }
    Ok(false)
}
