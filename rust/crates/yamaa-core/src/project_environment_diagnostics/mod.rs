//! Project admission findings retain their typed input and authored field paths.
//! Resource exhaustion and host failures remain outside semantic vocabulary.
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    project_environment::{Draft, Finding, LockKind},
    project_function::{Definition, Finding as F, Language},
    project_terminology::{Fault, Finding as T},
    value::Value,
};
use serde_json::Value as JsonValue;
use std::vec;

pub mod projection;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("project environment diagnostics invalid index")]
    InvalidIndex,
    #[error("project environment diagnostics unavailable shape")]
    UnavailableShape,
}

/// Closed native root-read facts; arbitrary error messages cannot establish them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadFailure {
    #[error("project environment diagnostics missing")]
    Missing,
    #[error("project environment diagnostics unreadable")]
    Unreadable,
}
impl ReadFailure {
    pub fn diagnostic(self, path: &str) -> Diagnostic {
        Diagnostic {
            code: C::ProjectEnvironmentMissing,
            spec_paths: vec!["environment".into()],
            context: [
                ("path".into(), JsonValue::String(path.into())),
                (
                    "cause".into(),
                    JsonValue::String(
                        match self {
                            Self::Missing => "missing",
                            Self::Unreadable => "unreadable",
                        }
                        .into(),
                    ),
                ),
            ]
            .into(),
            source_span: None,
            operand_route: None,
        }
    }
}

fn root_findings(
    document: &crate::schema::Document,
    host: Language,
) -> Result<[Option<Finding>; 3], Error> {
    use crate::schema::DocumentNode as N;
    let root = document.root();
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        return Err(Error::UnavailableShape);
    }
    let present = match document
        .field(root, "functions")
        .map(|n| &document.nodes()[n])
    {
        None => false,
        Some(N::Mapping(_)) => true,
        _ => return Err(Error::UnavailableShape),
    };
    let declared = match document
        .field(root, "language")
        .map(|n| &document.nodes()[n])
    {
        None => None,
        Some(N::Text(text)) if text == "python" => Some(Language::Python),
        Some(N::Text(text)) if text == "r" => Some(Language::R),
        _ => return Err(Error::UnavailableShape),
    };
    Ok([
        (present && declared.is_none()).then_some(Finding::MissingLanguage),
        declared
            .filter(|declared| *declared != host)
            .map(|declared| Finding::LanguageMismatch { declared, host }),
        (document.field(root, "study").is_none()
            && ["sdtm", "adam", "send"]
                .iter()
                .any(|name| document.field(root, name).is_some()))
        .then_some(Finding::MissingStudy),
    ])
}

