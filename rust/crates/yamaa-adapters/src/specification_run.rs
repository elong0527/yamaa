//! Execute prepared YAML specifications over captured schema and typed snapshots.
//! The resolver port supplies snapshots; publication belongs to the public Save service.
//!
//! [`PreparedRun`] is the one prepared specification: alone, it carries an
//! empty environment, and with a project it carries the project's admitted
//! environment. Every build of it activates the project functions it calls
//! through one host-functions port, then captures, decodes, binds and executes.
use crate::{
    column_table::ColumnTable,
    csv_source::{self, TextTableError},
    dataset_result::{self, DatasetError, DatasetResponse},
    decoder::{Decoder, Snapshot},
    project_source::{CapturedEnvironment, OwnedEnvironment},
    specification_source::{LinkedSpecification, PreparedDocument},
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_core::host_functions::HostFunctions;
use yamaa_core::project_activation::{Failure, Observations};
use yamaa_core::project_domain::{self, CheckedOwnedProject, RejectedOwnedProject};
use yamaa_core::project_environment::ExecutionEnvironment;
use yamaa_core::specification::{
    BindError, PrepareError, PreparedSpecification, SourceDeclaration,
};
use yamaa_core::specification_run as application;
#[derive(Debug)]
pub enum Error {
    ResourceLimit(crate::resource_diagnostics::Refusal),
    Prepare(PrepareError),
    Source(TextTableError),
    TypedSource(crate::typed_csv::Error),
    ParquetSource(crate::parquet_source::Error),
    SasSource(crate::sas_source::Error),
    Ordinal(yamaa_core::source_ordinal::Error),
    OdmFields(Vec<yamaa_core::odm_source::SchemaError>),
    OdmValues(Vec<yamaa_core::odm_source::MissingValues>),
    /// The ODM XML profile's read of an `.xml` input (ODM-1), or a build
    /// without its reader.
    OdmSource(crate::odm_xml::Error),
    ProducerContract(yamaa_core::producer_contract::Error),
    ProducerMetadata(yamaa_core::diagnostic::Diagnostic),
    Sources(Vec<(SourceDeclaration, Error)>),
    Bind(BindError),
    Execution(DatasetError),
}
pub use yamaa_core::specification_run::{InspectionFailure, PortError, SourceRead};
/// One captured source of an attempt: the exact owned snapshot used for
/// execution; reporting never reopens or reparses data.
pub type CapturedSource<E> = application::CapturedSource<Snapshot<E>>;
/// The dataset half of one attempt: every source it entered and the
/// execution or the port failure that ended it.
pub type CapturedAttempt<C, E> = application::CapturedAttempt<C, Error, Snapshot<E>>;

/// One prepared specification: its document, the environment it was compiled
/// against (empty without a project) and its compilation. Preparation has no
/// runtime or study authority; every build retains its own observations and
/// original host failures, including an unwound host boundary.
#[derive(Debug)]
pub struct PreparedRun {
    document: PreparedDocument,
    captured: CapturedEnvironment,
    checked: CheckedOwnedProject,
}

/// Failed static checking still owns the original documents, lock and metadata.
/// A diagnostic renderer must use these held origins instead of recapturing them.
#[derive(Debug)]
pub struct RejectedRun {
    pub document: PreparedDocument,
    pub captured: CapturedEnvironment,
    pub rejection: Box<RejectedOwnedProject>,
}

pub enum BoundaryFailure<E> {
    Activation(Failure<E>),
    Unwind(Box<dyn Any + Send>),
}
impl<E: std::fmt::Debug> std::fmt::Debug for BoundaryFailure<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Activation(error) => formatter.debug_tuple("Activation").field(error).finish(),
            Self::Unwind(_) => formatter.write_str("Unwind(..)"),
        }
    }
}

/// Boundary success alone does not imply dataset success. The engine's dataset
/// attempt independently owns capture, ingestion, execution and check outcomes.
pub struct Attempt<C, E> {
    pub boundary: Result<(), BoundaryFailure<E>>,
    pub activation: Observations,
    pub dataset: CapturedAttempt<C, E>,
}

