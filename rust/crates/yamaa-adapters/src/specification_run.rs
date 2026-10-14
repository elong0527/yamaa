//! Execute prepared YAML specifications over captured schema and typed snapshots.
//! Source ports supply snapshots; publication belongs to the public Save service.
use crate::{
    arrow_table::{ArrowTable, TableLimits},
    csv_source::{self, TextTableError},
    dataset_result::{self, DatasetError, DatasetResponse},
    specification_source::PreparedDocument,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use yamaa_engine::domain::{self, CheckedSpecification};
use yamaa_engine::specification::{
    BindError, PrepareError, PreparedSpecification, SourceDeclaration,
};
use yamaa_engine::specification_run as application;
#[derive(Debug)]
pub enum Error {
    ResourceLimit(crate::resource_diagnostics::Refusal),
    Prepare(PrepareError),
    Source(TextTableError),
    TypedSource(crate::typed_csv::Error),
    ParquetSource(crate::parquet_source::Error),
    SasSource(crate::sas_source::Error),
    Ordinal(yamaa_engine::source_ingestion::Error),
    OdmFields(Vec<yamaa_core::odm_source::SchemaError>),
    OdmValues(Vec<yamaa_core::odm_source::MissingValues>),
    Sources(Vec<(SourceDeclaration, Error)>),
    Bind(BindError),
    Execution(DatasetError),
}
pub use yamaa_engine::specification_run::{InspectionFailure, SourcePort, SourceRead};
#[derive(Debug)]
pub enum PortError<E> {
    Capture(E),
    Inspect(Vec<InspectionFailure<E>>),
    Run(Error),
    CaptureAccounting,
}
/// Observations survive failed binding/execution. The table is the exact owned
/// source snapshot used for execution; reporting never reopens or reparses data.
pub type CapturedSource = application::CapturedSource<ArrowTable>;
pub struct CapturedAttempt<E> {
    pub sources: Vec<CapturedSource>,
    pub result: Result<DatasetResponse, PortError<E>>,
}

/// Keeps schema/source provenance alive through binding and execution.
#[derive(Debug)]
pub struct PreparedRun {
    document: PreparedDocument,
    checked: CheckedSpecification,
}
impl PreparedRun {
    /// Entire vocabulary admission precedes obtaining a study-data snapshot.
    pub fn prepare(document: PreparedDocument) -> Result<Self, Error> {
        Self::prepare_with_profile_inspection(document, |_, _, _| None)
    }
    pub(crate) fn prepare_with_profile_inspection(
        document: PreparedDocument,
        mut inspect: impl FnMut(
            &PreparedDocument,
            &str,
            &str,
        ) -> Option<(yamaa_core::resource::ResourceFailure, String)>,
    ) -> Result<Self, Error> {
        let checked = domain::check(document.compilation_model()).map_err(|error| {
            Error::Prepare(yamaa_engine::source_profile_inspection::refine(
                error,
                |dataset, path| inspect(&document, dataset, path),
            ))
        })?;
        Ok(Self { document, checked })
    }
    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }
    pub fn key_names(&self) -> impl Iterator<Item = &str> {
        self.compiled().key_names()
    }
    pub fn compiled(&self) -> &PreparedSpecification {
        self.checked.compiled()
    }
    /// Read static findings from the same engine capability used by build.
    pub fn check_diagnostics(&self) -> Vec<yamaa_core::diagnostic::Diagnostic> {
        self.checked.check_diagnostics()
    }
    pub fn source(&self) -> &SourceDeclaration {
        self.compiled().source()
    }
    /// The shared application service decodes, binds and executes held bytes.
    pub fn execute_csv(&self, bytes: &[u8]) -> Result<DatasetResponse, Error> {
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            application::execute_bytes(self.compiled(), bytes, &mut SourceDecoder, limits())
        }))
        .map_err(|_| Error::Execution(DatasetError::Internal))?;
        let execution = attempt
            .result
            .map_err(|error| run_error(error, self.compiled()))?;
        response(execution)
    }

    /// Delegate capture and execution order to the engine. Preserve observations
    /// outside the panic fence; reporting never captures or decodes a source again.
    pub fn execute_with_port<P: SourcePort>(&self, port: &mut P) -> CapturedAttempt<P::Error> {
        self.execute_with_observer(
            port,
            yamaa_engine::application_observer::Scope::new(
                &yamaa_engine::application_observer::Noop,
            ),
        )
    }
    pub fn execute_with_observer<P: SourcePort>(
        &self,
        port: &mut P,
        scope: yamaa_engine::application_observer::Scope<'_>,
    ) -> CapturedAttempt<P::Error> {
        let mut attempt = application::CapturedAttempt::new(self.source());
        let guarded = catch_unwind(AssertUnwindSafe(|| {
            self.checked.build_with_observer_into(
                port,
                &mut SourceDecoder,
                &mut yamaa_engine::function_invocation::NoFunctions::new(),
                limits(),
                &mut attempt,
                scope,
            );
        }));
        let result = if guarded.is_err() {
            Err(PortError::Run(Error::Execution(DatasetError::Internal)))
        } else {
            match attempt.result {
                Ok(execution) => response(execution).map_err(PortError::Run),
                Err(application::PortError::Incomplete) => {
                    Err(PortError::Run(Error::Execution(DatasetError::Internal)))
                }
                Err(application::PortError::Capture(error)) => Err(PortError::Capture(error)),
                Err(application::PortError::Inspect(errors)) => Err(PortError::Inspect(errors)),
                Err(application::PortError::Run(error)) => {
                    Err(PortError::Run(run_error(error, self.compiled())))
                }
                Err(application::PortError::CaptureAccounting) => Err(PortError::CaptureAccounting),
            }
        };
        CapturedAttempt {
            sources: attempt
                .sources
                .into_iter()
                .map(|source| CapturedSource {
                    read: source.read,
                    snapshot: source.snapshot,
                    table: source.table.map(|table| table.0),
                })
                .collect(),
            result,
        }
    }
}

