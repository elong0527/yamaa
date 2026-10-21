//! Project admission findings retain their typed input and authored field paths.
//! Resource exhaustion and host failures remain outside semantic vocabulary.
//!
//! A finding in a 0.5 environment file or one of its definitions fails as
//! `project_environment_invalid`, at the environment file's path to it
//! (`functions.<name>...`, `lock`). One in a 1.0 function file fails as
//! `function_file_invalid` at the function file's own path to it, the lock
//! at the project's `environments.<host>.lock`, with the same reasons and
//! context (FUNC-2, FUNC-3; P-FN-2).
use crate::json::Value as JsonValue;
use crate::{
    ast::{self, Surface},
    diagnostic::{ConditionCode as C, Diagnostic},
    project_environment::{Draft, Finding, LockKind},
    project_function::{Finding as F, Language},
    project_terminology::{Fault, Finding as T},
    value::Value,
};
use std::vec;

pub mod projection;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("project environment diagnostics invalid index")]
    InvalidIndex,
    #[error("project environment diagnostics unavailable shape")]
    UnavailableShape,
}

/// Project scalar failures with the written text each finding keeps. Integer
/// values outside runtime i64 keep their exact mathematical decimal text.
/// A normalized-shape contradiction has no fabricated language condition.
pub fn scalar_diagnostics(
    findings: &[crate::project_function_document::Finding],
) -> Result<Vec<Diagnostic>, Error> {
    let mut project = Owned;
    findings
        .iter()
        .map(|finding| scalar_diagnostic(finding, C::ProjectEnvironmentInvalid, &mut project))
        .collect()
}

/// The scalar failures of a 1.0 function file, each at its own path in that
/// file (FUNC-2).
pub fn function_file_scalar_diagnostics(
    findings: &[crate::project_function_document::Finding],
) -> Result<Vec<Diagnostic>, Error> {
    let mut project = Owned;
    findings
        .iter()
        .map(|finding| scalar_diagnostic(finding, C::FunctionFileInvalid, &mut project))
        .collect()
}