impl PreparedRun {
    /// Compile a document alone, without a project. Entire vocabulary
    /// admission precedes obtaining a study-data snapshot. A specification
    /// without a project never reads its environment's host language.
    pub fn prepare(document: PreparedDocument) -> Result<Self, Error> {
        let environment = OwnedEnvironment::without_environment(Some(
            yamaa_core::project_function::Language::Python,
        ));
        Self::prepare_with_environment(document, environment)
            .map_err(|rejected| Error::Prepare(rejected.rejection.error))
    }
    /// Compile a document against an admitted environment.
    pub fn prepare_with_environment(
        document: PreparedDocument,
        environment: OwnedEnvironment,
    ) -> Result<Self, Box<RejectedRun>> {
        Self::prepare_with_profile_inspection(document, &[], environment, |_, _, _| None)
    }
    /// Compile against the admitted environment and the producing
    /// specifications that the document's `schema` links name; each supplies
    /// only its output contract.
    pub(crate) fn prepare_with_profile_inspection(
        document: PreparedDocument,
        links: &[LinkedSpecification],
        environment: OwnedEnvironment,
        mut inspect: impl FnMut(
            &PreparedDocument,
            &str,
            &str,
        ) -> Option<(yamaa_core::resource::ResourceFailure, String)>,
    ) -> Result<Self, Box<RejectedRun>> {
        let (environment, captured) = environment.into_parts();
        let checked = {
            let metadata = crate::specification_source::link_metadata(&document, links);
            let links = yamaa_core::producer_contract::Links {
                consumer: &document.source().identity,
                links: &metadata,
            };
            // A specification binds from the tree its front end lowered
            // (#3030 D76).
            project_domain::check_owned_tree_linked(
                document.tree().clone(),
                document.document(),
                environment,
                links,
            )
        };
        match checked {
            Ok(checked) => Ok(Self {
                document,
                captured,
                checked,
            }),
            Err(mut rejection) => {
                rejection.error = yamaa_core::source_profile_inspection::refine(
                    rejection.error,
                    |dataset, path| inspect(&document, dataset, path),
                );
                Err(Box::new(RejectedRun {
                    document,
                    captured,
                    rejection,
                }))
            }
        }
    }
    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }
    pub(crate) fn reject(self, error: PrepareError) -> Box<RejectedRun> {
        Box::new(RejectedRun {
            document: self.document,
            captured: self.captured,
            rejection: self.checked.reject(error),
        })
    }
    /// The captured project documents, empty without a project.
    pub fn captured_environment(&self) -> &CapturedEnvironment {
        &self.captured
    }
    /// Whether a project file was supplied for this specification.
    pub fn has_project(&self) -> bool {
        self.captured.root_optional().is_some()
    }
    pub fn environment(&self) -> &ExecutionEnvironment {
        self.checked.environment()
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
            application::execute_bytes(
                self.compiled(),
                bytes,
                &mut Decoder::<crate::callback::CallbackError>::default(),
            )
        }))
        .map_err(|_| Error::Execution(DatasetError::Internal))?;
        let execution = attempt
            .result
            .map_err(|error| run_error(error, self.compiled()))?;
        response(execution)
    }

    /// Own evidence outside the host boundary before granting activation or
    /// study authority. No prior attempt, binding or successful test is
    /// reused here. A specification that calls no project function never
    /// enters `functions`, the build's host-functions port; the empty port
    /// refuses one that does before it binds anything or reads any input.
    pub fn execute_with_ports<A: HostFunctions, P: yamaa_core::resolver::Resolver>(
        &self,
        functions: &mut A,
        source: &mut P,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Attempt<P::Error, A::Error> {
        let mut attempt = Attempt {
            boundary: Ok(()),
            activation: Observations::default(),
            dataset: application::CapturedAttempt::new(self.compiled().source()),
        };
        let lock = self
            .captured
            .lock()
            .map_or(&[][..], |lock| &lock.source.bytes[..]);
        attempt.boundary = match catch_unwind(AssertUnwindSafe(|| {
            self.checked.build_into(
                functions,
                lock,
                source,
                &mut Decoder::<A::Error>::default(),
                &mut attempt.dataset,
                Some(&mut attempt.activation),
                scope,
            )
        })) {
            Ok(result) => result.map_err(BoundaryFailure::Activation),
            Err(payload) => Err(BoundaryFailure::Unwind(payload)),
        };
        attempt
    }
}

fn response(
    execution: yamaa_core::dataset::ExecutionAttempt<crate::callback::CallbackError>,
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
        error @ application::RunError::Boundary { .. } => Error::ResourceLimit(
            crate::resource_diagnostics::run_limit(&error, prepared.sources())
                .expect("source boundary owner"),
        ),
    }
}