pub(crate) fn limits() -> application::Limits {
    application::Limits {
        source_bytes: yamaa_engine::capacity::SOURCE_BYTES,
        source_cells: dataset_result::MAX_SOURCE_CELLS,
        execution: dataset_result::LIMITS,
    }
}

fn response(
    execution: yamaa_engine::dataset::ExecutionAttempt<crate::callback::CallbackError>,
) -> Result<DatasetResponse, Error> {
    catch_unwind(AssertUnwindSafe(|| {
        dataset_result::specification_response(execution)
    }))
    .map_err(|_| Error::Execution(DatasetError::Internal))?
    .map_err(Error::Execution)
}

fn run_error(error: application::RunError<Error>, prepared: &PreparedSpecification) -> Error {
    match error {
        application::RunError::Decode(error) => error,
        application::RunError::Sources(mut errors) if prepared.sources().len() == 1 => {
            errors.pop().expect("source failure").1
        }
        application::RunError::Sources(errors) => Error::Sources(
            errors
                .into_iter()
                .map(|(index, error)| (prepared.sources()[index].clone(), error))
                .collect(),
        ),
        application::RunError::AdmittedSources(errors) => Error::Sources(
            errors
                .into_iter()
                .map(|(index, error)| {
                    (
                        prepared.sources()[index].clone(),
                        match error {
                            application::SourceIssue::Decode(error) => error,
                            application::SourceIssue::OdmValues(values) => Error::OdmValues(values),
                        },
                    )
                })
                .collect(),
        ),
        application::RunError::Bind(error) => Error::Bind(error),
        error @ (application::RunError::SourceBytes { .. }
        | application::RunError::SourceCells { .. }) => Error::ResourceLimit(
            crate::resource_diagnostics::run_limit(&error, prepared.sources())
                .expect("source budget owner"),
        ),
    }
}