// One field selector feeds owned diagnostics and retained borrowed reports.
trait Project<'a> {
    type Output;
    fn start(&mut self, code: C, path: &'a str) -> Result<Self::Output, Error>;
    fn append(&mut self, d: &mut Self::Output, text: &'a str) -> Result<(), Error>;
    fn index(&mut self, d: &mut Self::Output, index: usize) -> Result<(), Error>;
    fn context(
        &mut self,
        d: &mut Self::Output,
        key: &'static str,
        text: &'a str,
    ) -> Result<(), Error>;
    fn scalar(
        &mut self,
        d: &mut Self::Output,
        key: &'static str,
        value: &'a Value,
    ) -> Result<(), Error>;
    fn integer(
        &mut self,
        d: &mut Self::Output,
        key: &'static str,
        text: &'a str,
    ) -> Result<(), Error>;
}
fn scalar_diagnostic<'a, B: Project<'a>>(
    finding: &'a crate::project_function_document::Finding,
    code: C,
    project: &mut B,
) -> Result<B::Output, Error> {
    use crate::project_function_document::Kind;
    let reason = match finding.kind {
        Kind::IntegerRange => "integer_out_of_range",
        Kind::InvalidDate => "invalid_date",
        Kind::InvalidDateTime => "invalid_datetime",
        Kind::NormalizedShape => return Err(Error::UnavailableShape),
    };
    let value = finding.value.as_deref().ok_or(Error::UnavailableShape)?;
    let mut d = project.start(code, &finding.path)?;
    project.context(&mut d, "reason", reason)?;
    match finding.kind {
        Kind::IntegerRange => project.integer(&mut d, "value", value)?,
        _ => project.context(&mut d, "value", value)?,
    }
    Ok(d)
}
struct Owned;
impl<'a> Project<'a> for Owned {
    type Output = Diagnostic;
    fn start(&mut self, code: C, path: &'a str) -> Result<Diagnostic, Error> {
        Ok(Diagnostic {
            code,
            spec_paths: vec![path.into()],
            context: Default::default(),
            source_span: None,
            operand_route: None,
        })
    }
    fn append(&mut self, d: &mut Diagnostic, text: &'a str) -> Result<(), Error> {
        d.spec_paths[0].push_str(text);
        Ok(())
    }
    fn index(&mut self, d: &mut Diagnostic, index: usize) -> Result<(), Error> {
        let text = format!("[{index}]");
        d.spec_paths[0].push_str(&text);
        Ok(())
    }
    fn context(
        &mut self,
        d: &mut Diagnostic,
        key: &'static str,
        text: &'a str,
    ) -> Result<(), Error> {
        d.context.insert(key.into(), JsonValue::String(text.into()));
        Ok(())
    }
    fn scalar(
        &mut self,
        d: &mut Diagnostic,
        key: &'static str,
        value: &'a Value,
    ) -> Result<(), Error> {
        d.context.insert(key.into(), JsonValue::from(value.clone()));
        Ok(())
    }
    fn integer(
        &mut self,
        d: &mut Diagnostic,
        key: &'static str,
        text: &'a str,
    ) -> Result<(), Error> {
        d.context.insert(
            key.into(),
            JsonValue::Number((text).parse().expect("canonical integer")),
        );
        Ok(())
    }
}
fn lock(value: LockKind) -> &'static str {
    match value {
        LockKind::Uv => "uv",
        LockKind::Renv => "renv",
    }
}
fn function(draft: &Draft, index: usize) -> Result<&ast::Function, Error> {
    draft
        .functions
        .as_deref()
        .and_then(|f| f.get(index))
        .ok_or(Error::InvalidIndex)
}
/// Start a definition's diagnostic: at the environment file's
/// `functions.<name>` for a 0.5 definition, and at the root of a 1.0 function
/// file, whose paths start without a separator. The flag says whether the
/// path has begun.
fn definition_start<'a, B: Project<'a>>(
    definition: &'a ast::Function,
    project: &mut B,
) -> Result<(B::Output, bool), Error> {
    if definition.header.is_some() {
        return Ok((project.start(C::FunctionFileInvalid, "")?, false));
    }
    let mut d = project.start(C::ProjectEnvironmentInvalid, "functions.")?;
    project.append(&mut d, &definition.name.name)?;
    Ok((d, true))
}
/// Append a field to a definition's path; the first field of a 1.0 function
/// file's path drops its separator.
fn segment<'a, B: Project<'a>>(
    project: &mut B,
    d: &mut B::Output,
    begun: &mut bool,
    text: &'a str,
) -> Result<(), Error> {
    let text = if *begun {
        text
    } else {
        text.strip_prefix('.').unwrap_or(text)
    };
    if text.is_empty() {
        return Ok(());
    }
    *begun = true;
    project.append(d, text)
}
/// The field of a 1.0 function file that names a host language's callable.
fn implementation_field(language: Language) -> &'static str {
    match language {
        Language::Python => ".implementations.python",
        Language::R => ".implementations.r",
    }
}
/// The field of a 1.0 project file that holds a host language's lock.
pub fn lock_field(language: Language) -> &'static str {
    match language {
        Language::Python => "environments.python.lock",
        Language::R => "environments.r.lock",
    }
}
/// The lock a 1.0 project holds for `host`, written `written`, when its
/// captured bytes are no packaging lock of either host language: it fails as
/// the project's function files do, at the field that holds it (FUNC-3), with
/// the context of the 0.5 record it replaces and the project file, which holds
/// that field, as `source`.
pub fn invalid_packaging_lock(host: Language, project: &str, written: &str) -> Diagnostic {
    let field = lock_field(host);
    let text = |value: &str| JsonValue::String(value.into());
    Diagnostic {
        code: C::FunctionFileInvalid,
        spec_paths: vec![field.into()],
        context: [
            ("source".into(), text(project)),
            ("entry".into(), text(project)),
            ("path".into(), text(written)),
            ("environment_path".into(), text(field)),
            ("cause".into(), text("invalid_packaging_lock")),
        ]
        .into(),
        source_span: None,
        operand_route: None,
    }
}
fn function_diagnostic<'a, B: Project<'a>>(
    definition: &'a ast::Function,
    finding: &'a F,
    project: &mut B,
) -> Result<B::Output, Error> {
    let (mut d, mut begun) = definition_start(definition, project)?;
    project.context(&mut d, "function", &definition.name.name)?;
    let file = definition.header.is_some();
    let (reason, field) = match finding {
        F::InvalidName => ("invalid_function_name", if file { ".name" } else { "" }),
        F::InvalidCallable => ("invalid_callable", ".function"),
        F::MissingImplementation => ("missing_implementation", ".implementations"),
        F::InvalidImplementation(language) => {
            project.context(&mut d, "language", language.key())?;
            ("invalid_callable", implementation_field(*language))
        }
        F::EmptyDescription => ("empty_description", ".description"),
        F::NegativeComparisonDecimals => ("negative_comparison_decimals", ".comparison_decimals"),
        F::MissingObligation(tag) => {
            project.context(&mut d, "obligation", tag)?;
            ("missing_coverage_obligation", ".tests")
        }
        F::InvalidParameterName { parameter }
        | F::DuplicateParameter { parameter }
        | F::RequiredDefault { parameter }
        | F::MissingDefault { parameter }
        | F::InvalidDefault { parameter } => {
            let p = definition
                .params
                .get(*parameter)
                .ok_or(Error::InvalidIndex)?;
            segment(project, &mut d, &mut begun, ".params")?;
            project.index(&mut d, *parameter)?;
            project.context(&mut d, "parameter", &p.name.name)?;
            match finding {
                F::InvalidParameterName { .. } => ("invalid_parameter_name", ".name"),
                F::DuplicateParameter { .. } => ("duplicate_parameter", ".name"),
                F::RequiredDefault { .. } => ("required_parameter_default", ".default"),
                F::MissingDefault { .. } => ("missing_parameter_default", ".default"),
                F::InvalidDefault { .. } => ("invalid_parameter_default", ".default"),
                _ => unreachable!(),
            }
        }
        case_finding => {
            let case = match case_finding {
                F::InvalidCaseId { case }
                | F::DuplicateCaseId { case }
                | F::EmptyCoverage { case }
                | F::DuplicateCoverage { case, .. }
                | F::InvalidCoverage { case, .. }
                | F::UnsupportedEvidence { case, .. }
                | F::DuplicateArgument { case, .. }
                | F::UnknownArgument { case, .. }
                | F::MissingRequiredArgument { case, .. }
                | F::ArgumentType { case, .. }
                | F::InvalidResult { case } => *case,
                _ => unreachable!(),
            };
            let c = definition.tests.get(case).ok_or(Error::InvalidIndex)?;
            segment(project, &mut d, &mut begun, ".tests")?;
            project.index(&mut d, case)?;
            project.context(&mut d, "case", &c.id.text)?;
            match case_finding {
                F::InvalidCaseId { .. } => ("invalid_case_id", ".id"),
                F::DuplicateCaseId { .. } => ("duplicate_case_id", ".id"),
                F::EmptyCoverage { .. } => ("empty_coverage", ".covers"),
                F::DuplicateCoverage { tag, .. }
                | F::InvalidCoverage { tag, .. }
                | F::UnsupportedEvidence { tag, .. } => {
                    let text = &c.covers.get(*tag).ok_or(Error::InvalidIndex)?.text;
                    project.append(&mut d, ".covers")?;
                    project.index(&mut d, *tag)?;
                    project.context(&mut d, "tag", text)?;
                    let reason = match case_finding {
                        F::DuplicateCoverage { .. } => "duplicate_coverage",
                        F::InvalidCoverage { .. } => "invalid_coverage",
                        _ => "unsupported_coverage_evidence",
                    };
                    (reason, "")
                }
                F::DuplicateArgument { argument, .. }
                | F::UnknownArgument { argument, .. }
                | F::ArgumentType { argument, .. } => {
                    let (name, value) = c.args.get(*argument).ok_or(Error::InvalidIndex)?;
                    project.append(&mut d, ".args.")?;
                    project.append(&mut d, &name.name)?;
                    project.context(&mut d, "argument", &name.name)?;
                    project.scalar(&mut d, "value", value)?;
                    let reason = match case_finding {
                        F::DuplicateArgument { .. } => "duplicate_case_argument",
                        F::UnknownArgument { .. } => "unknown_case_argument",
                        _ => "invalid_case_argument_type",
                    };
                    (reason, "")
                }
                F::MissingRequiredArgument { parameter, .. } => {
                    let p = definition
                        .params
                        .get(*parameter)
                        .ok_or(Error::InvalidIndex)?;
                    project.append(&mut d, ".args.")?;
                    project.append(&mut d, &p.name.name)?;
                    project.context(&mut d, "argument", &p.name.name)?;
                    ("missing_required_case_argument", "")
                }
                F::InvalidResult { .. } => {
                    project.scalar(&mut d, "value", &c.result)?;
                    ("invalid_case_result", ".result")
                }
                _ => unreachable!(),
            }
        }
    };
    segment(project, &mut d, &mut begun, field)?;
    project.context(&mut d, "reason", reason)?;
    Ok(d)
}
fn terminology_diagnostic<'a, B: Project<'a>>(
    draft: &'a Draft,
    finding: &'a T,
    project: &mut B,
) -> Result<B::Output, Error> {
    let list = draft
        .codelists
        .get(finding.source)
        .and_then(|s| s.codelists.get(finding.codelist))
        .ok_or(Error::InvalidIndex)?;
    let code = match finding.fault {
        Fault::DuplicateId | Fault::DuplicateName => C::CodelistDuplicateIdentifier,
        Fault::FormatNamePrefix => C::CodelistFormatNamePrefix,
        Fault::InvalidShape => C::CodelistInvalidShape,
        Fault::InvalidValue { .. } => C::CodelistInvalidValue,
        Fault::DuplicateValue { .. } => C::CodelistDuplicateValue,
        Fault::PartialDecode | Fault::PartialRank => C::CodelistPartialItemField,
        Fault::ExtensionNotAdmitted { .. } => C::CodelistExtensionNotAdmitted,
        Fault::ExtendedAlias { .. } => C::CodelistExtendedAlias,
    };
    let mut d = project.start(code, "codelists")?;
    project.index(&mut d, finding.source)?;
    project.append(&mut d, ".codelists")?;
    project.index(&mut d, finding.codelist)?;
    project.context(&mut d, "codelist", &list.id.name)?;
    match finding.fault {
        Fault::DuplicateId => {
            project.append(&mut d, ".id")?;
            project.context(&mut d, "field", "id")?;
        }
        Fault::DuplicateName => {
            project.append(&mut d, ".name")?;
            project.context(&mut d, "field", "name")?;
            project.context(&mut d, "name", &list.name.text)?;
        }
        // A text codelist's format name without `$`, or a numeric one's with it.
        Fault::FormatNamePrefix => {
            project.append(&mut d, ".format_name")?;
            let format = &list.format_name.as_ref().ok_or(Error::InvalidIndex)?.text;
            project.context(&mut d, "value", format)?;
            project.context(&mut d, "data_type", list.data_type.name())?;
        }
        Fault::InvalidShape => {}
        Fault::PartialDecode => project.context(&mut d, "field", "decode")?,
        Fault::PartialRank => project.context(&mut d, "field", "rank")?,
        Fault::InvalidValue { item }
        | Fault::DuplicateValue { item }
        | Fault::ExtensionNotAdmitted { item }
        | Fault::ExtendedAlias { item } => {
            let entry = list
                .items
                .as_deref()
                .and_then(|i| i.get(item))
                .ok_or(Error::InvalidIndex)?;
            let value = &entry.value;
            project.append(&mut d, ".items")?;
            project.index(&mut d, item)?;
            match finding.fault {
                Fault::ExtensionNotAdmitted { .. } => project.append(&mut d, ".extended")?,
                Fault::ExtendedAlias { .. } => {
                    project.append(&mut d, ".alias")?;
                    project.scalar(&mut d, "value", value)?;
                    let alias = &entry.alias.as_ref().ok_or(Error::InvalidIndex)?.text;
                    project.context(&mut d, "alias", alias)?;
                }
                _ => {
                    project.append(&mut d, ".value")?;
                    project.scalar(&mut d, "value", value)?;
                }
            }
        }
    }
    Ok(d)
}
/// Project all independent findings in admission order.
/// Source indices refer to the retained typed draft; adapters restore source origins.
pub fn diagnostics(draft: &Draft, findings: &[Finding]) -> Result<Vec<Diagnostic>, Error> {
    let mut project = Owned;
    findings
        .iter()
        .map(|finding| finding_diagnostic(draft, finding, &mut project))
        .collect()
}
fn finding_diagnostic<'a, B: Project<'a>>(
    draft: &'a Draft,
    finding: &'a Finding,
    project: &mut B,
) -> Result<B::Output, Error> {
    match finding {
        Finding::LockKindMismatch {
            language: declared,
            actual,
        } => {
            let mut d = match draft.surface {
                Surface::V0_5 => project.start(C::ProjectEnvironmentInvalid, "lock")?,
                Surface::V1 { .. } => {
                    project.start(C::FunctionFileInvalid, lock_field(*declared))?
                }
            };
            project.context(&mut d, "reason", "lock_kind_mismatch")?;
            project.context(&mut d, "language", declared.key())?;
            project.context(&mut d, "actual_lock_kind", lock(*actual))?;
            Ok(d)
        }
        Finding::DuplicateFunction { function: index } => {
            let definition = function(draft, *index)?;
            // A 1.0 name is declared by the second file that writes it.
            let (mut d, mut begun) = definition_start(definition, project)?;
            if definition.header.is_some() {
                segment(project, &mut d, &mut begun, "name")?;
            }
            project.context(&mut d, "function", &definition.name.name)?;
            project.context(&mut d, "reason", "duplicate_function")?;
            Ok(d)
        }
        Finding::Function {
            function: index,
            finding,
        } => function_diagnostic(function(draft, *index)?, finding, project),
        Finding::Codelist(finding) => terminology_diagnostic(draft, finding, project),
    }
}
