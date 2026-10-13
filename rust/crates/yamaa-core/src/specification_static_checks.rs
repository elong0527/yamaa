//! Retained declaration findings share the compiled operations used by build.
//! Checking never obtains source schemas, activates functions, or executes a plan.
use super::*;
use crate::diagnostic::Diagnostic;

fn charge(used: &mut usize, amount: usize) -> Result<(), PrepareError> {
    *used = used
        .checked_add(amount)
        .filter(|&n| n <= 16_777_216)
        .ok_or(PrepareError::Limit("check_declaration_diagnostics"))?;
    Ok(())
}
fn context_bytes(
    value: &crate::diagnostic::ContextValue,
    used: &mut usize,
) -> Result<(), PrepareError> {
    use crate::{diagnostic::ContextValue as V, value::Value};
    charge(used, core::mem::size_of::<V>())?;
    match value {
        V::Scalar(Value::Str(text)) | V::Integer(text) => charge(used, text.len())?,
        V::Sequence(values) => {
            for value in values {
                context_bytes(value, used)?;
            }
        }
        V::Object(fields) => {
            for (name, value) in fields {
                charge(used, name.len())?;
                context_bytes(value, used)?;
            }
        }
        _ => (),
    }
    Ok(())
}

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
            )?));
            Ok(())
        }
        Err(error) => Err(BindError::PredicatePolicy(error.clone())),
    }
}

pub(super) fn branch(
    branch: &cases::Branch,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    if let Some(Err(error)) = branch.predicate() {
        findings.push(BindFinding::Predicate(error.diagnostic()?));
    }
    operation(&branch.operation, &branch.path, findings)
}

fn operation(op: &Operation, path: &str, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
    // Build already uses this declaration pass for flags, padding, nested cases,
    // numeric failures and concatenation operands. Keep exactly its paths/causes.
    derivations::declaration_findings(op, path, findings)?;
    filters(op, path, findings)
}

fn filters(op: &Operation, path: &str, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
    match op {
        Operation::Case(branches) => {
            for item in branches {
                filters(&item.operation, &item.path, findings)?;
            }
        }
        Operation::Source(source) => source.check_findings(path, findings)?,
        Operation::FirstAvailable(selection) => {
            for (index, source) in selection.sources.iter().enumerate() {
                source.check_findings(&format!("{path}.sources[{index}]"), findings)?;
            }
        }
        Operation::MappingCut(mapping) => mapping
            .source
            .check_findings(&format!("{path}.source"), findings)?,
        Operation::Window(window) => window.check_findings(findings)?,
        Operation::Aggregate(aggregate) => aggregate.check_findings(findings)?,
        Operation::Odm(odm) => odm.check_findings(findings)?,
        Operation::Scalar(scalar) => {
            if let scalar::Declaration::Concat { sources, .. } = &**scalar {
                for source in sources {
                    filters(&source.operation, &source.path, findings)?;
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
            self.intermediates.check_findings(findings)?;
            for declaration in &self.declarations {
                operation(&declaration.operation, &declaration.path, findings)?;
            }
            if let Some(rows) = &self.rows {
                rows.check_findings(findings)?;
            }
            Ok(())
        };
        collect(&mut findings).map_err(|error| match error {
            BindError::Internal => PrepareError::Internal,
            _ => PrepareError::Limit("check_declaration_diagnostics"),
        })?;
        let mut bytes = 0usize;
        for finding in findings {
            let diagnostics = finding
                .diagnostics(self.source())
                .ok_or(PrepareError::Internal)?;
            for diagnostic in diagnostics {
                // Charge all retained path and context text before extending the collection.
                // Source-model and parser admission bound each individual finding already.
                charge(&mut bytes, core::mem::size_of::<Diagnostic>())?;
                for path in &diagnostic.spec_paths {
                    charge(&mut bytes, path.len())?;
                }
                for (name, value) in &diagnostic.context {
                    charge(&mut bytes, name.len())?;
                    context_bytes(value, &mut bytes)?;
                }
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
        self.check_declaration_findings()
            .map(VerificationDeclarationFinding::diagnostic)
            .collect()
    }
}