/// Project scalar failures against their retained normalized arena. Integer
/// values outside runtime i64 keep their exact mathematical decimal text.
/// A normalized-shape contradiction has no fabricated language condition.
pub fn scalar_diagnostics(
    document: &crate::schema::Document,
    findings: &[crate::project_function_document::Finding],
) -> Result<Vec<Diagnostic>, Error> {
    let mut project = Owned;
    findings
        .iter()
        .map(|finding| scalar_diagnostic(document, finding, &mut project))
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
    document: &'a crate::schema::Document,
    finding: &'a crate::project_function_document::Finding,
    project: &mut B,
) -> Result<B::Output, Error> {
    use crate::{project_function_document::Kind, schema::DocumentNode as N};
    let node = document
        .nodes()
        .get(finding.node)
        .ok_or(Error::InvalidIndex)?;
    let reason = match finding.kind {
        Kind::IntegerRange => "integer_out_of_range",
        Kind::InvalidDate => "invalid_date",
        Kind::InvalidDateTime => "invalid_datetime",
        Kind::NormalizedShape => return Err(Error::UnavailableShape),
    };
    let mut d = project.start(C::ProjectEnvironmentInvalid, &finding.path)?;
    project.context(&mut d, "reason", reason)?;
    match (finding.kind, node) {
        (Kind::IntegerRange, N::Integer(text)) => project.integer(&mut d, "value", text)?,
        (Kind::InvalidDate | Kind::InvalidDateTime, N::Mapping(_)) => {
            let field = if finding.kind == Kind::InvalidDate {
                "date"
            } else {
                "datetime"
            };
            let text = document
                .field(finding.node, field)
                .and_then(|node| document.nodes().get(node));
            let Some(N::Text(text)) = text else {
                return Err(Error::UnavailableShape);
            };
            project.context(&mut d, "value", text)?;
        }
        _ => return Err(Error::UnavailableShape),
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
fn language(value: Language) -> &'static str {
    match value {
        Language::Python => "python",
        Language::R => "r",
    }
}
fn lock(value: LockKind) -> &'static str {
    match value {
        LockKind::Uv => "uv",
        LockKind::Renv => "renv",
    }
}
fn function(draft: &Draft, index: usize) -> Result<&Definition, Error> {
    draft
        .functions
        .as_deref()
        .and_then(|f| f.get(index))
        .ok_or(Error::InvalidIndex)
}
fn function_diagnostic<'a, B: Project<'a>>(
    definition: &'a Definition,
    finding: &'a F,
    project: &mut B,
) -> Result<B::Output, Error> {
    let mut d = project.start(C::ProjectEnvironmentInvalid, "functions.")?;
    project.append(&mut d, &definition.name)?;
    project.context(&mut d, "function", &definition.name)?;
    let (reason, field) = match finding {
        F::InvalidName => ("invalid_function_name", ""),
        F::InvalidCallable => ("invalid_callable", ".function"),
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
            project.append(&mut d, ".params")?;
            project.index(&mut d, *parameter)?;
            project.context(&mut d, "parameter", &p.name)?;
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
            project.append(&mut d, ".tests")?;
            project.index(&mut d, case)?;
            project.context(&mut d, "case", &c.id)?;
            match case_finding {
                F::InvalidCaseId { .. } => ("invalid_case_id", ".id"),
                F::DuplicateCaseId { .. } => ("duplicate_case_id", ".id"),
                F::EmptyCoverage { .. } => ("empty_coverage", ".covers"),
                F::DuplicateCoverage { tag, .. }
                | F::InvalidCoverage { tag, .. }
                | F::UnsupportedEvidence { tag, .. } => {
                    let text = c.covers.get(*tag).ok_or(Error::InvalidIndex)?;
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
                    project.append(&mut d, name)?;
                    project.context(&mut d, "argument", name)?;
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
                    project.append(&mut d, &p.name)?;
                    project.context(&mut d, "argument", &p.name)?;
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
    project.append(&mut d, field)?;
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
        Fault::InvalidShape => C::CodelistInvalidShape,
        Fault::InvalidValue { .. } => C::CodelistInvalidValue,
        Fault::DuplicateValue { .. } => C::CodelistDuplicateValue,
        Fault::PartialDecode | Fault::PartialRank => C::CodelistPartialItemField,
        Fault::ExtensionNotAdmitted { .. } => C::CodelistExtensionNotAdmitted,
    };
    let mut d = project.start(code, "codelists")?;
    project.index(&mut d, finding.source)?;
    project.append(&mut d, ".codelists")?;
    project.index(&mut d, finding.codelist)?;
    project.context(&mut d, "codelist", &list.id)?;
    match finding.fault {
        Fault::DuplicateId => {
            project.append(&mut d, ".id")?;
            project.context(&mut d, "field", "id")?;
        }
        Fault::DuplicateName => {
            project.append(&mut d, ".name")?;
            project.context(&mut d, "field", "name")?;
            project.context(&mut d, "name", &list.name)?;
        }
        Fault::InvalidShape => {}
        Fault::PartialDecode => project.context(&mut d, "field", "decode")?,
        Fault::PartialRank => project.context(&mut d, "field", "rank")?,
        Fault::InvalidValue { item }
        | Fault::DuplicateValue { item }
        | Fault::ExtensionNotAdmitted { item } => {
            let value = &list
                .items
                .as_deref()
                .and_then(|i| i.get(item))
                .ok_or(Error::InvalidIndex)?
                .value;
            project.append(&mut d, ".items")?;
            project.index(&mut d, item)?;
            if matches!(finding.fault, Fault::ExtensionNotAdmitted { .. }) {
                project.append(&mut d, ".extended")?;
            } else {
                project.append(&mut d, ".value")?;
                project.scalar(&mut d, "value", value)?;
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
    let (reason, path) = match finding {
        Finding::MissingLanguage => ("missing_language", "language"),
        Finding::MissingStudy => ("missing_study", "study"),
        Finding::LanguageMismatch { declared, host } => {
            let mut d = project.start(C::RunnerLanguageMismatch, "language")?;
            project.context(&mut d, "declared", language(*declared))?;
            project.context(&mut d, "runner", language(*host))?;
            return Ok(d);
        }
        Finding::LockKindMismatch {
            language: declared,
            actual,
        } => {
            let mut d = project.start(C::ProjectEnvironmentInvalid, "lock")?;
            project.context(&mut d, "reason", "lock_kind_mismatch")?;
            project.context(&mut d, "language", language(*declared))?;
            project.context(&mut d, "actual_lock_kind", lock(*actual))?;
            return Ok(d);
        }
        Finding::DuplicateFunction { function: index } => {
            let definition = function(draft, *index)?;
            let mut d = project.start(C::ProjectEnvironmentInvalid, "functions.")?;
            project.append(&mut d, &definition.name)?;
            project.context(&mut d, "function", &definition.name)?;
            project.context(&mut d, "reason", "duplicate_function")?;
            return Ok(d);
        }
        Finding::Function {
            function: index,
            finding,
        } => return function_diagnostic(function(draft, *index)?, finding, project),
        Finding::Codelist(finding) => return terminology_diagnostic(draft, finding, project),
    };
    let mut d = project.start(C::ProjectEnvironmentInvalid, path)?;
    project.context(&mut d, "reason", reason)?;
    Ok(d)
}
