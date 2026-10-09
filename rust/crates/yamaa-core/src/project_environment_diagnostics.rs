//! Project admission findings retain their typed input and authored field paths.
//! Resource exhaustion and host failures remain outside semantic vocabulary.
use crate::{
    diagnostic::{ConditionCode as C, ContextValue, Diagnostic},
    project_environment::{Draft, Finding, LockKind},
    project_function::{Definition, Finding as F, Language},
    project_terminology::{Fault, Finding as T},
    value::Value,
};
use alloc::{format, string::String, vec, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Limit,
    InvalidIndex,
    UnavailableShape,
}

/// Independent root obligations remain observable when an inline scalar cannot
/// decode. The normalized root is retained, and lock format is never inferred
/// from an uncaptured path. No partial function or terminology model is created.
pub fn root_diagnostics(
    document: &crate::schema::Document,
    host: Language,
) -> Result<Vec<Diagnostic>, Error> {
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
    let lock = match document.field(root, "lock").map(|n| &document.nodes()[n]) {
        None => false,
        Some(N::Text(_)) => true,
        _ => return Err(Error::UnavailableShape),
    };
    let mut findings = Vec::new();
    if present {
        if declared.is_none() {
            findings.push(Finding::MissingLanguage);
        }
        if !lock {
            findings.push(Finding::MissingLock);
        }
    }
    if let Some(declared) = declared {
        if declared != host {
            findings.push(Finding::LanguageMismatch { declared, host });
        }
    }
    let has_study = document.field(root, "study").is_some();
    if !has_study
        && ["sdtm", "adam", "send"]
            .iter()
            .any(|name| document.field(root, name).is_some())
    {
        findings.push(Finding::MissingStudy);
    }
    diagnostics(
        &Draft {
            language: declared,
            lock: None,
            functions: None,
            codelists: Vec::new(),
            has_study,
            submissions: Vec::new(),
        },
        &findings,
    )
}

