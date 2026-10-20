//! Retained declaration findings share the compiled operations used by build.
//! Checking never obtains source schemas, activates functions, or executes a plan.
use super::*;
use crate::{diagnostic::Diagnostic, reference_scope::Phase};

/// The rows a derivation evaluates on, as binding scopes its reads.
#[derive(Clone, Copy, Debug)]
pub(super) struct Scope<'a> {
    /// The dataset whose records build those rows, once for each binding:
    /// the driver without row templates, a row template's own driver, or in
    /// the column phase the driver of each binding the derivation gets
    /// (`Rows::column_drivers`).
    pub(super) drivers: &'a [&'a str],
    /// The row template of a row-phase derivation.
    pub(super) row: Option<&'a str>,
    /// The grouping phase under row templates; without them there is none.
    pub(super) phase: Option<Phase<'a>>,
}

/// The static types of one derivation over the declared types, as binding
/// checks them: each flag condition and case predicate (FORMULA-5,
/// FORMULA-12), and each aggregate derive binding's conversion (VALUE-24).
pub(super) fn derivation_types(
    operation: &Operation,
    tree: &types::Tree,
    declared: &types::DeclaredTypes<'_>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    let check = |plan: &CompiledExpression, findings: &mut Vec<BindFinding>| {
        findings.extend(
            types::predicate_kinds(plan, |name| declared.name(name).single())
                .into_iter()
                .map(BindFinding::Predicate),
        );
    };
    match operation {
        Operation::Flag(flag) => {
            if let Some(condition) = flag.condition() {
                check(condition, findings);
            }
        }
        Operation::Case(branches) => {
            for branch in branches {
                if let Some(Ok(plan)) = branch.predicate() {
                    check(plan, findings);
                }
                derivation_types(&branch.operation, tree, declared, findings)?;
            }
        }
        Operation::Aggregate(aggregate) => {
            aggregate.derive_conversion_findings(tree, declared, findings)?
        }
        _ => (),
    }
    Ok(())
}

pub(super) fn predicate(
    compiled: &Result<CompiledExpression, CompileError>,
    expression: &str,
    path: &str,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    match compiled {
        Ok(_) => Ok(()),
        Err(CompileError::Invalid {
            position, failure, ..
        }) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                path,
                expression,
                position.character,
                failure,
            )));
            Ok(())
        }
        Err(error) => Err(BindError::ExpressionPolicy(error.clone())),
    }
}

pub(super) fn branch(
    branch: &cases::Branch,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    scoped_branch(branch, None, findings)
}

pub(super) fn scoped_branch(
    branch: &cases::Branch,
    scope: Option<Scope<'_>>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    if let Some(Err(error)) = branch.predicate() {
        findings.push(BindFinding::Predicate(error.diagnostic()?));
    }
    derivations::declaration_findings(&branch.operation, &branch.path, findings)?;
    filters(&branch.operation, &branch.path, scope, None, findings)
}

/// FORMULA-9: a qualified formula name must name a declared record lookup.
/// The test reads declarations alone, so binding and check share it.
pub(super) fn qualified_numeric_findings(
    compiled: &CompiledExpression,
    path: &str,
    intermediates: &intermediates::Declarations,
    findings: &mut Vec<BindFinding>,
) {
    for identifier in compiled
        .identifiers()
        .filter(|name| name.contains('.') && intermediates.reference(name).is_none())
    {
        findings.push(BindFinding::QualifiedNumericReference {
            path: path.into(),
            expression: compiled.expression().into(),
            identifier: identifier.into(),
        });
    }
}

fn operation(
    op: &Operation,
    path: &str,
    scope: Scope<'_>,
    output: Option<&TableSchema>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    // Build already uses this declaration pass for flags, padding, nested cases
    // and numeric failures. Keep exactly its paths/causes.
    derivations::declaration_findings(op, path, findings)?;
    filters(op, path, Some(scope), output, findings)
}

fn source_filter(
    source: &source_expressions::Declaration,
    path: &str,
    scope: Option<Scope<'_>>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    // A row-phase read's driver is its row template's own; the column phase
    // decides from the groups alone.
    if let Some(Scope {
        drivers,
        phase: Some(phase),
        ..
    }) = scope
    {
        let driver = drivers.first().copied().unwrap_or_default();
        if source.reject_group_key_filter(path, driver, phase, findings) {
            return Ok(());
        }
    }
    source.check_findings(path, findings)
}