pub(crate) struct SourceDecoder;
impl application::SourceDecoder for SourceDecoder {
    type Error = Error;
    type Table = dataset_result::Snapshot;
    fn continue_after(&self, error: &Self::Error) -> bool {
        matches!(
            error,
            Error::Source(TextTableError::Csv(csv_source::Error::Profile { .. }))
                | Error::Ordinal(yamaa_engine::source_ingestion::Error::Collision)
                | Error::TypedSource(crate::typed_csv::Error::Typing(
                    yamaa_core::typed_csv::Error::UnknownField { .. }
                        | yamaa_core::typed_csv::Error::FieldParse { .. }
                ))
                | Error::ParquetSource(
                    crate::parquet_source::Error::Malformed
                        | crate::parquet_source::Error::EmptyName { .. }
                        | crate::parquet_source::Error::DuplicateName { .. }
                        | crate::parquet_source::Error::Unsupported { .. }
                        | crate::parquet_source::Error::Value { .. }
                )
                | Error::OdmFields(_)
                | Error::SasSource(
                    crate::sas_source::Error::Invalid(_) | crate::sas_source::Error::Typing(_)
                )
                | Error::OdmValues(_)
        )
    }
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
    ) -> Result<Self::Table, Self::Error> {
        decode_source(source, bytes, None).map(dataset_result::Snapshot)
    }
    fn decode_with_interpretation(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: yamaa_core::specification::SourceInterpretation,
    ) -> Result<Self::Table, Self::Error> {
        decode_interpreted(source, bytes, None, interpretation).map(dataset_result::Snapshot)
    }
}

pub(crate) fn decode_source(
    source: &SourceDeclaration,
    bytes: &[u8],
    bounded: Option<yamaa_engine::producer_build::DecodeLimits>,
) -> Result<ArrowTable, Error> {
    decode_interpreted(
        source,
        bytes,
        bounded,
        yamaa_core::specification::SourceInterpretation::Ordinary,
    )
}

pub(crate) fn decode_interpreted(
    source: &SourceDeclaration,
    bytes: &[u8],
    bounded: Option<yamaa_engine::producer_build::DecodeLimits>,
    interpretation: yamaa_core::specification::SourceInterpretation,
) -> Result<ArrowTable, Error> {
    decode_interpreted_inner(source, bytes, bounded, interpretation, None)
}

pub(crate) fn decode_interpreted_with_usage(
    source: &SourceDeclaration,
    bytes: &[u8],
    limits: yamaa_engine::producer_build::DecodeLimits,
    interpretation: yamaa_core::specification::SourceInterpretation,
    usage: &mut crate::source_decode_usage::Usage,
) -> Result<ArrowTable, Error> {
    usage.complete();
    decode_interpreted_inner(source, bytes, Some(limits), interpretation, Some(usage))
}

