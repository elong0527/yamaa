//! SELF preparation uses authored phases and output metadata, never study ports.
use super::*;
use crate::dataset::{Intermediate, SecondarySource};

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
    use crate::diagnostic::{ConditionCode as C, Diagnostic};
    use crate::json::Value as JsonValue;
    let mut findings = Vec::new();

    for (index, &item) in items.iter().enumerate() {
        let dataset = text(d, field(d, item, "dataset")?)?;
        if dataset != "SELF" {
            continue;
        }
        let name = text(d, field(d, item, "id")?)?;
        let path = format!("intermediates[{index}].dataset");
        let (code, paths, context) = if sources.iter().any(|source| source.name == "SELF") {
            (
                C::IntermediateIdentifierCollision,
                vec![path, "input.SELF".into()],
                vec![("identifier".into(), JsonValue::String("SELF".into()))],
            )
        } else if !present(d, d.root(), "rows") {
            (
                C::LookupSelectionReference,
                vec![path],
                vec![
                    ("intermediate".into(), JsonValue::String(name.into())),
                    ("identifier".into(), JsonValue::String("SELF".into())),
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

/// Supplemental template placeholders share the already-admitted parser metadata.
pub(super) fn template_references(
    d: &Document,
    value: usize,
    templates: &str_template::Prepared,
    names: &mut Vec<String>,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            None,
        ));
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
    Ok(())
}

/// Retain every invalid first-template SELF read in declaration order.
pub(super) struct FirstTemplate<'a> {
    d: &'a Document,
    project: Option<&'a functions::Prepared>,
    named: &'a intermediates::Declarations,
    templates: &'a str_template::Prepared,
    findings: Vec<PreflightFinding>,
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
        }
    }
    pub(super) fn finish(self) -> Vec<PreflightFinding> {
        self.findings
    }
    /// The first template's row `filter` reads for that template's rows, so
    /// a `SELF` read there, directly or through `between` values, finds no
    /// completed row (LOOKUP-7).
    pub(super) fn filter(&mut self, plan: &CompiledExpression, path: &str) {
        let mut seen = std::collections::BTreeSet::new();
        for name in plan.identifiers() {
            if seen.insert(name) {
                self.register(name.into(), path);
            }
        }
    }
    fn register(&mut self, name: String, path: &str) {
        // Reading another selector also runs the selector its `between.value`
        // names, for the same row, so the read reaches SELF through each such
        // value. Report the read, then each value in the order followed.
        let mut paths = vec![path.to_owned()];
        let mut followed = Vec::new();
        let mut reached = name;
        loop {
            let Some((index, _)) = self.named.reference(&reached) else {
                return;
            };
            if self.named.is_self(index) {
                break;
            }
            // Binding reports a value that reaches back to its own selector.
            let Some((value, between)) = self.named.range_value(index) else {
                return;
            };
            if followed.contains(&index) {
                return;
            }
            followed.push(index);
            paths.push(between);
            reached = value.into();
        }
        self.findings
            .push(PreflightFinding::Scalar(crate::diagnostic::Diagnostic {
                code: crate::diagnostic::ConditionCode::IntermediateSelfPhaseBoundary,
                source_span: None,
                operand_route: None,
                spec_paths: paths,
                context: [
                    ("identifier".into(), crate::json::Value::String(reached)),
                    (
                        "available_phase".into(),
                        crate::json::Value::String("completed_prior_row_template".into()),
                    ),
                    (
                        "required_phase".into(),
                        crate::json::Value::String("row_construction".into()),
                    ),
                ]
                .into_iter()
                .collect(),
            }));
    }
    /// Include unselected nested alternatives; first-template admission is static.
    pub(super) fn visit(
        &mut self,
        value: usize,
        prefix: &str,
        depth: usize,
    ) -> Result<(), PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(depth),
                Some(prefix),
            ));
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
                    if let Ok(compiled) =
                        compile(text(d, condition)?, ExpressionContext::Predicate, "")
                    {
                        for name in compiled.identifiers() {
                            self.register(name.into(), &format!("{path}[{index}].when"));
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
        if matches!(operation, "first_available" | "greatest" | "least") {
            for (index, &source) in sequence(d, field(d, payload, "sources")?)?
                .iter()
                .enumerate()
            {
                let name = match &d.nodes()[source] {
                    N::Text(name) => name.as_str(),
                    _ => text(d, field(d, source, "variable")?)?,
                };
                self.register(name.into(), &format!("{path}.sources[{index}]"));
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
            );
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
            reference, path, named, secondary, items, output, catalog, row, available, edges,
            findings,
        )
    } else {
        rounding::bind_row_operand(
            reference, path, catalog, driver, row, phase, available, edges, findings,
        )
    }
}