fn filters(
    op: &Operation,
    path: &str,
    scope: Option<Scope<'_>>,
    output: Option<&TableSchema>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    match op {
        Operation::Case(branches) => {
            for item in branches {
                filters(&item.operation, &item.path, scope, output, findings)?;
            }
        }
        Operation::Source(source) => source_filter(source, path, scope, findings)?,
        Operation::FirstAvailable(selection) => {
            for (index, source) in selection.sources.iter().enumerate() {
                source_filter(source, &format!("{path}.sources[{index}]"), scope, findings)?;
            }
        }
        Operation::MappingCut(mapping) => {
            source_filter(&mapping.source, &format!("{path}.source"), scope, findings)?
        }
        Operation::Window(window) => {
            window.check_findings(scope, findings)?;
            if let Some(output) = output {
                window.check_output_types(output, findings);
            }
        }
        Operation::Aggregate(aggregate) => aggregate.check_findings(findings)?,
        Operation::Odm(odm) => odm.check_findings(findings)?,
        _ => (),
    }
    Ok(())
}

impl PreparedSpecification {
    pub(super) fn finish_checks(mut self) -> Result<Self, PrepareError> {
        let mut findings = Vec::new();
        let collect = |findings: &mut Vec<BindFinding>| -> Result<(), BindError> {
            if let Some(filter) = &self.root_filter {
                filter.check_findings(findings)?;
            }
            self.intermediates
                .check_findings(&self.sources, &self.output, &self.tree, findings)?;
            let outputs = self.rows.is_none().then(|| {
                self.output
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(index, column)| (column.name.as_str(), index))
                    .collect::<BTreeMap<_, _>>()
            });
            let declared = self.declared_types();
            // Without row templates, the driver's records build every row.
            let driver = [self.source().name.as_str()];
            let scope = Scope {
                drivers: &driver,
                row: None,
                phase: None,
            };
            let mut dependencies = Vec::new();
            for (column, declaration) in self.declarations.iter().enumerate() {
                let start = findings.len();
                // Binding reports a key that reads other records first and
                // binds nothing else of it (COLUMN-7).
                let refused = self.key_record_findings(column);
                if !refused.is_empty() {
                    findings.extend(refused);
                    if outputs.is_some() {
                        dependencies.push(Some(Vec::new()));
                    }
                    continue;
                }
                operation(
                    &declaration.operation,
                    &declaration.path,
                    scope,
                    self.rows.is_none().then_some(&self.output),
                    findings,
                )?;
                if let Some(outputs) = &outputs {
                    dependencies.push(Some(self.formula_reads(declaration, outputs, findings)));
                }
                derivation_types(&declaration.operation, &self.tree, &declared, findings)?;
                if findings.len() == start && declaration.handler.is_none() {
                    let result =
                        self.tree
                            .result_type(declaration.value, ast::Scope::Row, &declared)?;
                    findings.extend(
                        self.conversion_finding(column, result)
                            .map(BindFinding::Scalar),
                    );
                }
            }
            if outputs.is_some() {
                self.formula_dependencies(&dependencies, findings)?;
            }
            if let Some(rows) = &self.rows {
                rows.check_findings(
                    &self.output,
                    &self.source().name,
                    &self.tree,
                    &declared,
                    findings,
                )?;
            }
            // Binding admits range values once every row's reads are bound.
            lookup_range::check(
                self.range_contexts(),
                &self.intermediates,
                &self.sources,
                self.project_calls.as_ref(),
                findings,
            );
            Ok(())
        };
        collect(&mut findings).map_err(|error| match error {
            BindError::ExpressionPolicy(error) => PrepareError::expression(error, None),
            _ => PrepareError::Internal,
        })?;
        for finding in findings {
            let diagnostics = finding
                .diagnostics(self.source())
                .ok_or(PrepareError::Internal)?;
            for diagnostic in diagnostics {
                self.check_expressions.push(diagnostic);
            }
        }
        Ok(self)
    }

    /// The contexts whose rows read lookup ranges ([`lookup_range::check`]).
    /// Without row templates the base drives every row, and its qualifier
    /// reads the row's own record. A dataset verification reads its lookups
    /// for every row, as a column does.
    fn range_contexts(&self) -> Vec<lookup_range::Declared<'_>> {
        let verified = relation_predicates::verification_predicates(&self.verifications.checks)
            .flat_map(crate::bound_expression::BoundPredicate::bindings)
            .filter(|binding| {
                matches!(
                    binding.read,
                    crate::bound_expression::Read::Intermediate { .. }
                )
            })
            .map(|binding| lookup_range::Evaluated::Name(&binding.name));
        match &self.rows {
            Some(rows) => rows.range_contexts(&self.sources, &self.intermediates, verified),
            None => {
                let base = self.source().name.as_str();
                vec![lookup_range::Declared {
                    evaluated: self
                        .declarations
                        .iter()
                        .map(|declaration| {
                            lookup_range::Evaluated::Operation(&declaration.operation, Some(base))
                        })
                        .chain(verified)
                        .collect(),
                    drivers: vec![base],
                }]
            }
        }
    }

    /// A formula binds without a source schema: a qualified name is a declared
    /// record lookup or refused, and a bare name reads an output column. Return
    /// the output columns it reads, after its qualification findings, as
    /// binding does. Other derivation kinds contribute no columns here.
    fn formula_reads(
        &self,
        declaration: &Declaration,
        outputs: &BTreeMap<&str, usize>,
        findings: &mut Vec<BindFinding>,
    ) -> Vec<usize> {
        let Operation::Compute(compiled) = &declaration.operation else {
            return Vec::new();
        };
        let path = format!("{}.expr", declaration.path);
        qualified_numeric_findings(compiled, &path, &self.intermediates, findings);
        compiled
            .identifiers()
            .filter(|name| !name.contains('.'))
            .filter_map(|name| outputs.get(name).copied())
            .collect()
    }

    /// Column-order findings over formula reads alone (COLUMN-5, COLUMN-6,
    /// COLUMN-7), projected with binding's names and paths. Build analyzes
    /// every derivation kind, so it can also report what passes here.
    fn formula_dependencies(
        &self,
        dependencies: &[Option<Vec<usize>>],
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let analysis = column_dependencies::analyze(dependencies, &self.keys, false)
            .map_err(BindError::DependencyPolicy)?;
        if !analysis.diagnostics.is_empty() {
            findings.push(BindFinding::Dependencies {
                columns: self
                    .output
                    .columns()
                    .iter()
                    .map(|column| column.name.clone())
                    .collect(),
                paths: self
                    .declarations
                    .iter()
                    .map(|declaration| declaration.path.clone())
                    .collect(),
                diagnostics: analysis.diagnostics,
            });
        }
        Ok(())
    }

    /// All intrinsically known declaration findings, without study binding.
    /// Actual dataset fields and values remain a build-time responsibility.
    pub fn check_declaration_findings(
        &self,
    ) -> impl Iterator<Item = VerificationDeclarationFinding<'_>> {
        self.check_expressions
            .iter()
            .map(VerificationDeclarationFinding::Diagnostic)
            .chain(self.verification_declaration_findings())
    }

    pub fn check_declaration_diagnostics(&self) -> Vec<Diagnostic> {
        // Output-declaration findings are intrinsically known too, so `check`
        // reports the same findings `domain` reaches at its output stage.
        // They follow the declaration findings, matching build order: output
        // declaration checks come after derivation, key checks and
        // verification, so a first issue agrees between the two routes.
        let mut diagnostics = self
            .check_declaration_findings()
            .map(VerificationDeclarationFinding::diagnostic)
            .collect::<Vec<_>>();
        diagnostics.extend(self.output_findings().iter().map(OutputFinding::diagnostic));
        diagnostics
    }

    /// Captured declaration types can refuse a donor filter even when source
    /// schemas are unavailable. Other declaration checkpoints remain separate.
    pub fn intermediate_filter_type_diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.check_expressions
            .iter()
            .filter(|finding| self.intermediates.owns_filter_type_finding(finding))
    }
}
