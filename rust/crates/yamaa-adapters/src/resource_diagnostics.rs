//! Typed resource policies retain their applied bounds and authored owners.
use crate::{csv_source::TextTableError, specification_run::Error};
use serde_json::Value;
use yamaa_engine::{
    dataset::{ExecutionError, Resource},
    specification::SourceDeclaration,
};

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
    pub fn diagnostic(&self, stage: &str, code: &str) -> yamaa_core::diagnostic::Diagnostic {
        yamaa_core::application_issue::resource_rejected(
            stage,
            code,
            self.resource,
            self.limit,
            self.actual,
            self.spec_paths.clone(),
        )
    }
    pub fn issues(&self, stage: &str, code: &str) -> Vec<crate::issue_rows::Issue> {
        vec![
            crate::issue_rows::Issue::from_core(self.diagnostic(stage, code))
                .expect("resource finding"),
        ]
    }
    pub(crate) fn value(&self, stage: &str, code: &str) -> Option<Value> {
        crate::specification_diagnostics::portable_diagnostic(self.diagnostic(stage, code))
    }
}
pub(crate) fn resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::SourceRows => "rows",
        Resource::SourceCells => "cells",
        Resource::SourceColumns => "columns",
        Resource::SourceCount => "sources",
        Resource::OutputRows => "output_rows",
        Resource::OutputCells => "output_cells",
        Resource::OutputColumns => "output_columns",
        Resource::WorkCells => "work_cells",
        Resource::ScalarTextBytes => "scalar_text_bytes",
        Resource::OutputTextBytes => "output_text_bytes",
        Resource::IdentityCells => "identity_cells",
        Resource::IdentityTextBytes => "identity_text_bytes",
        Resource::PredicateWork => "predicate_work",
        Resource::PredicateResolutions => "predicate_resolutions",
        Resource::PredicateTextBytes => "predicate_text_bytes",
        Resource::PredicateLikeWork => "predicate_like_work",
        Resource::PredicateRegexSubjectBytes => "predicate_regex_subject_bytes",
        Resource::PredicateRegexWork => "predicate_regex_work",
        Resource::PredicateRegexStateCells => "predicate_regex_state_cells",
    }
}
fn input_paths(sources: &[SourceDeclaration]) -> Vec<String> {
    sources
        .iter()
        .map(|source| format!("input.{}", source.name))
        .collect()
}
pub(crate) fn execution<E>(
    error: &ExecutionError<E>,
    sources: &[SourceDeclaration],
) -> Option<Refusal> {
    let (resource, limit, actual, mut spec_paths) = match error {
        ExecutionError::Limit {
            resource,
            limit,
            required,
        } => (resource_name(*resource), *limit, *required, Vec::new()),
        ExecutionError::ScopedLimit {
            resource,
            limit,
            required,
            spec_paths,
        } => (
            resource_name(*resource),
            *limit,
            *required,
            spec_paths.clone(),
        ),
        ExecutionError::Grouping(yamaa_engine::table_grouping::GroupingError::RowLimit {
            limit,
            required,
        }) => ("rows", *limit, Some(*required), Vec::new()),
        ExecutionError::Grouping(yamaa_engine::table_grouping::GroupingError::KeyCellLimit {
            limit,
        })
        | ExecutionError::OutputGrouping(
            yamaa_engine::table_grouping::GroupingError::KeyCellLimit { limit },
        ) => ("key_cells", *limit, None, Vec::new()),
        _ => return None,
    };
    if spec_paths.is_empty() && matches!(resource, "rows" | "cells" | "columns" | "sources") {
        spec_paths = input_paths(sources);
    }
    Some(Refusal::new(resource, limit, actual, spec_paths))
}
pub(crate) fn source(error: &Error, source: &SourceDeclaration) -> Option<Refusal> {
    let (resource, limit, actual) = match error {
        Error::ResourceLimit(refusal) => return Some(refusal.clone()),
        Error::Execution(crate::dataset_result::DatasetError::Table(
            crate::ipc_table::TableError::Limit {
                resource,
                limit,
                required,
            },
        )) => return Some(Refusal::new(resource, *limit, *required, Vec::new())),
        Error::ParquetSource(crate::parquet_source::Error::ResourceLimit {
            resource,
            limit,
            actual,
        }) => (*resource, *limit, *actual),
        Error::Source(TextTableError::Csv(crate::csv_source::Error::Limit { resource, limit })) => {
            (*resource, *limit, None)
        }
        Error::Source(TextTableError::Table(crate::arrow_table::TableError::Limit {
            resource,
            limit,
            required,
        }))
        | Error::TypedSource(crate::typed_csv::Error::Table(
            crate::arrow_table::TableError::Limit {
                resource,
                limit,
                required,
            },
        ))
        | Error::ParquetSource(crate::parquet_source::Error::Table(
            crate::arrow_table::TableError::Limit {
                resource,
                limit,
                required,
            },
        )) => (*resource, *limit, *required),
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
pub(crate) fn source_findings(error: &Error, source: &SourceDeclaration) -> Option<Vec<Value>> {
    if let Error::Sources(errors) = error {
        let mut findings = Vec::new();
        for (source, error) in errors {
            findings.extend(source_findings(error, source)?);
        }
        return Some(findings);
    }
    crate::specification_diagnostics::findings(error, Some(source)).or_else(|| {
        Some(vec![
            self::source(error, source)?.value("ingest", "source_limit")?
        ])
    })
}
pub(crate) fn run_limit<E>(
    error: &yamaa_engine::specification_run::RunError<E>,
    sources: &[SourceDeclaration],
) -> Option<Refusal> {
    use yamaa_engine::specification_run::RunError;
    let (resource, limit, actual, source) = match error {
        RunError::SourceBytes {
            limit,
            actual,
            source,
        } => ("bytes", *limit, *actual, Some(*source)),
        RunError::SourceCells {
            limit,
            actual,
            source,
        } => ("cells", *limit, *actual, *source),
        _ => return None,
    };
    let spec_paths = match source {
        Some(index) => vec![format!("input.{}", sources.get(index)?.name)],
        None => input_paths(sources),
    };
    Some(Refusal::new(resource, limit, actual, spec_paths))
}

pub(crate) fn outcome(value: &Value, sources: &[SourceDeclaration]) -> Option<Refusal> {
    let resource = match value["resource"].as_str()? {
        "rows" => "rows",
        "cells" => "cells",
        "columns" => "columns",
        "sources" => "sources",
        "output_rows" => "output_rows",
        "output_cells" => "output_cells",
        "output_columns" => "output_columns",
        "source_rows" => "rows",
        "key_cells" => "key_cells",
        "work_cells" => "work_cells",
        "scalar_text_bytes" => "scalar_text_bytes",
        "output_text_bytes" => "output_text_bytes",
        "identity_cells" => "identity_cells",
        "identity_text_bytes" => "identity_text_bytes",
        "predicate_work" => "predicate_work",
        "predicate_resolutions" => "predicate_resolutions",
        "predicate_text_bytes" => "predicate_text_bytes",
        "predicate_like_work" => "predicate_like_work",
        "predicate_regex_subject_bytes" => "predicate_regex_subject_bytes",
        "predicate_regex_work" => "predicate_regex_work",
        "predicate_regex_state_cells" => "predicate_regex_state_cells",
        _ => return None,
    };
    let limit = value["limit"].as_str()?.parse().ok()?;
    let actual = value["required"].as_str().and_then(|v| v.parse().ok());
    let mut paths = match value["spec_paths"].as_array() {
        Some(v) => v
            .iter()
            .map(|path| path.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    if paths.is_empty() && matches!(resource, "rows" | "cells" | "columns" | "sources") {
        paths = input_paths(sources);
    }
    Some(Refusal::new(resource, limit, actual, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> SourceDeclaration {
        SourceDeclaration {
            name: "XW".into(),
            path: "xw.csv".into(),
            profile: yamaa_core::specification::SourceProfile::Csv,
            types: Vec::new(),
            ordinal: None,
            empty_string_present: false,
        }
    }
    #[test]
    fn source_shape_refusal_keeps_the_supplied_quota_count_and_input() {
        for (rows, cells, resource) in [(1, 64, "rows"), (64, 1, "cells")] {
            let failure = crate::csv_source::parse_text_table(
                b"ID\n1\n2\n",
                Default::default(),
                crate::arrow_table::TableLimits {
                    max_rows: rows,
                    max_cells: cells,
                    max_columns: 1,
                    max_batches: 1,
                },
            )
            .unwrap_err();
            let error = Error::Source(failure);
            let limit = 1;
            let actual = Some(2);
            let refusal = source(&error, &input()).unwrap();
            assert_eq!(
                refusal,
                Refusal::new(resource, limit, actual, vec!["input.XW".into()])
            );
            let value = refusal.value("ingest", "source_limit").unwrap();
            assert_eq!(value["context"]["resource"], resource);
            assert_eq!(value["context"]["limit"], serde_json::json!(limit));
            assert_eq!(value["spec_paths"], serde_json::json!(["input.XW"]));
            assert_eq!(value["context"].get("actual").is_some(), actual.is_some());
        }
    }
}