/// A linked artifact's stored fields, in order and with their decoded types,
/// must equal the producing specification's output contract (INPUT-11). The
/// generated ordinal is appended only after this comparison.
fn stored_contract(
    source: &SourceDeclaration,
    contract: &yamaa_core::producer_contract::Contract,
    table: &ColumnTable,
) -> Result<(), Error> {
    use yamaa_core::table::TableAccess;
    let fields = table
        .schema()
        .columns()
        .iter()
        .map(|field| (field.name.as_str(), field.kind))
        .collect::<Vec<_>>();
    match contract
        .validate_typed_fields(&source.name, &fields)
        .map_err(Error::ProducerContract)?
    {
        Some(diagnostic) => Err(Error::ProducerMetadata(diagnostic)),
        None => Ok(()),
    }
}

/// A frame has already admitted physical types at the Arrow boundary. Preserve
/// those values, enforce explicit source declarations and reuse ODM/ordinal,
/// empty-string and producer-contract admission before ordinary binding and
/// execution.
pub(crate) fn decode_frame(
    source: &SourceDeclaration,
    table: ColumnTable,
    interpretation: yamaa_core::specification::SourceInterpretation,
    contract: Option<&yamaa_core::producer_contract::Contract>,
) -> Result<ColumnTable, Error> {
    use yamaa_core::table::TableAccess;
    // A linked artifact's declared types are its producer's contract; compare
    // the whole stored field list before any single declared field.
    if let Some(contract) = contract {
        stored_contract(source, contract, &table)?;
    }
    yamaa_core::typed_csv::PreparedTypes::new(&source.types)
        .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
    for (name, kind) in &source.types {
        let field = table
            .schema()
            .columns()
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| {
                typed_source_error(crate::typed_csv::Error::Typing(
                    yamaa_core::typed_csv::Error::UnknownField {
                        field: name.clone(),
                    },
                ))
            })?;
        if field.kind != *kind {
            return Err(typed_source_error(crate::typed_csv::Error::Typing(
                yamaa_core::typed_csv::Error::InvalidSource,
            )));
        }
    }
    let table = if interpretation == yamaa_core::specification::SourceInterpretation::Odm {
        project_odm(table)?
    } else {
        table
    };
    crate::source_ordinal::append(source, empty_string_convention(source, table)?)
}

pub(crate) fn decode_source(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: yamaa_core::specification::SourceInterpretation,
    contract: Option<&yamaa_core::producer_contract::Contract>,
) -> Result<ColumnTable, Error> {
    let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
    let table = match source.profile {
        yamaa_core::specification::SourceProfile::Parquet => decode_parquet(source, bytes, odm)?,
        yamaa_core::specification::SourceProfile::Xport
        | yamaa_core::specification::SourceProfile::Sas7bdat => {
            let table = crate::sas_source::parse(bytes, source).map_err(Error::SasSource)?;
            if odm {
                project_odm(table)?
            } else {
                table
            }
        }
        yamaa_core::specification::SourceProfile::OdmXml => {
            // The fixed clinical-item table, whose fields the profile names
            // and types; an `odm` read binds its eleven fields (NAME-8).
            let table = crate::odm_xml::read_input(bytes).map_err(Error::OdmSource)?;
            let table = if odm { project_odm(table)? } else { table };
            empty_string_convention(source, table)?
        }
        yamaa_core::specification::SourceProfile::Csv => {
            // Admit declared types before scanning, retaining raw producer header
            // validation before any typed cell conversion.
            let types = yamaa_core::typed_csv::PreparedTypes::new(&source.types)
                .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
            // The input's declaration decides whether the source begins with
            // a byte-order mark (CSV-10).
            let parsed = csv_source::parse_declared(bytes, source.byte_order_mark)
                .map_err(|error| Error::Source(TextTableError::Csv(error)))?;
            if odm {
                let binding = odm_header(&parsed.names)?;
                odm_records(parsed, &binding)?
            } else {
                producer_header(source, contract, &parsed.names)?;
                crate::typed_csv::convert(parsed, types).map_err(typed_source_error)?
            }
        }
    };
    if let Some(contract) = contract {
        stored_contract(source, contract, &table)?;
    }
    crate::source_ordinal::append(source, table)
}

/// The ODM fields a CSV header stores (NAME-8), all read as text.
fn odm_header(names: &[String]) -> Result<yamaa_core::odm_source::FieldBinding, Error> {
    let fields = names
        .iter()
        .map(|name| yamaa_core::odm_source::StoredField {
            name,
            kind: Some(yamaa_core::value::ColumnType::Str),
        })
        .collect::<Vec<_>>();
    yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)
}