fn decode_interpreted_inner(
    source: &SourceDeclaration,
    bytes: &[u8],
    bounded: Option<yamaa_engine::producer_build::DecodeLimits>,
    interpretation: yamaa_core::specification::SourceInterpretation,
    mut usage: Option<&mut crate::source_decode_usage::Usage>,
) -> Result<ArrowTable, Error> {
    let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
    let cells = bounded.map_or(yamaa_engine::capacity::SOURCE_CELLS, |l| {
        l.cells.min(yamaa_engine::capacity::SOURCE_CELLS)
    });
    let table_limits = TableLimits {
        max_rows: bounded.map_or(yamaa_engine::capacity::SOURCE_ROWS, |l| {
            l.rows.min(yamaa_engine::capacity::SOURCE_ROWS)
        }),
        max_columns: yamaa_core::specification::DEFAULT_MAX_COLUMNS,
        max_batches: 1,
        max_cells: cells,
    };
    let table = if source.profile == yamaa_core::specification::SourceProfile::Parquet {
        if let Some(usage) = usage.as_deref_mut() {
            decode_parquet_with_usage(source, bytes, bounded, odm, Some(usage))?
        } else {
            decode_parquet(source, bytes, bounded, odm)?
        }
    } else if matches!(
        source.profile,
        yamaa_core::specification::SourceProfile::Xport
            | yamaa_core::specification::SourceProfile::Sas7bdat
    ) {
        let table = crate::sas_source::parse(
            bytes,
            source,
            crate::sas_source::Limits {
                source_bytes: csv_source::Limits::default().bytes,
                work_bytes: bounded.map_or(
                    yamaa_engine::capacity::SOURCE_STORAGE_BYTES,
                    |limits| {
                        limits
                            .work_bytes
                            .min(yamaa_engine::capacity::SOURCE_STORAGE_BYTES)
                    },
                ),
                retained_bytes: bounded.map_or(
                    yamaa_engine::capacity::SOURCE_STORAGE_BYTES,
                    |limits| {
                        limits
                            .storage_bytes
                            .min(yamaa_engine::capacity::SOURCE_STORAGE_BYTES)
                    },
                ),
            },
            table_limits,
        )
        .map_err(Error::SasSource)?;
        if odm {
            project_odm(table, table_limits)?
        } else {
            table
        }
    } else {
        let csv_limits = bounded.map_or_else(csv_source::Limits::default, bounded_csv_limits);
        if odm {
            let parsed = if let (Some(limits), Some(usage)) = (bounded, usage.as_deref_mut()) {
                crate::source_decode_usage::parse_csv(bytes, csv_limits, limits, usage)?
            } else {
                crate::csv_source::parse(bytes, csv_limits)
                    .map_err(|error| Error::Source(TextTableError::Csv(error)))?
            };
            let fields = parsed
                .names
                .iter()
                .map(|name| yamaa_core::odm_source::StoredField {
                    name,
                    kind: Some(yamaa_core::value::ColumnType::Str),
                })
                .collect::<Vec<_>>();
            let binding = yamaa_core::odm_source::bind_fields(&fields)
                .map_err(|error| {
                    let yamaa_core::odm_source::AdmissionError::MetadataLimit {
                        resource,
                        limit,
                        actual,
                    } = error;
                    Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
                        resource,
                        limit,
                        actual,
                        vec![format!("input.{}", source.name)],
                    ))
                })?
                .map_err(Error::OdmFields)?;
            if let (Some(limits), Some(usage)) = (bounded, usage.as_deref_mut()) {
                crate::source_decode_usage::csv_conversion(&parsed, limits.storage_bytes, usage)
                    .map_err(|error| Error::Source(TextTableError::Table(error)))?;
            }
            let projected = crate::csv_source::CsvSource {
                names: yamaa_core::odm_source::FIELDS
                    .iter()
                    .map(|name| (*name).into())
                    .collect(),
                records: parsed
                    .records
                    .into_iter()
                    .map(|mut row| {
                        binding
                            .stored()
                            .iter()
                            .map(|&column| row[column].take())
                            .collect()
                    })
                    .collect(),
            };
            crate::typed_csv::convert(
                projected,
                yamaa_core::typed_csv::PreparedTypes::new(&[], 11)
                    .expect("fixed text schema has no authored type declarations"),
                TableLimits {
                    max_columns: 11,
                    ..table_limits
                },
            )
            .map_err(typed_source_error)?
        } else if let (Some(limits), Some(usage)) = (bounded, usage.as_deref_mut()) {
            let types =
                yamaa_core::typed_csv::PreparedTypes::new(&source.types, table_limits.max_columns)
                    .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
            let parsed = crate::source_decode_usage::parse_csv(bytes, csv_limits, limits, usage)?;
            crate::source_decode_usage::csv_conversion(&parsed, limits.storage_bytes, usage)
                .map_err(|error| Error::Source(TextTableError::Table(error)))?;
            crate::typed_csv::convert(parsed, types, table_limits).map_err(typed_source_error)?
        } else {
            crate::typed_csv::parse(bytes, &source.types, csv_limits, table_limits)
                .map_err(typed_source_error)?
        }
    };
    // ODM interpretation has already selected the fixed canonical fields.
    // Its ordinal declaration is rejected by core before decoding begins.
    if let (Some(limits), Some(usage)) = (bounded, usage) {
        crate::source_ordinal::append_with_usage(
            source,
            table,
            table_limits,
            limits.storage_bytes,
            usage,
        )
    } else {
        crate::source_ordinal::append(source, table, table_limits)
    }
}

