//! Retained declaration findings share the compiled operations used by build.
//! Checking never obtains source schemas, activates functions, or executes a plan.
use super::*;
use crate::diagnostic::Diagnostic;

pub(super) fn predicate(
    compiled: &Result<crate::predicate::Plan, crate::predicate_compiler::Error>,
    expression: &str,
    path: &str,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    match compiled {
        Ok(_) => Ok(()),
        Err(crate::predicate_compiler::Error::Parse(
            crate::predicate_parser::ParseError::Grammar {
                position, failure, ..
            },
        )) => {
            findings.push(BindFinding::Lookup(LookupFinding::grammar(
                path,
                expression,
                position.character,
                failure,
            )));
            Ok(())
        }
        Err(error) => Err(BindError::PredicatePolicy(error.clone())),
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
    scope: Option<(&str, crate::reference_scope::Phase<'_>)>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    if let Some(Err(error)) = branch.predicate() {
        findings.push(BindFinding::Predicate(error.diagnostic()?));
    }
    derivations::declaration_findings(&branch.operation, &branch.path, findings)?;
    filters(&branch.operation, &branch.path, scope, None, findings)
}

fn operation(
    op: &Operation,
    path: &str,
    output: Option<&TableSchema>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    // Build already uses this declaration pass for flags, padding, nested cases,
    // numeric failures and concatenation operands. Keep exactly its paths/causes.
    derivations::declaration_findings(op, path, findings)?;
    filters(op, path, None, output, findings)
}

fn source_filter(
    source: &source_expressions::Declaration,
    path: &str,
    scope: Option<(&str, crate::reference_scope::Phase<'_>)>,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    if scope.is_some_and(|(driver, phase)| {
        source.reject_group_key_filter(path, driver, phase, findings)
    }) {
        return Ok(());
    }
    source.check_findings(path, findings)
}

fn filters(
    op: &Operation,
    path: &str,
    scope: Option<(&str, crate::reference_scope::Phase<'_>)>,
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
            window.check_findings(findings)?;
            if let Some(output) = output {
                window.check_output_types(output, findings);
            }
        }
        Operation::Aggregate(aggregate) => aggregate.check_findings(findings)?,
        Operation::Odm(odm) => odm.check_findings(findings)?,
        Operation::Scalar(scalar) => {
            if let scalar::Declaration::Concat { sources, .. } = &**scalar {
                for source in sources {
                    filters(&source.operation, &source.path, scope, output, findings)?;
                }
            }
        }
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
                .check_findings(&self.sources, &self.output, findings)?;
            for declaration in &self.declarations {
                operation(
                    &declaration.operation,
                    &declaration.path,
                    self.rows.is_none().then_some(&self.output),
                    findings,
                )?;
            }
            if let Some(rows) = &self.rows {
                rows.check_findings(findings)?;
            }
            Ok(())
        };
        collect(&mut findings).map_err(|error| match error {
            BindError::PredicatePolicy(error) => PrepareError::predicate(error, None),
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