/// CSV records as the fixed ODM table: the eleven fields `binding` locates,
/// in their canonical order, as text.
fn odm_records(
    parsed: csv_source::CsvSource,
    binding: &yamaa_core::odm_source::FieldBinding,
) -> Result<ColumnTable, Error> {
    let projected = csv_source::CsvSource {
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
        yamaa_core::typed_csv::PreparedTypes::new(&[]).expect("fixed text declarations"),
    )
    .map_err(typed_source_error)
}

/// A linked CSV artifact's raw header against its producer's output contract
/// (INPUT-11), before any typed cell conversion.
fn producer_header(
    source: &SourceDeclaration,
    contract: Option<&yamaa_core::producer_contract::Contract>,
    names: &[String],
) -> Result<(), Error> {
    let Some(contract) = contract else {
        return Ok(());
    };
    let header = names.iter().map(String::as_str).collect::<Vec<_>>();
    match contract
        .validate_header(&source.name, &header)
        .map_err(Error::ProducerContract)?
    {
        Some(diagnostic) => Err(Error::ProducerMetadata(diagnostic)),
        None => Ok(()),
    }
}

/// Decode `source`'s held bytes by partition (#3792): a CSV source in
/// partitions of `rows` records, the last holding the rest, and a Parquet
/// source in runs of whole row groups of at most `rows` records, a larger
/// row group being a partition of its own. Any other profile is read whole,
/// as one partition.
///
/// Every finding [`decode_source`] reports is reported in the same order, at
/// the same record, row and field, and with the same context: the CSV scan
/// reads every record before any converts, and each later check runs over
/// every partition before the next one starts. Only the row and cell limits
/// differ, with the text-offset bound of each column: each partition, not
/// the whole table, is admitted within them. A partition's generated ordinal
/// is its records' positions in the whole source.
pub(crate) fn decode_partitions(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: yamaa_core::specification::SourceInterpretation,
    contract: Option<&yamaa_core::producer_contract::Contract>,
    rows: std::num::NonZeroUsize,
) -> Result<Vec<ColumnTable>, Error> {
    use yamaa_core::specification::SourceProfile;
    use yamaa_core::table::TableAccess;
    let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
    let tables = match source.profile {
        SourceProfile::Parquet => {
            let tables =
                crate::parquet_source::parse_partitions(bytes, odm, rows).map_err(parquet_error)?;
            tables
                .into_iter()
                .map(|table| empty_string_convention(source, table))
                .collect::<Result<Vec<_>, _>>()?
        }
        SourceProfile::Csv => {
            let types = yamaa_core::typed_csv::PreparedTypes::new(&source.types)
                .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
            let partitions = csv_source::scan(bytes, source.byte_order_mark, rows)
                .map_err(|error| Error::Source(TextTableError::Csv(error)))?;
            let records = |index| {
                partitions
                    .records(index)
                    .map_err(|error| Error::Source(TextTableError::Csv(error)))
            };
            if odm {
                let binding = odm_header(partitions.names())?;
                (0..partitions.len())
                    .map(|index| odm_records(records(index)?, &binding))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                producer_header(source, contract, partitions.names())?;
                (0..partitions.len())
                    .map(|index| {
                        crate::typed_csv::convert(records(index)?, types)
                            .map_err(typed_source_error)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
        }
        // Every other profile is read whole: one partition.
        SourceProfile::Xport | SourceProfile::Sas7bdat | SourceProfile::OdmXml => {
            return decode_source(source, bytes, interpretation, contract).map(|table| vec![table]);
        }
    };
    if let (Some(contract), Some(first)) = (contract, tables.first()) {
        stored_contract(source, contract, first)?;
    }
    let mut start = 0usize;
    tables
        .into_iter()
        .map(|table| {
            let records = table.row_count();
            let table = crate::source_ordinal::append_at(source, table, start)?;
            start += records;
            Ok(table)
        })
        .collect()
}

/// The schema `source`'s held bytes decode to, read from a CSV source's
/// header or a Parquet source's footer alone (#3792): the fields
/// [`decode_source`] gives the table when it finds nothing, the generated
/// ordinal included. `None` for any other profile, or a header or footer
/// that does not read, whose source is read whole.
pub(crate) fn source_schema(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: yamaa_core::specification::SourceInterpretation,
) -> Option<yamaa_core::table::TableSchema> {
    use yamaa_core::specification::SourceProfile;
    use yamaa_core::table::TableAccess;
    match source.profile {
        // The header alone decodes as the whole source does, to no records.
        SourceProfile::Csv => {
            let header = &bytes[..csv_source::header_end(bytes)];
            let table = decode_source(source, header, interpretation, None).ok()?;
            Some(table.schema().clone())
        }
        SourceProfile::Parquet => {
            let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
            let schema = crate::parquet_source::schema(bytes, odm)?;
            match yamaa_core::source_ordinal::prepare(source, &schema, 0) {
                Ok(Some(ordinal)) => Some(ordinal.schema().clone()),
                Ok(None) => Some(schema),
                Err(_) => None,
            }
        }
        SourceProfile::Xport | SourceProfile::Sas7bdat | SourceProfile::OdmXml => None,
    }
}

pub(crate) fn typed_source_error(error: crate::typed_csv::Error) -> Error {
    match error {
        crate::typed_csv::Error::Csv(error) => Error::Source(TextTableError::Csv(error)),
        crate::typed_csv::Error::Table(error) => Error::Source(TextTableError::Table(error)),
        error => Error::TypedSource(error),
    }
}

fn project_odm(table: ColumnTable) -> Result<ColumnTable, Error> {
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
    let binding = yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)?;
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
    table
        .project(schema, binding.stored())
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}

fn decode_parquet(
    source: &SourceDeclaration,
    bytes: &[u8],
    odm: bool,
) -> Result<ColumnTable, Error> {
    use crate::parquet_source;
    let table = if odm {
        parquet_source::parse_odm(bytes)
    } else {
        parquet_source::parse(bytes)
    }
    .map_err(parquet_error)?;
    // All strings were admitted by the bounded codec.
    empty_string_convention(source, table)
}

fn parquet_error(error: crate::parquet_source::Error) -> Error {
    match error {
        crate::parquet_source::Error::OdmFields(fields) => Error::OdmFields(fields),
        error => Error::ParquetSource(error),
    }
}

/// Apply the dataset's empty-string convention after the profile decodes, so
/// equal logical input yields equal values whichever transport carried it.
/// Only a text column holding a convention-missing value is rebuilt, with
/// that value missing; every other column stays shared, and an unaffected
/// table is returned unchanged.
fn empty_string_convention(
    source: &SourceDeclaration,
    table: ColumnTable,
) -> Result<ColumnTable, Error> {
    use crate::column_table::Column;
    let replaced = table.replaced(|column| match column {
        Column::Str(text) => text
            .missing_where(|value| source.text_is_missing(value))
            .map(Column::Str),
        _ => None,
    });
    match replaced {
        None => Ok(table),
        Some(table) => table.map_err(|error| Error::Source(TextTableError::Table(error))),
    }
}

/// Host frames decode as their input declares: the empty-string convention
/// and the `odm` projection.
#[cfg(all(test, feature = "arrow"))]
mod frame_tests {
    use super::*;
    use arrow_array::{ArrayRef, LargeStringArray, RecordBatch, StringArray, StringViewArray};
    use arrow_schema::{Field, Schema};
    use std::sync::Arc;
    use yamaa_core::{
        specification::{SourceInterpretation, SourceProfile},
        table::{TableAccess, ValueRef},
        value::ColumnType,
    };

    fn source(empty_string_present: bool) -> SourceDeclaration {
        SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            byte_order_mark: false,
            empty_string_present,
            ordinal: None,
        }
    }

    /// A host frame of these named columns, through a C stream.
    fn frame(columns: Vec<(&str, ArrayRef)>) -> ColumnTable {
        let schema = Arc::new(Schema::new(
            columns
                .iter()
                .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
                .collect::<Vec<_>>(),
        ));
        let arrays = columns.into_iter().map(|(_, array)| array).collect();
        let batch = RecordBatch::try_new(schema.clone(), arrays).unwrap();
        crate::arrow_bridge::import_public_stream(crate::arrow_bridge::export_stream(
            schema,
            vec![batch],
        ))
        .unwrap()
    }

    #[test]
    fn frames_apply_the_declared_convention_to_every_text_layout() {
        let values = vec![Some(""), Some("x"), None];
        for array in [
            Arc::new(StringArray::from(values.clone())) as ArrayRef,
            Arc::new(LargeStringArray::from(values.clone())),
            Arc::new(StringViewArray::from(values.clone())),
        ] {
            let kind = array.data_type().clone();
            let missing = decode_frame(
                &source(false),
                frame(vec![("S", array.clone())]),
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
            assert_eq!(missing.cell(0, 0).unwrap(), ValueRef::Missing, "{kind}");
            assert_eq!(missing.cell(1, 0).unwrap(), ValueRef::Str("x"), "{kind}");
            assert_eq!(missing.cell(2, 0).unwrap(), ValueRef::Missing, "{kind}");
            let present = decode_frame(
                &source(true),
                frame(vec![("S", array)]),
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
            assert_eq!(present.cell(0, 0).unwrap(), ValueRef::Str(""), "{kind}");
            assert_eq!(present.cell(2, 0).unwrap(), ValueRef::Missing, "{kind}");
        }
    }

    /// An `odm` read binds the eleven fields by name, ignoring ASCII case
    /// (NAME-8), and takes them in their canonical order and spelling,
    /// whatever text layout the frame holds them in.
    #[test]
    fn an_odm_frame_binds_the_eleven_canonical_fields_whatever_their_case_and_layout() {
        use yamaa_core::odm_source::FIELDS;
        // Lower-cased and in reverse order, as large strings, each holding
        // its own name with a suffix.
        let names = FIELDS
            .iter()
            .rev()
            .map(|name| name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let columns = names
            .iter()
            .map(|name| {
                let values = LargeStringArray::from(vec![Some(format!("{name}-1"))]);
                (name.as_str(), Arc::new(values) as ArrayRef)
            })
            .collect();
        let table = decode_frame(
            &source(false),
            frame(columns),
            SourceInterpretation::Odm,
            None,
        )
        .unwrap();
        let fields = table
            .schema()
            .columns()
            .iter()
            .map(|column| (column.name.as_str(), column.kind))
            .collect::<Vec<_>>();
        assert_eq!(fields, FIELDS.map(|name| (name, ColumnType::Str)));
        for (column, name) in FIELDS.iter().enumerate() {
            let expected = format!("{}-1", name.to_ascii_lowercase());
            assert_eq!(
                table.cell(0, column).unwrap(),
                ValueRef::Str(&expected),
                "{name}"
            );
        }
    }
}

#[cfg(test)]
mod width_tests {
    use super::*;
    use yamaa_core::{
        specification::SourceProfile,
        table::{TableAccess, ValueRef},
        value::ColumnType,
    };

    #[test]
    #[cfg(feature = "parquet")]
    fn parquet_decode_admits_small_groups_beyond_prior_group_and_metadata_frontiers() {
        use arrow_array::{Int64Array, RecordBatch};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;

        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            byte_order_mark: false,
            empty_string_present: false,
            ordinal: None,
        };
        let schema = Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, true)]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from_iter_values(0..5000))],
        )
        .unwrap();
        let properties = crate::storage_api::writer_properties_max_rows(1);
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
        // One row per write still yields one row group per row; a single
        // 5,000-row write makes the writer recurse once per group it splits
        // out and overflows the test thread.
        for row in 0..batch.num_rows() {
            writer.write(&batch.slice(row, 1)).unwrap();
        }
        writer.close().unwrap();
        let table = decode_parquet(&source, &bytes, false).unwrap();
        assert_eq!(table.row_count(), 5000);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(0));
        assert_eq!(table.cell(4999, 0).unwrap(), ValueRef::Int(4999));
    }

    #[test]
    #[cfg(feature = "parquet")]
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
            byte_order_mark: false,
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
            let result = decode_source(
                &source,
                &bytes,
                yamaa_core::specification::SourceInterpretation::Ordinary,
                None,
            );
            if width == 1_024 {
                let table = result.unwrap();
                assert_eq!(table.schema().columns().len(), 1_024);
                assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            } else {
                assert!(matches!(
                    result,
                    Err(Error::ParquetSource(
                        crate::parquet_source::Error::ResourceLimit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025)
                        }
                    ))
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
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            // One complete header plus one row must fit: the old 64-header
            // allowance refused this in-range table before width admission.
            let table = decode_source(
                &source,
                bytes.as_bytes(),
                yamaa_core::specification::SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
            assert_eq!(table.schema().columns().len(), 1_024);
            assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            let excess = format!("{},EXCESS\n", names.join(","));
            assert!(matches!(
                decode_source(
                    &source,
                    excess.as_bytes(),
                    yamaa_core::specification::SourceInterpretation::Ordinary,
                    None
                ),
                Err(Error::Source(TextTableError::Csv(
                    crate::csv_source::Error::Limit {
                        resource: "columns",
                        limit: 1_024,
                        actual: Some(1_025)
                    }
                )))
            ));
        }
    }
}