pub(crate) fn typed_source_error(error: crate::typed_csv::Error) -> Error {
    match error {
        crate::typed_csv::Error::Csv(error) => Error::Source(TextTableError::Csv(error)),
        crate::typed_csv::Error::Table(error) => Error::Source(TextTableError::Table(error)),
        error => Error::TypedSource(error),
    }
}

fn project_odm(table: ArrowTable, table_limits: TableLimits) -> Result<ArrowTable, Error> {
    use yamaa_core::table::{Column, TableAccess, TableSchema};
    let fields = table
        .schema()
        .columns()
        .iter()
        .map(|column| yamaa_core::odm_source::StoredField {
            name: column.name.as_str(),
            kind: Some(column.kind),
        })
        .collect::<Vec<_>>();
    let binding = yamaa_core::odm_source::bind_fields(&fields)
        .map_err(|_| {
            Error::SasSource(crate::sas_source::Error::Limit {
                resource: "odm_metadata",
                limit: 1_048_576,
                actual: None,
            })
        })?
        .map_err(Error::OdmFields)?;
    let schema = TableSchema::new(
        yamaa_core::odm_source::FIELDS
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: yamaa_core::value::ColumnType::Str,
            })
            .collect(),
    )
    .map_err(|_| Error::SasSource(crate::sas_source::Error::Internal))?;
    let batches = table
        .batches()
        .iter()
        .map(|batch| {
            batch
                .project(binding.stored())
                .map_err(|_| Error::SasSource(crate::sas_source::Error::Internal))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ArrowTable::try_new(
        schema,
        batches,
        TableLimits {
            max_columns: 11,
            ..table_limits
        },
    )
    .map_err(|_| Error::SasSource(crate::sas_source::Error::Internal))
}
pub(crate) fn bounded_csv_limits(
    limits: yamaa_engine::producer_build::DecodeLimits,
) -> csv_source::Limits {
    let ordinary = csv_source::Limits::default();
    let cells = limits.cells.min(yamaa_engine::capacity::SOURCE_CELLS);
    csv_source::Limits {
        bytes: ordinary
            .bytes
            .min(limits.storage_bytes)
            .min(limits.work_bytes),
        // Charges precede each temporary field, including at most 1,024 headers.
        fields: ordinary
            .fields
            .min(cells.saturating_add(yamaa_core::specification::DEFAULT_MAX_COLUMNS)),
        records: ordinary
            .records
            .min(limits.rows.saturating_add(1))
            .min(cells.saturating_add(1)),
    }
}

fn decode_parquet(
    source: &SourceDeclaration,
    bytes: &[u8],
    bounded: Option<yamaa_engine::producer_build::DecodeLimits>,
    odm: bool,
) -> Result<ArrowTable, Error> {
    decode_parquet_with_usage(source, bytes, bounded, odm, None)
}

fn decode_parquet_with_usage(
    source: &SourceDeclaration,
    bytes: &[u8],
    bounded: Option<yamaa_engine::producer_build::DecodeLimits>,
    odm: bool,
    usage: Option<&mut crate::source_decode_usage::Usage>,
) -> Result<ArrowTable, Error> {
    use crate::parquet_source::{self, CompressionLimits, FramingLimits, MetadataLimits};
    use arrow_array::{Array, ArrayRef, RecordBatch, StringArray};
    use std::sync::Arc;
    use yamaa_core::table::TableAccess;
    let table_limits = TableLimits {
        max_rows: bounded.map_or(yamaa_engine::capacity::SOURCE_ROWS, |l| {
            l.rows.min(yamaa_engine::capacity::SOURCE_ROWS)
        }),
        max_columns: yamaa_core::specification::DEFAULT_MAX_COLUMNS,
        max_batches: 1,
        max_cells: bounded.map_or(yamaa_engine::capacity::SOURCE_CELLS, |l| {
            l.cells.min(yamaa_engine::capacity::SOURCE_CELLS)
        }),
    };
    let work_bytes = bounded.map_or(yamaa_engine::capacity::SOURCE_STORAGE_BYTES, |l| {
        l.work_bytes
            .min(yamaa_engine::capacity::SOURCE_STORAGE_BYTES)
    });
    let storage_bytes = bounded.map_or(yamaa_engine::capacity::SOURCE_STORAGE_BYTES, |l| {
        l.storage_bytes
            .min(yamaa_engine::capacity::SOURCE_STORAGE_BYTES)
    });
    let limits = parquet_source::Limits {
        framing: FramingLimits {
            source_bytes: csv_source::Limits::default().bytes,
            // Compact nodes need at least one encoded byte. Headers and
            // footers remain inside the independently admitted source.
            metadata_bytes: yamaa_engine::capacity::SOURCE_BYTES,
            metadata_nodes: yamaa_engine::capacity::SOURCE_BYTES,
            header_bytes: yamaa_engine::capacity::SOURCE_BYTES,
            header_nodes: yamaa_engine::capacity::SOURCE_BYTES,
            row_groups: yamaa_engine::dataset::ExecutionBudget::longitudinal().max_batches,
            columns: 4096,
            rows: table_limits.max_rows,
            cells: table_limits.max_cells,
            pages: yamaa_engine::capacity::SOURCE_CELLS,
            page_bytes: yamaa_engine::capacity::SOURCE_BYTES,
            decoded_bytes: work_bytes,
        },
        compression: CompressionLimits {
            page_bytes: yamaa_engine::capacity::SOURCE_BYTES,
            window_log: 24,
        },
        metadata: MetadataLimits {
            bytes: yamaa_engine::capacity::SOURCE_BYTES,
            tables: yamaa_engine::capacity::SOURCE_BYTES,
            depth: 64,
        },
        expanded_bytes: work_bytes,
        retained_bytes: storage_bytes,
        array_elements: table_limits.max_cells,
        batches: table_limits.max_batches,
    };
    let table = if let Some(usage) = usage {
        parquet_source::parse_with_usage(bytes, limits, table_limits, odm, usage)
    } else if odm {
        parquet_source::parse_odm(bytes, limits, table_limits)
    } else {
        parquet_source::parse(bytes, limits, table_limits)
    }
    .map_err(|error| match error {
        parquet_source::Error::OdmFields(fields) => Error::OdmFields(fields),
        error => Error::ParquetSource(error),
    })?;
    // The compiler owns the input policy. Preserve buffers when no empty value
    // needs a new validity bit; all strings were admitted by the bounded codec.
    let batches = table
        .batches()
        .iter()
        .map(|batch| {
            let arrays: Vec<ArrayRef> = batch
                .columns()
                .iter()
                .map(|array| {
                    let Some(strings) = array.as_any().downcast_ref::<StringArray>() else {
                        return array.clone();
                    };
                    if !strings
                        .iter()
                        .flatten()
                        .any(|text| source.text_is_missing(text))
                    {
                        return array.clone();
                    }
                    Arc::new(StringArray::from_iter(strings.iter().map(|value| {
                        value.filter(|value| !source.text_is_missing(value))
                    }))) as ArrayRef
                })
                .collect();
            RecordBatch::try_new(batch.schema(), arrays).map_err(|error| {
                Error::Source(TextTableError::Table(
                    crate::arrow_table::TableError::Arrow(error),
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    ArrowTable::try_new(table.schema().clone(), batches, table_limits)
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}

#[cfg(test)]
mod width_tests {
    use super::*;
    use crate::arrow_table::TableError;
    use yamaa_core::{
        specification::SourceProfile,
        table::{TableAccess, ValueRef},
        value::ColumnType,
    };

    #[test]
    fn parquet_decode_admits_small_groups_beyond_prior_group_and_metadata_frontiers() {
        use arrow_array::{Int64Array, RecordBatch};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
        use std::sync::Arc;

        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            empty_string_present: false,
            ordinal: None,
        };
        let schema = Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, true)]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from_iter_values(0..5000))],
        )
        .unwrap();
        let properties = WriterProperties::builder()
            .set_max_row_group_row_count(Some(1))
            .build();
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        let table = decode_parquet(&source, &bytes, None, false).unwrap();
        assert_eq!(table.row_count(), 5000);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(0));
        assert_eq!(table.cell(4999, 0).unwrap(), ValueRef::Int(4999));
    }

    #[test]
    fn parquet_decode_admits_1024_and_refuses_1025_source_fields() {
        use arrow_array::{ArrayRef, RecordBatch, StringArray};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;

        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            empty_string_present: false,
            ordinal: None,
        };
        for width in [1_024, 1_025] {
            let schema = Arc::new(Schema::new(
                (0..width)
                    .map(|column| Field::new(format!("F{column:04}"), DataType::Utf8, true))
                    .collect::<Vec<_>>(),
            ));
            let value = Arc::new(StringArray::from(vec!["x"])) as ArrayRef;
            let batch = RecordBatch::try_new(schema.clone(), vec![value; width]).unwrap();
            let mut bytes = Vec::new();
            let mut writer = ArrowWriter::try_new(&mut bytes, schema, None).unwrap();
            writer.write(&batch).unwrap();
            writer.close().unwrap();
            let result = decode_source(&source, &bytes, None);
            if width == 1_024 {
                let table = result.unwrap();
                assert_eq!(table.schema().columns().len(), 1_024);
                assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            } else {
                assert!(matches!(
                    result,
                    Err(Error::ParquetSource(crate::parquet_source::Error::Table(
                        TableError::Limit {
                            resource: "columns",
                            limit: 1024,
                            required: Some(1025)
                        }
                    )))
                ));
            }
        }
    }

    #[test]
    fn csv_decode_admits_1024_typed_and_untyped_fields_with_bounded_headers() {
        let names = (0..1_024)
            .map(|column| format!("F{column:04}"))
            .collect::<Vec<_>>();
        let bytes = format!("{}\n{}\n", names.join(","), vec!["x"; 1_024].join(","));
        for typed in [false, true] {
            let source = SourceDeclaration {
                name: "SRC".into(),
                path: "input.csv".into(),
                types: if typed {
                    names
                        .iter()
                        .map(|name| (name.clone(), ColumnType::Str))
                        .collect()
                } else {
                    vec![]
                },
                profile: SourceProfile::Csv,
                encoding: None,
                empty_string_present: false,
                ordinal: None,
            };
            // One complete header plus one row must fit: the old 64-header
            // allowance refused this in-range table before width admission.
            let limits = yamaa_engine::producer_build::DecodeLimits {
                rows: yamaa_engine::capacity::SOURCE_ROWS,
                cells: 1_024,
                storage_bytes: 1_000_000,
                work_bytes: 1_000_000,
            };
            let table = decode_source(&source, bytes.as_bytes(), Some(limits)).unwrap();
            assert_eq!(table.schema().columns().len(), 1_024);
            assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            let excess = format!("{},EXCESS\n", names.join(","));
            assert!(matches!(
                decode_source(&source, excess.as_bytes(), None),
                Err(Error::Source(TextTableError::Table(TableError::Limit {
                    resource: "columns",
                    limit: 1_024,
                    required: Some(1_025)
                })))
            ));
        }
    }
}
