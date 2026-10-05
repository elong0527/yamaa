//! Bounded reference metadata transport, shared by batch and prepared host adapters.
mod intermediate;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{fmt, panic::catch_unwind};
use yamaa_core::reference_scope;
use yamaa_core::{reference_binding as core, value::ColumnType};

/// Admit UTF-8 bytes before JSON allocations; clients cannot raise these policies.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;
const MAX_QUERIES: usize = 4096;

/// Advertise only implemented metadata queries, before activation or source access.
pub fn capabilities() -> &'static str {
    r#"{"protocol":"reference-analysis/1","features":["binding","output_validation","qualified_validation","intermediate_validation"]}"#
}

/// Invalid transport and metadata stay distinct from language diagnostics and limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    RequestLimit,
    InvalidEnvelope,
    UnsupportedProtocol,
    InvalidCatalog,
    InvalidQuery,
    Internal,
}
impl fmt::Display for TransportError {
    /// Return stable text without echoing caller-controlled names or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "reference request exceeds byte limit",
            Self::InvalidEnvelope => "invalid reference envelope",
            Self::UnsupportedProtocol => "unsupported reference protocol",
            Self::InvalidCatalog => "invalid reference catalog metadata",
            Self::InvalidQuery => "invalid reference query metadata",
            Self::Internal => "internal reference analysis failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Str,
    Int,
    Float,
    Date,
    Datetime,
}
impl Kind {
    /// Map the closed column vocabulary without host scalar coercion.
    fn core(self) -> ColumnType {
        match self {
            Self::Str => ColumnType::Str,
            Self::Int => ColumnType::Int,
            Self::Float => ColumnType::Float,
            Self::Date => ColumnType::Date,
            Self::Datetime => ColumnType::DateTime,
        }
    }
}
impl From<ColumnType> for Kind {
    /// Preserve exact declared types in binding and diagnostic results.
    fn from(value: ColumnType) -> Self {
        match value {
            ColumnType::Str => Self::Str,
            ColumnType::Int => Self::Int,
            ColumnType::Float => Self::Float,
            ColumnType::Date => Self::Date,
            ColumnType::DateTime => Self::Datetime,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    name: String,
    #[serde(rename = "type")]
    kind: Kind,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dataset {
    name: String,
    fields: Vec<Field>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCatalog {
    outputs: Vec<Field>,
    datasets: Vec<Dataset>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogRequest {
    protocol: String,
    catalog: WireCatalog,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisRequest {
    protocol: String,
    catalog: WireCatalog,
    queries: Vec<Query>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryRequest {
    protocol: String,
    queries: Vec<Query>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Query {
    ValidateIntermediate {
        field: String,
        target: intermediate::Intermediate,
    },
    ValidateIntermediateRead {
        read: intermediate::Read,
    },
    Bind {
        name: String,
    },
    ValidateOutput {
        name: String,
        expected: Option<Kind>,
        available: Option<Vec<usize>>,
        candidates: Vec<usize>,
    },
    ValidateQualified {
        name: String,
        expected: Option<Kind>,
        scope: WireScope,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reach {
    Scalar,
    Record,
    Relation,
    Declared,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireScope {
    drivers: Vec<String>,
    current_driver: bool,
    reach: Reach,
    joined: bool,
    phase: Phase,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Phase {
    Row { group_by: Option<Vec<String>> },
    Column { groups: Vec<Vec<String>> },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum QualifiedFinding {
    DriverMismatch,
    UnknownField,
    RowPhase,
    RowGroup,
    ColumnGroup,
    IncompatibleInputType { expected: Kind, actual: Kind },
}

impl From<reference_scope::Finding> for QualifiedFinding {
    /// Transport core-selected scope findings without interpreting their priority.
    fn from(value: reference_scope::Finding) -> Self {
        use reference_scope::Finding as F;
        match value {
            F::DriverMismatch => Self::DriverMismatch,
            F::UnknownField => Self::UnknownField,
            F::RowPhase => Self::RowPhase,
            F::RowGroup => Self::RowGroup,
            F::ColumnGroup => Self::ColumnGroup,
            F::IncompatibleInputType { expected, actual } => Self::IncompatibleInputType {
                expected: expected.into(),
                actual: actual.into(),
            },
        }
    }
}

/// Borrow decoded context for one pure validation without retaining wire buffers.
fn qualified(
    catalog: &core::Catalog,
    name: &str,
    expected: Option<Kind>,
    scope: &WireScope,
) -> Result<Vec<QualifiedFinding>, core::Error> {
    let drivers: Vec<_> = scope.drivers.iter().map(String::as_str).collect();
    let groups: Vec<Vec<&str>> = match &scope.phase {
        Phase::Row { group_by } => group_by
            .iter()
            .map(|group| group.iter().map(String::as_str).collect())
            .collect(),
        Phase::Column { groups } => groups
            .iter()
            .map(|group| group.iter().map(String::as_str).collect())
            .collect(),
    };
    let slices: Vec<_> = groups.iter().map(Vec::as_slice).collect();
    let phase = match scope.phase {
        Phase::Row { .. } => reference_scope::Phase::Row {
            group_by: slices.first().copied(),
        },
        Phase::Column { .. } => reference_scope::Phase::Column { groups: &slices },
    };
    let scope = reference_scope::Scope {
        drivers: &drivers,
        current_driver: scope.current_driver,
        joined: scope.joined,
        phase,
        reach: match scope.reach {
            Reach::Scalar => reference_scope::Reach::Scalar,
            Reach::Record => reference_scope::Reach::Record,
            Reach::Relation => reference_scope::Reach::Relation,
            Reach::Declared => reference_scope::Reach::Declared,
        },
    };
    reference_scope::validate(
        catalog,
        name,
        expected.map(Kind::core),
        scope,
        reference_scope::Limits::default(),
    )
    .map(|findings| findings.into_iter().map(Into::into).collect())
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Bound {
    Output {
        column: usize,
        #[serde(rename = "type")]
        kind: Kind,
    },
    Dataset {
        dataset: usize,
        field: usize,
        #[serde(rename = "type")]
        kind: Kind,
    },
}
#[derive(Serialize)]
#[serde(tag = "condition", rename_all = "snake_case")]
enum Diagnostic {
    UnknownField,
    UnresolvableName { dataset: usize },
    PhaseBoundary { column: usize },
    IncompatibleInputType { expected: Kind, actual: Kind },
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum QueryResult {
    IntermediateValidation {
        diagnostics: Vec<intermediate::Finding>,
    },
    Binding {
        binding: Option<Bound>,
    },
    Validation {
        diagnostic: Option<Diagnostic>,
    },
    QualifiedValidation {
        diagnostics: Vec<QualifiedFinding>,
    },
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Complete {
        results: Vec<QueryResult>,
    },
    Limit {
        resource: &'static str,
        limit: String,
        required: String,
    },
}

/// An immutable core catalog, containing neither borrowed JSON nor host objects.
#[derive(Debug)]
pub struct CompiledCatalog(core::Catalog);

/// Decode only within the trusted byte budget and the strict declared JSON shape.
fn decode<T: DeserializeOwned>(request: &str) -> Result<T, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    serde_json::from_str(request).map_err(|_| TransportError::InvalidEnvelope)
}

/// Convert bounded borrowed field metadata without duplicating semantic name rules.
fn field(value: &Field) -> core::Field<'_> {
    core::Field {
        name: &value.name,
        column_type: value.kind.core(),
    }
}

/// Prepare one owned catalog, retaining core admission order and resource accounting.
fn compile(wire: &WireCatalog) -> Result<CompiledCatalog, core::Error> {
    let outputs: Vec<_> = wire.outputs.iter().map(field).collect();
    let fields: Vec<Vec<_>> = wire
        .datasets
        .iter()
        .map(|dataset| dataset.fields.iter().map(field).collect())
        .collect();
    let datasets: Vec<_> = wire
        .datasets
        .iter()
        .zip(&fields)
        .map(|(dataset, fields)| core::Dataset {
            name: &dataset.name,
            fields,
        })
        .collect();
    core::Catalog::compile(&outputs, &datasets, core::Limits::default()).map(CompiledCatalog)
}

/// Keep resource outcomes separate from malformed compiler metadata.
fn failure(error: core::Error, invalid: TransportError) -> Result<Outcome, TransportError> {
    match error {
        core::Error::Limit {
            resource,
            limit,
            required,
        } => Ok(Outcome::Limit {
            resource,
            limit: limit.to_string(),
            required: required.to_string(),
        }),
        core::Error::SizeOverflow => Err(TransportError::Internal),
        _ => Err(invalid),
    }
}

/// Serialize owned outcomes without exposing names not already held by the host binder.
fn response(protocol: &'static str, outcome: Outcome) -> Result<String, TransportError> {
    #[derive(Serialize)]
    struct Response {
        protocol: &'static str,
        outcome: Outcome,
    }
    serde_json::to_string(&Response { protocol, outcome }).map_err(|_| TransportError::Internal)
}

impl CompiledCatalog {
    /// Reuse the same immutable catalog for bounded query batches, without host callbacks.
    pub fn analyze(&self, request: &str) -> Result<String, TransportError> {
        catch_unwind(|| {
            let request: QueryRequest = decode(request)?;
            if request.protocol != "reference-queries/1" {
                return Err(TransportError::UnsupportedProtocol);
            }
            response("reference-analysis/1", self.queries(&request.queries)?)
        })
        .map_err(|_| TransportError::Internal)?
    }

    /// Preserve query order; malformed context rejects the whole batch before publication.
    fn queries(&self, queries: &[Query]) -> Result<Outcome, TransportError> {
        if queries.len() > MAX_QUERIES {
            return Ok(Outcome::Limit {
                resource: "queries",
                limit: MAX_QUERIES.to_string(),
                required: queries.len().to_string(),
            });
        }
        let mut results = Vec::with_capacity(queries.len());
        for query in queries {
            let result = match query {
                Query::ValidateIntermediate { field, target } => {
                    intermediate::field(&self.0, field, target)
                        .map(|diagnostics| QueryResult::IntermediateValidation { diagnostics })
                }
                Query::ValidateIntermediateRead { read } => intermediate::read(&self.0, read)
                    .map(|diagnostics| QueryResult::IntermediateValidation { diagnostics }),
                Query::ValidateQualified {
                    name,
                    expected,
                    scope,
                } => qualified(&self.0, name, *expected, scope)
                    .map(|diagnostics| QueryResult::QualifiedValidation { diagnostics }),
                Query::Bind { name } => self.0.bind(name).map(|binding| QueryResult::Binding {
                    binding: binding.map(|bound| match bound {
                        core::Binding::Output {
                            column,
                            column_type,
                        } => Bound::Output {
                            column,
                            kind: column_type.into(),
                        },
                        core::Binding::Dataset {
                            dataset,
                            field,
                            column_type,
                        } => Bound::Dataset {
                            dataset,
                            field,
                            kind: column_type.into(),
                        },
                    }),
                }),
                Query::ValidateOutput {
                    name,
                    expected,
                    available,
                    candidates,
                } => self
                    .0
                    .validate_output(
                        name,
                        expected.map(Kind::core),
                        available.as_deref(),
                        candidates,
                    )
                    .map(|diagnostic| QueryResult::Validation {
                        diagnostic: diagnostic.map(|diagnostic| match diagnostic {
                            core::Diagnostic::UnknownField => Diagnostic::UnknownField,
                            core::Diagnostic::UnresolvableName { dataset } => {
                                Diagnostic::UnresolvableName { dataset }
                            }
                            core::Diagnostic::PhaseBoundary { column } => {
                                Diagnostic::PhaseBoundary { column }
                            }
                            core::Diagnostic::IncompatibleInputType { expected, actual } => {
                                Diagnostic::IncompatibleInputType {
                                    expected: expected.into(),
                                    actual: actual.into(),
                                }
                            }
                        }),
                    }),
            };
            match result {
                Ok(result) => results.push(result),
                Err(error) => return failure(error, TransportError::InvalidQuery),
            }
        }
        Ok(Outcome::Complete { results })
    }
}

/// Compile once for hosts that retain an opaque immutable catalog between queries.
/// A limit returns no catalog plus a structured outcome, never a partially usable handle.
pub fn compile_reference_catalog(
    request: &str,
) -> Result<(Option<CompiledCatalog>, String), TransportError> {
    catch_unwind(|| {
        let request: CatalogRequest = decode(request)?;
        if request.protocol != "reference-catalog/1" {
            return Err(TransportError::UnsupportedProtocol);
        }
        let (catalog, outcome) = match compile(&request.catalog) {
            Ok(catalog) => (Some(catalog), Outcome::Complete { results: vec![] }),
            Err(error) => (None, failure(error, TransportError::InvalidCatalog)?),
        };
        Ok((catalog, response("reference-catalog/1", outcome)?))
    })
    .map_err(|_| TransportError::Internal)?
}

/// Analyze a complete batch in one call, sharing the same compiler with prepared hosts.
pub fn analyze_references(request: &str) -> Result<String, TransportError> {
    catch_unwind(|| {
        let request: AnalysisRequest = decode(request)?;
        if request.protocol != "reference-analysis/1" {
            return Err(TransportError::UnsupportedProtocol);
        }
        let outcome = match compile(&request.catalog) {
            Ok(catalog) => catalog.queries(&request.queries)?,
            Err(error) => failure(error, TransportError::InvalidCatalog)?,
        };
        response("reference-analysis/1", outcome)
    })
    .map_err(|_| TransportError::Internal)?
}
