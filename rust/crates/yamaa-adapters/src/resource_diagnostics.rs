//! Typed resource policies retain their applied bounds and authored owners.
use crate::{csv_source::TextTableError, specification_run::Error};
use serde_json::Value;
use yamaa_core::{
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
        Some(crate::specification_diagnostics::portable_diagnostic(
            self.diagnostic(stage, code),
        ))
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
        Resource::RegexWork => "regex_steps",
    }
}
fn input_paths(sources: &[SourceDeclaration]) -> Vec<String> {
    sources
        .iter()
        .map(|source| format!("input.{}", source.name))
        .collect()
}
fn source_path(
    sources: &[SourceDeclaration],
    driver_index: usize,
    source_index: usize,
) -> Option<String> {
    let source = if source_index == 0 {
        sources.get(driver_index)?
    } else {
        sources
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != driver_index)
            .nth(source_index - 1)?
            .1
    };
    Some(format!("input.{}", source.name))
}
pub(crate) fn execution<E>(
    error: &ExecutionError<E>,
    sources: &[SourceDeclaration],
    driver_index: usize,
) -> Option<Refusal> {
    let (resource, limit, actual, mut spec_paths) = match error {
        ExecutionError::Limit {
            resource,
            limit,
            required,
        } => (resource_name(*resource), *limit, *required, Vec::new()),
        ExecutionError::SourceLimit {
            resource,
            limit,
            required,
            source_index,
        } => (
            resource_name(*resource),
            *limit,
            *required,
            vec![source_path(sources, driver_index, *source_index)?],
        ),
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
        ExecutionError::Grouping(yamaa_core::table_grouping::GroupingError::Limit(error))
        | ExecutionError::OutputGrouping(yamaa_core::table_grouping::GroupingError::Limit(error))
        | ExecutionError::Reduction {
            error: yamaa_core::table_reduction::TableReductionError::Limit(error),
            ..
        } => (error.resource, error.limit, error.actual, Vec::new()),
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
        })
        | Error::SasSource(crate::sas_source::Error::Limit {
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
pub(crate) fn source_findings(
    error: &Error,
    source: &SourceDeclaration,
) -> Result<Option<Vec<Value>>, crate::source_diagnostics::SourceProjectionError> {
    source_findings_at(error, source, 0)
}
fn source_findings_at(
    error: &Error,
    source: &SourceDeclaration,
    depth: usize,
) -> Result<Option<Vec<Value>>, crate::source_diagnostics::SourceProjectionError> {
    let projected = crate::specification_diagnostics::findings_with_limit(
        error,
        Some(source),
        yamaa_core::diagnostic::MAX_PROJECTION_BYTES,
    )?;
    if projected.is_some() {
        return Ok(projected);
    }
    if let Error::Sources(errors) = error {
        if depth == crate::source_diagnostics::MAX_SOURCE_REPORT_FRAMES {
            return Err(crate::source_diagnostics::SourceProjectionError::TraversalLimit);
        }
        let mut findings = Vec::new();
        for (source, error) in errors {
            let Some(projected) = source_findings_at(error, source, depth + 1)? else {
                return Ok(None);
            };
            findings.extend(projected);
        }
        return Ok(Some(findings));
    }
    Ok(self::source(error, source)
        .and_then(|refusal| refusal.value("ingest", "source_limit"))
        .map(|finding| vec![finding]))
}
pub(crate) fn run_limit<E>(
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

pub(crate) fn outcome(
    value: &Value,
    sources: &[SourceDeclaration],
    driver_index: usize,
) -> Option<Refusal> {
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
    if let Some(source_index) = value.get("source_index") {
        paths = vec![source_path(
            sources,
            driver_index,
            usize::try_from(source_index.as_u64()?).ok()?,
        )?];
    } else if paths.is_empty() && matches!(resource, "rows" | "cells" | "columns" | "sources") {
        paths = input_paths(sources);
    }
    Some(Refusal::new(resource, limit, actual, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_projection_refusals_survive_resource_fallback() {
        use crate::source_diagnostics::SourceProjectionError;
        use yamaa_core::diagnostic::{ProjectionError, MAX_PROJECTION_BYTES};
        let error = Error::TypedSource(crate::typed_csv::Error::Typing(
            yamaa_core::typed_csv::Error::FieldParse {
                field: "N".into(),
                target: yamaa_core::value::ColumnType::Int,
                value: "x".repeat(MAX_PROJECTION_BYTES / 6),
            },
        ));
        assert_eq!(
            source_findings(&error, &input()),
            Err(SourceProjectionError::Context(ProjectionError::Limit))
        );
        let mut error = Error::Source(TextTableError::Csv(
            crate::csv_source::parse(b"", Default::default()).unwrap_err(),
        ));
        for _ in 0..=crate::source_diagnostics::MAX_SOURCE_REPORT_FRAMES {
            error = Error::Sources(vec![(input(), error)]);
        }
        assert_eq!(
            source_findings(&error, &input()),
            Err(SourceProjectionError::TraversalLimit)
        );
    }
    fn input() -> SourceDeclaration {
        SourceDeclaration {
            name: "XW".into(),
            path: "xw.csv".into(),
            profile: yamaa_core::specification::SourceProfile::Csv,
            encoding: None,
            types: Vec::new(),
            ordinal: None,
            empty_string_present: false,
        }
    }
    #[test]
    fn actual_sas_reader_quota_keeps_its_bound_and_input_owner() {
        let mut declaration = input();
        declaration.path = "v5.xpt".into();
        declaration.profile = yamaa_core::specification::SourceProfile::Xport;
        declaration.encoding = Some("windows-1252".into());
        let failure = crate::sas_source::parse(
            include_bytes!("../tests/fixtures/sas/v5.xpt"),
            &declaration,
            crate::sas_source::Limits {
                source_bytes: 1_048_576,
                work_bytes: 4_194_304,
                retained_bytes: 4_194_304,
            },
            crate::arrow_table::TableLimits {
                max_rows: 1,
                max_columns: 1024,
                max_cells: 1024,
                max_batches: 1,
            },
        )
        .unwrap_err();
        let failure = Error::SasSource(failure);
        let refusal = source(&failure, &declaration).unwrap();
        assert_eq!(refusal.resource, "rows");
        assert_eq!(refusal.limit, 1);
        assert_eq!(refusal.actual, Some(2));
        assert_eq!(refusal.spec_paths, ["input.XW"]);
        let projected: Value = serde_json::from_str(&crate::specification_diagnostics::failure(
            &failure,
            Some(&declaration),
        ))
        .unwrap();
        assert_eq!(projected["outcome"]["stage"], "ingest");
        assert_eq!(projected["outcome"]["code"], "sas_limit");
        assert_eq!(projected["outcome"]["resource"], "rows");
        assert_eq!(projected["outcome"]["limit"], 1);
        assert_eq!(projected["outcome"]["actual"], 2);
        assert_eq!(
            projected["outcome"]["spec_paths"],
            serde_json::json!(["input.XW"])
        );
    }

    #[test]
    fn column_refusal_projects_the_source_coordinate_with_a_non_first_driver() {
        let sources = ["FIRST", "DRIVER", "LAST"].map(|name| SourceDeclaration {
            name: name.into(),
            ..input()
        });
        for (source_index, owner) in [(0, "DRIVER"), (1, "FIRST"), (2, "LAST")] {
            let error = ExecutionError::<core::convert::Infallible>::SourceLimit {
                resource: Resource::SourceColumns,
                limit: 1,
                required: Some(2),
                source_index,
            };
            let projected = execution(&error, &sources, 1).unwrap();
            assert_eq!(projected.spec_paths, [format!("input.{owner}")]);
            let encoded = crate::dataset_result::semantic_condition(&error).unwrap();
            assert_eq!(encoded["source_index"], source_index);
            assert_eq!(outcome(&encoded, &sources, 1), Some(projected));
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
