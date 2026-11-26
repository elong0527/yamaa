//! Typed resource policies retain their applied bounds and authored owners.
use crate::{csv_source::TextTableError, source_decoding::Error};
use yamaa_core::application_issue::{Fault, Step};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{dataset::Resource, specification::SourceDeclaration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub resource: &'static str,
    pub limit: usize,
    pub actual: Option<usize>,
    pub spec_paths: Vec<String>,
}
impl Refusal {
    pub fn new(
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
        spec_paths: Vec<String>,
    ) -> Self {
        Self {
            resource,
            limit,
            actual,
            spec_paths,
        }
    }
    pub fn diagnostic(&self, step: Step, fault: Fault) -> Diagnostic {
        yamaa_core::application_issue::resource_rejected(
            step,
            fault,
            self.resource,
            self.limit,
            self.actual,
            self.spec_paths.clone(),
        )
    }
    pub fn issues(&self, step: Step, fault: Fault) -> Vec<Diagnostic> {
        vec![self.diagnostic(step, fault)]
    }
}
pub fn resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::TextBytes => "text_bytes",
    }
}
pub(crate) fn source(error: &Error, source: &SourceDeclaration) -> Option<Refusal> {
    let (resource, limit, actual) = match error {
        Error::ResourceLimit(refusal) => return Some(refusal.clone()),
        Error::ParquetSource(crate::parquet_source::Error::ResourceLimit {
            resource,
            limit,
            actual,
        })
        | Error::SasSource(crate::sas_source::Error::Limit {
            resource,
            limit,
            actual,
        })
        | Error::DatasetJsonSource(crate::dataset_json_source::Error::Limit {
            resource,
            limit,
            actual,
        }) => (*resource, *limit, *actual),
        Error::Source(TextTableError::Csv(crate::csv_source::Error::Limit {
            resource,
            limit,
            actual,
        })) => (*resource, *limit, *actual),
        _ => return None,
    };
    Some(Refusal::new(
        resource,
        limit,
        actual,
        vec![format!("input.{}", source.name)],
    ))
}
/// A mixed ingestion collection retains authored order and the original barrier.
pub fn source_findings(
    error: &Error,
    source: &SourceDeclaration,
) -> Result<Option<Vec<Diagnostic>>, crate::source_diagnostics::SourceProjectionError> {
    let mut pending = vec![(source, error)];
    let mut findings = Vec::new();
    while let Some((source, error)) = pending.pop() {
        if let Some(projected) = crate::specification_diagnostics::findings(error, Some(source))? {
            findings.extend(projected);
        } else if let Error::Sources(errors) = error {
            pending.extend(errors.iter().rev().map(|(source, error)| (source, error)));
        } else if let Some(refusal) = self::source(error, source) {
            findings.push(refusal.diagnostic(Step::Ingest, Fault::SourceLimit));
        } else if let Error::Allocation { spec_paths } = error {
            findings.push(yamaa_core::application_issue::allocation(
                spec_paths.clone(),
            ));
        } else {
            return Ok(None);
        }
    }
    Ok(Some(findings))
}
/// The findings of several sources' failures, as [`source_findings`] reports
/// one failure that lists them: every finding when all are language findings,
/// otherwise each source's own findings or bound, in order.
pub fn sources_findings(
    failures: &[(&SourceDeclaration, &Error)],
) -> Result<Option<Vec<Diagnostic>>, crate::source_diagnostics::SourceProjectionError> {
    if let Some(findings) = crate::source_diagnostics::findings_of(failures)? {
        return Ok(Some(findings));
    }
    let mut findings = Vec::new();
    for &(source, error) in failures {
        match source_findings(error, source)? {
            Some(projected) => findings.extend(projected),
            None => return Ok(None),
        }
    }
    Ok(Some(findings))
}
pub fn run_limit<E>(
    error: &yamaa_core::specification_run::RunError<E>,
    sources: &[SourceDeclaration],
) -> Option<Refusal> {
    let yamaa_core::specification_run::RunError::Boundary { source, error } = error else {
        return None;
    };
    Some(Refusal::new(
        error.resource,
        error.limit,
        error.actual,
        vec![format!("input.{}", sources.get(*source)?.name)],
    ))
}

/// The refusal of one execution bound, at the paths that own it.
pub fn outcome(bound: &crate::dataset_result::Limit) -> Option<Refusal> {
    if bound.resource != "text_bytes" {
        return None;
    }
    Some(Refusal::new(
        bound.resource,
        bound.limit?,
        bound.required,
        bound.spec_paths.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_resource_fallback_preserves_complete_context_and_nested_findings() {
        let value = "x".repeat(2_000_000);
        let error = Error::TypedSource(crate::typed_csv::Error::Typing(
            yamaa_core::typed_csv::Error::FieldParse {
                field: "N".into(),
                target: yamaa_core::value::ColumnType::Int,
                value: value.clone(),
                record: 1,
            },
        ));
        let findings = source_findings(&error, &input()).unwrap().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].context["value"], value);
        let mut error = Error::Source(TextTableError::Csv(
            crate::csv_source::parse(b"").unwrap_err(),
        ));
        for _ in 0..150 {
            error = Error::Sources(vec![(input(), error)]);
        }
        assert_eq!(source_findings(&error, &input()).unwrap().unwrap().len(), 1);
    }
    fn input() -> SourceDeclaration {
        SourceDeclaration {
            name: "XW".into(),
            path: "xw.csv".into(),
            profile: yamaa_core::specification::SourceProfile::Csv,
            encoding: None,
            byte_order_mark: false,
            fixed_width: None,
            types: Vec::new(),
            ordinal: None,
            empty_string_present: false,
        }
    }
    #[test]
    fn fixed_sas_boundary_keeps_its_bound_and_input_owner() {
        let mut declaration = input();
        declaration.path = "v5.xpt".into();
        declaration.profile = yamaa_core::specification::SourceProfile::Xport;
        declaration.encoding = Some("windows-1252".into());
        let failure = crate::sas_source::Error::Limit {
            resource: "source_bytes",
            limit: 10_737_418_240,
            actual: Some(10_737_418_241),
        };
        let failure = Error::SasSource(failure);
        let refusal = source(&failure, &declaration).unwrap();
        assert_eq!(refusal.resource, "source_bytes");
        assert_eq!(refusal.limit, 10_737_418_240);
        assert_eq!(refusal.actual, Some(10_737_418_241));
        assert_eq!(refusal.spec_paths, ["input.XW"]);
        let crate::specification_diagnostics::Rejection::Fault(projected) =
            crate::specification_diagnostics::failure(&failure, Some(&declaration))
        else {
            panic!("a fixed source bound is an engine fault");
        };
        assert_eq!(projected.context["step"], "ingest");
        assert_eq!(projected.context["code"], "sas_limit");
        assert_eq!(projected.context["resource"], "source_bytes");
        assert_eq!(projected.context["limit"], 10_737_418_240usize);
        assert_eq!(projected.context["actual"], 10_737_418_241usize);
        assert_eq!(projected.spec_paths, ["input.XW"]);
    }
}