/// Project scalar failures against their retained normalized arena. Integer
/// values outside runtime i64 keep their exact mathematical decimal text.
/// A normalized-shape contradiction has no fabricated language condition.
pub fn scalar_diagnostics(
    document: &crate::schema::Document,
    findings: &[crate::project_function_document::Finding],
) -> Result<Vec<Diagnostic>, Error> {
    use crate::{project_function_document::Kind, schema::DocumentNode as N};
    if findings.len() > 65_536 {
        return Err(Error::Limit);
    }
    let mut budget = Budget(16_777_216);
    findings
        .iter()
        .map(|finding| {
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
            let mut d = budget.start(C::ProjectEnvironmentInvalid, &finding.path)?;
            budget.context(&mut d, "reason", reason)?;
            match (finding.kind, node) {
                (Kind::IntegerRange, N::Integer(text)) => {
                    let key = budget.text("value")?;
                    let text = budget.text(text)?;
                    d.context.insert(key, ContextValue::Integer(text));
                }
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
                    budget.context(&mut d, "value", text)?;
                }
                _ => return Err(Error::UnavailableShape),
            }
            Ok(d)
        })
        .collect()
}
struct Budget(usize);
impl Budget {
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(bytes).ok_or(Error::Limit)?;
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<String, Error> {
        self.charge(text.len())?;
        Ok(text.into())
    }
    fn append(&mut self, path: &mut String, text: &str) -> Result<(), Error> {
        self.charge(text.len())?;
        path.push_str(text);
        Ok(())
    }
    fn index(&mut self, path: &mut String, index: usize) -> Result<(), Error> {
        // Decimal usize text is bounded independently of authored metadata.
        self.append(path, &format!("[{index}]"))
    }
    fn context(&mut self, d: &mut Diagnostic, key: &str, text: &str) -> Result<(), Error> {
        let key = self.text(key)?;
        let text = self.text(text)?;
        d.context
            .insert(key, ContextValue::Scalar(Value::Str(text)));
        Ok(())
    }
    fn scalar(&mut self, d: &mut Diagnostic, key: &str, value: &Value) -> Result<(), Error> {
        let key = self.text(key)?;
        self.charge(match value {
            Value::Str(s) => s.len(),
            _ => 32,
        })?;
        d.context.insert(key, ContextValue::Scalar(value.clone()));
        Ok(())
    }
    fn start(&mut self, code: C, path: &str) -> Result<Diagnostic, Error> {
        self.charge(128)?;
        Ok(Diagnostic {
            code,
            spec_paths: vec![self.text(path)?],
            context: Default::default(),
            source_span: None,
            operand_route: None,
        })
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
fn function_diagnostic(
    definition: &Definition,
    finding: &F,
    budget: &mut Budget,
) -> Result<Diagnostic, Error> {
    let mut d = budget.start(C::ProjectEnvironmentInvalid, "functions.")?;
    budget.append(&mut d.spec_paths[0], &definition.name)?;
    budget.context(&mut d, "function", &definition.name)?;
    let (reason, field) = match finding {
        F::InvalidName => ("invalid_function_name", ""),
        F::InvalidCallable => ("invalid_callable", ".function"),
        F::EmptyDescription => ("empty_description", ".description"),
        F::NegativeComparisonDecimals => ("negative_comparison_decimals", ".comparison_decimals"),
        F::MissingObligation(tag) => {
            budget.context(&mut d, "obligation", tag)?;
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
            budget.append(&mut d.spec_paths[0], ".params")?;
            budget.index(&mut d.spec_paths[0], *parameter)?;
            budget.context(&mut d, "parameter", &p.name)?;
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
            budget.append(&mut d.spec_paths[0], ".tests")?;
            budget.index(&mut d.spec_paths[0], case)?;
            budget.context(&mut d, "case", &c.id)?;
            match case_finding {
                F::InvalidCaseId { .. } => ("invalid_case_id", ".id"),
                F::DuplicateCaseId { .. } => ("duplicate_case_id", ".id"),
                F::EmptyCoverage { .. } => ("empty_coverage", ".covers"),
                F::DuplicateCoverage { tag, .. }
                | F::InvalidCoverage { tag, .. }
                | F::UnsupportedEvidence { tag, .. } => {
                    let text = c.covers.get(*tag).ok_or(Error::InvalidIndex)?;
                    budget.append(&mut d.spec_paths[0], ".covers")?;
                    budget.index(&mut d.spec_paths[0], *tag)?;
                    budget.context(&mut d, "tag", text)?;
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
                    budget.append(&mut d.spec_paths[0], ".args.")?;
                    budget.append(&mut d.spec_paths[0], name)?;
                    budget.context(&mut d, "argument", name)?;
                    budget.scalar(&mut d, "value", value)?;
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
                    budget.append(&mut d.spec_paths[0], ".args.")?;
                    budget.append(&mut d.spec_paths[0], &p.name)?;
                    budget.context(&mut d, "argument", &p.name)?;
                    ("missing_required_case_argument", "")
                }
                F::InvalidResult { .. } => {
                    budget.scalar(&mut d, "value", &c.result)?;
                    ("invalid_case_result", ".result")
                }
                _ => unreachable!(),
            }
        }
    };
    budget.append(&mut d.spec_paths[0], field)?;
    budget.context(&mut d, "reason", reason)?;
    Ok(d)
}
fn terminology_diagnostic(
    draft: &Draft,
    finding: &T,
    budget: &mut Budget,
) -> Result<Diagnostic, Error> {
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
    let mut d = budget.start(code, "codelists")?;
    budget.index(&mut d.spec_paths[0], finding.source)?;
    budget.append(&mut d.spec_paths[0], ".codelists")?;
    budget.index(&mut d.spec_paths[0], finding.codelist)?;
    budget.context(&mut d, "codelist", &list.id)?;
    match finding.fault {
        Fault::DuplicateId => {
            budget.append(&mut d.spec_paths[0], ".id")?;
            budget.context(&mut d, "field", "id")?;
        }
        Fault::DuplicateName => {
            budget.append(&mut d.spec_paths[0], ".name")?;
            budget.context(&mut d, "field", "name")?;
            budget.context(&mut d, "name", &list.name)?;
        }
        Fault::InvalidShape => {}
        Fault::PartialDecode => budget.context(&mut d, "field", "decode")?,
        Fault::PartialRank => budget.context(&mut d, "field", "rank")?,
        Fault::InvalidValue { item }
        | Fault::DuplicateValue { item }
        | Fault::ExtensionNotAdmitted { item } => {
            let value = &list
                .items
                .as_deref()
                .and_then(|i| i.get(item))
                .ok_or(Error::InvalidIndex)?
                .value;
            budget.append(&mut d.spec_paths[0], ".items")?;
            budget.index(&mut d.spec_paths[0], item)?;
            if matches!(finding.fault, Fault::ExtensionNotAdmitted { .. }) {
                budget.append(&mut d.spec_paths[0], ".extended")?;
            } else {
                budget.append(&mut d.spec_paths[0], ".value")?;
                budget.scalar(&mut d, "value", value)?;
            }
        }
    }
    Ok(d)
}
/// Project all independent findings in admission order, with one cumulative
/// bound checked before each owned text/value copy. No partial vector is returned.
/// Source indices refer to the retained typed draft; adapters restore source origins.
pub fn diagnostics(draft: &Draft, findings: &[Finding]) -> Result<Vec<Diagnostic>, Error> {
    diagnostics_with_text_limit(draft, findings, 16_777_216)
}
pub fn diagnostics_with_text_limit(
    draft: &Draft,
    findings: &[Finding],
    bytes: usize,
) -> Result<Vec<Diagnostic>, Error> {
    if findings.len() > 65_536 {
        return Err(Error::Limit);
    }
    let mut budget = Budget(bytes);
    findings
        .iter()
        .map(|finding| {
            let (reason, path) = match finding {
                Finding::MissingLanguage => ("missing_language", "language"),
                Finding::MissingLock => ("missing_lock", "lock"),
                Finding::MissingStudy => ("missing_study", "study"),
                Finding::LanguageMismatch { declared, host } => {
                    let mut d = budget.start(C::RunnerLanguageMismatch, "language")?;
                    budget.context(&mut d, "declared", language(*declared))?;
                    budget.context(&mut d, "runner", language(*host))?;
                    return Ok(d);
                }
                Finding::LockKindMismatch {
                    language: declared,
                    actual,
                } => {
                    let mut d = budget.start(C::ProjectEnvironmentInvalid, "lock")?;
                    budget.context(&mut d, "reason", "lock_kind_mismatch")?;
                    budget.context(&mut d, "language", language(*declared))?;
                    budget.context(&mut d, "actual_lock_kind", lock(*actual))?;
                    return Ok(d);
                }
                Finding::DuplicateFunction { function: index } => {
                    let definition = function(draft, *index)?;
                    let mut d = budget.start(C::ProjectEnvironmentInvalid, "functions.")?;
                    budget.append(&mut d.spec_paths[0], &definition.name)?;
                    budget.context(&mut d, "function", &definition.name)?;
                    budget.context(&mut d, "reason", "duplicate_function")?;
                    return Ok(d);
                }
                Finding::Function {
                    function: index,
                    finding,
                } => return function_diagnostic(function(draft, *index)?, finding, &mut budget),
                Finding::Codelist(finding) => {
                    return terminology_diagnostic(draft, finding, &mut budget)
                }
            };
            let mut d = budget.start(C::ProjectEnvironmentInvalid, path)?;
            budget.context(&mut d, "reason", reason)?;
            Ok(d)
        })
        .collect()
}
