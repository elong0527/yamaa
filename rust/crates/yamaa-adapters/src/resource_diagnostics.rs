//! Typed resource policies retain their applied bounds and authored owners.
use crate::{csv_source::TextTableError, specification_run::Error};
use yamaa_core::application_issue::{Fault, Stage};
use yamaa_core::diagnostic::Diagnostic;
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
    pub fn diagnostic(&self, stage: Stage, fault: Fault) -> Diagnostic {
        yamaa_core::application_issue::resource_rejected(
            stage,
            fault,
            self.resource,
            self.limit,
            self.actual,
            self.spec_paths.clone(),
        )
    }
    pub fn issues(&self, stage: Stage, fault: Fault) -> Vec<Diagnostic> {
        vec![self.diagnostic(stage, fault)]
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
        Error::Source(TextTableError::Csv(crate::csv_source::Error::Limit {
            resource,
            limit,
            actual,
        })) => (*resource, *limit, *actual),
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
) -> Result<Option<Vec<Diagnostic>>, crate::source_diagnostics::SourceProjectionError> {
    let mut pending = vec![(source, error)];
    let mut findings = Vec::new();
    while let Some((source, error)) = pending.pop() {
        if let Some(projected) = crate::specification_diagnostics::findings(error, Some(source))? {
            findings.extend(projected);
        } else if let Error::Sources(errors) = error {
            pending.extend(errors.iter().rev().map(|(source, error)| (source, error)));
        } else if let Some(refusal) = self::source(error, source) {
            findings.push(refusal.diagnostic(Stage::Ingest, Fault::SourceLimit));
        } else {
            return Ok(None);
        }
    }
    Ok(Some(findings))
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

/// The refusal of one execution bound, naming its source when it has one.
pub(crate) fn outcome(
    bound: &crate::dataset_result::Limit,
    sources: &[SourceDeclaration],
    driver_index: usize,
) -> Option<Refusal> {
    let resource = bound.resource;
    if !matches!(
        resource,
        "rows"
            | "cells"
            | "columns"
            | "sources"
            | "output_rows"
            | "output_cells"
            | "output_columns"
            | "table_rows"
            | "table_cells"
    ) {
        return None;
    }
    let limit = bound.limit?;
    let mut paths = bound.spec_paths.clone();
    if let Some(source_index) = bound.source_index {
        paths = vec![source_path(sources, driver_index, source_index)?];
    } else if paths.is_empty() && matches!(resource, "rows" | "cells" | "columns" | "sources") {
        paths = input_paths(sources);
    }
    Some(Refusal::new(resource, limit, bound.required, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::json::Value;
    #[test]
    fn source_resource_fallback_preserves_complete_context_and_nested_findings() {
        let value = "x".repeat(2_000_000);
        let error = Error::TypedSource(crate::typed_csv::Error::Typing(
            yamaa_core::typed_csv::Error::FieldParse {
                field: "N".into(),
                target: yamaa_core::value::ColumnType::Int,
                value: value.clone(),
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
            resource: "table_rows",
            limit: 10_000_000,
            actual: Some(10_000_001),
        };
        let failure = Error::SasSource(failure);
        let refusal = source(&failure, &declaration).unwrap();
        assert_eq!(refusal.resource, "table_rows");
        assert_eq!(refusal.limit, 10_000_000);
        assert_eq!(refusal.actual, Some(10_000_001));
        assert_eq!(refusal.spec_paths, ["input.XW"]);
        let crate::specification_diagnostics::Rejection::Fault(projected) =
            crate::specification_diagnostics::failure(&failure, Some(&declaration))
        else {
            panic!("a fixed source bound is an engine fault");
        };
        assert_eq!(projected.context["stage"], "ingest");
        assert_eq!(projected.context["code"], "sas_limit");
        assert_eq!(projected.context["resource"], "table_rows");
        assert_eq!(projected.context["limit"], 10_000_000);
        assert_eq!(projected.context["actual"], 10_000_001);
        assert_eq!(projected.spec_paths, ["input.XW"]);
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
            let crate::dataset_result::Outcome::Limit(bound) =
                crate::dataset_result::semantic_condition(&error).unwrap()
            else {
                panic!("a source bound is a limit outcome");
            };
            assert_eq!(bound.source_index, Some(source_index));
            assert_eq!(outcome(&bound, &sources, 1), Some(projected));
        }
    }
    #[test]
    fn intermediate_table_limits_keep_their_bounds_through_the_execution_outcome() {
        use core::convert::Infallible;
        use yamaa_core::{
            limits::Limits, table_grouping::GroupingError, table_reduction::TableReductionError,
        };

        let sources = [input()];
        for limit in [
            Limits::table(Limits::TABLE_ROWS + 1, 1).unwrap_err(),
            Limits::table(3_575_419, 179).unwrap_err(),
        ] {
            for error in [
                ExecutionError::<Infallible>::Grouping(GroupingError::Limit(limit)),
                ExecutionError::OutputGrouping(GroupingError::Limit(limit)),
                ExecutionError::Reduction {
                    path: "row_columns.TOTAL".into(),
                    error: TableReductionError::Limit(limit),
                    identity: None,
                },
            ] {
                let crate::dataset_result::Outcome::Limit(bound) =
                    crate::dataset_result::semantic_condition(&error).unwrap()
                else {
                    panic!("a table bound is a limit outcome");
                };
                let expected = Refusal::new(limit.resource, limit.limit, limit.actual, Vec::new());
                assert_eq!(bound.resource, limit.resource);
                assert_eq!(bound.limit, Some(limit.limit));
                assert_eq!(bound.required, limit.actual);
                assert_eq!(execution(&error, &sources, 0), Some(expected.clone()));
                assert_eq!(outcome(&bound, &sources, 0), Some(expected));
            }
        }
    }
    #[test]
    fn fixed_source_geometry_refusals_keep_the_actual_count_and_input() {
        crate::csv_source::parse_text_table(b"ID\n1\n2\n").unwrap();
        for (resource, limit, actual) in [
            ("table_rows", 10_000_000, 10_000_001),
            ("table_cells", 640_000_000, 640_000_001),
        ] {
            let error = Error::Source(TextTableError::Table(
                crate::arrow_table::TableError::Limit {
                    resource,
                    limit,
                    required: Some(actual),
                },
            ));
            let refusal = source(&error, &input()).unwrap();
            assert_eq!(
                refusal,
                Refusal::new(resource, limit, Some(actual), vec!["input.XW".into()])
            );
            let finding = refusal.diagnostic(Stage::Ingest, Fault::SourceLimit);
            assert_eq!(finding.context["resource"], resource);
            assert_eq!(finding.context["limit"], Value::from(limit));
            assert_eq!(finding.context["actual"], Value::from(actual));
            assert_eq!(finding.spec_paths, ["input.XW"]);
        }
    }
}
