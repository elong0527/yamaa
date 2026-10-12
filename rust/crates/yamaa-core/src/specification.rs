//! Bounded original-specification compiler prototype: declared typed sources,
//! implicit key grain, source columns and checked numeric computations.
//! Admission never reads study data; binding consumes only the captured source schema.
use crate::{
    bound_expression::{Binding, Read},
    dataset::{Assignment, BoundNumeric, DatasetPlan, Expression, RowMode, RowTemplate},
};
use crate::{
    column_dependencies,
    numeric_compiler::{compile_numeric_with_policy, CompileError, CompiledNumeric},
    numeric_parser::ParseError,
    reference_binding::{self, Catalog},
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::{Column, TableSchema},
    value::ColumnType,
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};

#[path = "specification_binding_diagnostics.rs"]
mod binding_diagnostics;
#[path = "specification_intermediates.rs"]
mod intermediates;
#[path = "specification_lookup_diagnostics.rs"]
mod lookup_diagnostics;
#[path = "specification_match_values.rs"]
mod match_values;
pub use lookup_diagnostics::LookupFinding;
use lookup_diagnostics::ReferenceCause;
#[path = "specification_output_diagnostics.rs"]
mod output_diagnostics;
#[path = "specification_preflight_diagnostics.rs"]
mod preflight_diagnostics;
#[path = "specification_root_filter.rs"]
mod root_filter;
#[path = "specification_sources.rs"]
mod source_expressions;
pub use source_expressions::SourceFinding;
#[path = "specification_column_checks.rs"]
mod column_checks;
#[path = "specification_logs.rs"]
mod logs;
#[path = "specification_row_drivers.rs"]
mod row_drivers;
#[path = "specification_row_intermediates.rs"]
mod row_intermediates;
#[path = "specification_rows.rs"]
mod rows;
#[path = "specification_verifications.rs"]
mod verifications;
pub use logs::{LogFinding, OutputLogs};
#[path = "specification_windows.rs"]
mod windows;
pub use windows::WindowFinding;
#[path = "specification_functions.rs"]
mod functions;
pub use functions::{Cause as FunctionCause, FunctionFinding};
#[path = "specification_cases.rs"]
mod cases;
#[path = "specification_derivations.rs"]
mod derivations;
#[path = "specification_flag.rs"]
mod flags;
#[path = "specification_temporal_imputation.rs"]
mod imputation;
#[path = "specification_rounding.rs"]
mod rounding;
#[path = "specification_scalar.rs"]
mod scalar;
#[path = "specification_str_case.rs"]
mod str_case;
#[path = "specification_str_extract.rs"]
mod str_extract;
#[path = "specification_str_pad.rs"]
mod str_pad;
#[path = "specification_str_template.rs"]
mod str_template;
#[path = "specification_temporal.rs"]
mod temporal;
#[path = "specification_terminology.rs"]
mod terminology;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedFeature {
    pub operation: String,
    pub path: String,
}
#[derive(Debug)]
pub enum PrepareError {
    Limit(&'static str),
    Unsupported(Vec<UnsupportedFeature>),
    Numeric { path: String, error: CompileError },
    Invalid(Vec<PreflightFinding>),
    OutputSchema(crate::table::SchemaError),
    Internal,
}
/// Trusted compiler policy, not language limits. Aggregate admission precedes
/// owned names, findings, compiled expressions and reference catalog construction.
#[derive(Clone, Copy, Debug)]
pub struct CompilationLimits {
    pub columns: usize,
    pub keys: usize,
    pub inputs: usize,
    pub projected_columns: usize,
    pub source_fields: usize,
    pub model_text_bytes: usize,
    pub numeric_bytes: usize,
    pub source_operands: usize,
}
impl Default for CompilationLimits {
    fn default() -> Self {
        Self {
            columns: 64,
            keys: 64,
            inputs: 16,
            projected_columns: 64,
            source_fields: 64,
            model_text_bytes: 262_144,
            numeric_bytes: 65_536,
            source_operands: 64,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum PreflightFinding {
    Scalar(crate::diagnostic::Diagnostic),
    TextExtract(crate::diagnostic::Diagnostic),
    TextTemplate(crate::diagnostic::Diagnostic),
    TextCase(crate::diagnostic::Diagnostic),
    Log(LogFinding),
    Imputation(crate::diagnostic::Diagnostic),
    RoundingInput {
        path: String,
        source: String,
        actual: &'static str,
    },
    TemporalInput {
        path: String,
        source: String,
        actual: &'static str,
    },
    TemporalBounds {
        path: String,
        value: String,
    },
    ProjectFunction(FunctionFinding),
    ProjectCodelist(alloc::boxed::Box<crate::project_codelist_binding::Finding>),
    RowPhase {
        path: String,
        identifier: String,
        row: String,
    },
    UndeclaredRowColumn {
        index: usize,
        column: String,
    },
    DuplicateRowDefault {
        column: String,
        rows: Vec<String>,
    },
    MissingRowDerivation {
        column: String,
        rows: Vec<String>,
    },
    ConflictingRowConstruction,
    InvalidGroup {
        index: usize,
        row: String,
        groups: Vec<String>,
    },
    GroupReference {
        index: usize,
        row: String,
        name: String,
        dataset: String,
    },
    RowDriverUnavailable {
        index: usize,
        row: String,
        dataset: Option<String>,
    },
    MissingDerivation {
        column: String,
    },
    UndeclaredKey {
        position: usize,
        column: String,
    },
    DriverUnavailable {
        dataset: Option<String>,
    },
    DomainInputCollision {
        domain: String,
    },
    RedundantSourceType {
        dataset: String,
        field: String,
        kind: ColumnType,
    },
    UnknownSourceProfile {
        dataset: String,
        path: String,
    },
    EmptyStringPresentUnsupported {
        dataset: String,
    },
    OdmSourceOrdinal {
        dataset: String,
    },
}
#[derive(Debug)]
pub enum BindError {
    Internal,
    PredicateDiagnosticLimit,
    SourceCount,
    PredicatePolicy(crate::predicate_compiler::Error),
    InvalidPredicateBinding(crate::bound_expression::BindingError),
    Invalid(Vec<BindFinding>),
    Catalog(reference_binding::Error),
    DependencyPolicy(column_dependencies::Error),
    InvalidNumericBinding(crate::bound_expression::BindingError),
    InvalidPlan(crate::dataset::PlanError),
}
#[derive(Debug)]
pub enum BindFinding {
    Padding(crate::diagnostic::Diagnostic),
    Scalar(crate::diagnostic::Diagnostic),
    TextExtract(crate::diagnostic::Diagnostic),
    TextTemplate(crate::diagnostic::Diagnostic),
    Temporal(crate::diagnostic::Diagnostic),
    Predicate(crate::diagnostic::Diagnostic),
    ProjectFunction(FunctionFinding),
    Source(SourceFinding),
    Window(WindowFinding),
    Lookup(LookupFinding),
    RowFilterPhase {
        path: String,
        identifier: String,
        row: String,
        grouped: bool,
        qualified: bool,
    },
    QualifiedReference {
        path: String,
        name: String,
        row: Option<String>,
        finding: crate::reference_scope::Finding,
    },
    Aggregate {
        path: String,
        expression: String,
        error: crate::aggregate_parser::GrammarFailure,
    },
    AggregateScope {
        path: String,
        expression: String,
        relation: Option<String>,
    },
    QualifiedNumericReference {
        path: String,
        expression: String,
        identifier: String,
    },
    Numeric {
        path: String,
        expression: String,
        error: CompileError,
    },
    UnknownReference {
        path: String,
        name: String,
    },
    OutputReference {
        path: String,
        name: String,
        finding: reference_binding::Diagnostic,
    },
    Dependencies {
        columns: Vec<String>,
        paths: Vec<String>,
        diagnostics: Vec<column_dependencies::Diagnostic>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDeclaration {
    pub name: String,
    pub path: String,
    pub types: Vec<(String, ColumnType)>,
    pub profile: SourceProfile,
    pub empty_string_present: bool,
    pub ordinal: Option<String>,
}
/// The closed source profile is selected by the authored path, never its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProfile {
    Csv,
    Parquet,
}
impl SourceProfile {
    pub fn from_path(path: &str) -> Option<Self> {
        let name = path.rsplit('/').next()?;
        let (stem, extension) = name.rsplit_once('.')?;
        if stem.is_empty() {
            return None;
        }
        if extension.eq_ignore_ascii_case("csv") {
            Some(Self::Csv)
        } else if extension.eq_ignore_ascii_case("parquet") {
            Some(Self::Parquet)
        } else {
            None
        }
    }
}
impl SourceDeclaration {
    /// Apply the input convention after the storage profile preserves its value.
    pub fn text_is_missing(&self, value: &str) -> bool {
        !self.empty_string_present && value.is_empty()
    }
}
#[derive(Clone, Debug)]
enum Operation {
    Scalar(alloc::boxed::Box<scalar::Declaration>),
    StrContains(crate::text_extract::StringExtract),
    Flag(flags::Declaration),
    StrExtract(crate::text_extract::StringExtract),
    StrTemplate(crate::text_template::StringTemplate),
    Case(Vec<cases::Branch>),
    ProjectFunction(usize),
    Literal(crate::value::Value),
    Window(alloc::boxed::Box<windows::Declaration>),
    Source(alloc::boxed::Box<source_expressions::Declaration>),
    FirstAvailable(alloc::boxed::Box<source_expressions::FirstAvailable>),
    Compute(CompiledNumeric),
    Round(crate::dataset::Rounding),
    Temporal(crate::dataset::Temporal),
    StrCase(crate::text_case::StringCase),
    StrPad(str_pad::Declaration),
    Imputation(crate::dataset::Imputation),
    /// Reference planning emits formula diagnostics after source ingestion.
    InvalidNumeric {
        expression: String,
        error: CompileError,
    },
}
#[derive(Clone, Debug)]
struct Declaration {
    handler: Option<crate::conversion::LiteralHandler>,
    path: String,
    operation: Operation,
}
/// Private admitted representation: no caller-provided typed plan or binding indices.
#[derive(Debug)]
pub struct PreparedSpecification {
    project_calls: Option<crate::project_calls::ProjectCalls>,
    sources: Vec<SourceDeclaration>,
    driver: usize,
    intermediates: intermediates::Declarations,
    output: TableSchema,
    projection: Vec<String>,
    output_path: String,
    output_logs: OutputLogs,
    output_decimals: Option<String>,
    output_order: Vec<(String, bool, bool)>,
    root_filter: Option<root_filter::RootFilter>,
    keys: Vec<usize>,
    declarations: Vec<Declaration>,
    rows: Option<rows::Rows>,
    verifications: verifications::Verifications,
    column_verifications: Vec<verifications::Verifications>,
}

/// A static finding borrowed from one admitted compilation. Allocate its
/// portable diagnostic only after the caller has admitted ownership capacity.
#[derive(Clone, Copy, Debug)]
pub enum VerificationDeclarationFinding<'a> {
    Diagnostic(&'a crate::diagnostic::Diagnostic),
    Declaration {
        path: &'a str,
        condition: &'static str,
        requirement: &'static str,
        reason: &'a str,
    },
}
impl VerificationDeclarationFinding<'_> {
    pub fn visit_paths(self, mut visit: impl FnMut(&str)) {
        match self {
            Self::Diagnostic(diagnostic) => {
                diagnostic.spec_paths.iter().for_each(|path| visit(path))
            }
            Self::Declaration { path, .. } => visit(path),
        }
    }
    pub fn diagnostic(self) -> crate::diagnostic::Diagnostic {
        match self {
            Self::Diagnostic(diagnostic) => diagnostic.clone(),
            Self::Declaration {
                path,
                condition,
                requirement,
                reason,
            } => crate::dataset_checks::declaration_diagnostic(
                path.into(),
                condition,
                requirement,
                reason.into(),
            )
            .expect("compiled verification finding has a registered cause"),
        }
    }
}

/// Preserve omitted versus explicit-null recovery at every declaration.
fn literal_handler(
    d: &Document,
    id: usize,
    prefix: &str,
) -> Result<Option<crate::conversion::LiteralHandler>, PrepareError> {
    d.field(id, "unconvertible")
        .map(|id| {
            let spec_path = format!("{prefix}.unconvertible");
            Ok(crate::conversion::LiteralHandler {
                value: literal(d, id, &spec_path)?,
                spec_path,
            })
        })
        .transpose()
}

/// Read an admitted scalar leaf once; rows and columns share the same boundary.
fn literal(d: &Document, id: usize, path: &str) -> Result<crate::value::Value, PrepareError> {
    use crate::value::Value;
    Ok(match &d.nodes()[id] {
        N::Null => Value::Missing,
        N::Text(value) => Value::Str(value.clone()),
        N::Boolean(value) => Value::Bool(*value),
        N::Float(value) => Value::float(*value),
        N::Integer(value) => Value::Int(value.parse().map_err(|_| {
            PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "wide_integer_literal".into(),
                path: path.into(),
            }])
        })?),
        _ => {
            return Err(PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "literal".into(),
                path: path.into(),
            }]))
        }
    })
}

fn text(d: &Document, id: usize) -> Result<&str, PrepareError> {
    match &d.nodes()[id] {
        N::Text(v) => Ok(v),
        _ => Err(PrepareError::Internal),
    }
}
fn field(d: &Document, id: usize, name: &str) -> Result<usize, PrepareError> {
    d.field(id, name).ok_or(PrepareError::Internal)
}
fn sequence(d: &Document, id: usize) -> Result<&[usize], PrepareError> {
    match &d.nodes()[id] {
        N::Sequence(v) => Ok(v),
        _ => Err(PrepareError::Internal),
    }
}
fn mapping(d: &Document, id: usize) -> Result<&[(usize, usize)], PrepareError> {
    match &d.nodes()[id] {
        N::Mapping(v) => Ok(v),
        _ => Err(PrepareError::Internal),
    }
}
fn present(d: &Document, id: usize, name: &str) -> bool {
    d.field(id, name)
        .is_some_and(|id| !matches!(d.nodes()[id], N::Null))
}
fn reject(extra: &mut Vec<UnsupportedFeature>, operation: &str, path: String) {
    extra.push(UnsupportedFeature {
        operation: operation.into(),
        path,
    });
}
fn optional_features(
    d: &Document,
    id: usize,
    fields: &[&str],
    prefix: &str,
    extra: &mut Vec<UnsupportedFeature>,
) {
    for &name in fields {
        if present(d, id, name) {
            reject(
                extra,
                name,
                if prefix.is_empty() {
                    name.into()
                } else {
                    format!("{prefix}.{name}")
                },
            )
        }
    }
}

// Free-form descriptive metadata does not affect execution. Governed metadata
// keys retain an explicit unsupported boundary until submission validation is
// connected; accepting these as descriptions would bypass REQ-0910.
fn metadata_features(
    d: &Document,
    id: usize,
    prefix: &str,
    column: bool,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<(), PrepareError> {
    let Some(metadata) = d
        .field(id, "metadata")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(());
    };
    let reserved: &[&str] = if column {
        &[
            "core",
            "mandatory",
            "role",
            "data_type",
            "length",
            "significant_digits",
            "display_format",
            "codelist",
            "inventory_vocabulary",
            "origin",
            "method",
            "comment",
        ]
    } else {
        &[
            "label",
            "class",
            "subclass",
            "structure",
            "repeating",
            "reference_data",
            "has_no_data",
            "domain",
            "comment",
        ]
    };
    for &(key, _) in mapping(d, metadata)? {
        let key = text(d, key)?;
        if reserved.contains(&key) {
            reject(
                extra,
                "reserved_metadata_key",
                if prefix.is_empty() {
                    format!("metadata.{key}")
                } else {
                    format!("{prefix}.metadata.{key}")
                },
            );
        }
    }
    Ok(())
}

/// Preserve the reference preflight order and collect all independent findings.
fn preflight(spec: &SpecificationDocument) -> Result<Vec<PreflightFinding>, PrepareError> {
    let d = spec.document();
    let root = d.root();
    let mut findings = Vec::new();
    let columns = sequence(d, field(d, root, "columns")?)?;
    let names = columns
        .iter()
        .map(|&id| text(d, field(d, id, "name")?))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = d
        .field(root, "rows")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| sequence(d, id))
        .transpose()?
        .unwrap_or(&[]);
    for (index, &row) in rows.iter().enumerate() {
        for &(name, _) in mapping(d, field(d, row, "derivations")?)? {
            let column = text(d, name)?;
            if !names.contains(&column) {
                findings.push(PreflightFinding::UndeclaredRowColumn {
                    index,
                    column: column.into(),
                });
            }
        }
    }
    if !rows.is_empty() {
        for (&id, &name) in columns.iter().zip(&names) {
            if let Some(default) = d
                .field(id, "derivation")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let value = field(d, default, "value")?;
                // Aggregate column results belong to the completed dataset and
                // cannot be promoted into row-local defaults (REQ-1260).
                if d.field(value, "aggregate").is_some() {
                    let mut overridden = Vec::new();
                    for &row in rows {
                        if d.field(field(d, row, "derivations")?, name).is_some() {
                            overridden.push(text(d, field(d, row, "id")?)?.into());
                        }
                    }
                    if !overridden.is_empty() {
                        findings.push(PreflightFinding::DuplicateRowDefault {
                            column: name.into(),
                            rows: overridden,
                        });
                    }
                }
            } else {
                let mut missing = Vec::new();
                for &row in rows {
                    if d.field(field(d, row, "derivations")?, name).is_none() {
                        missing.push(text(d, field(d, row, "id")?)?.into());
                    }
                }
                if !missing.is_empty() {
                    findings.push(PreflightFinding::MissingRowDerivation {
                        column: name.into(),
                        rows: missing,
                    });
                }
            }
        }
    } else {
        for (&id, &name) in columns.iter().zip(&names) {
            if !present(d, id, "derivation") {
                findings.push(PreflightFinding::MissingDerivation {
                    column: name.into(),
                });
            }
        }
    }
    let inputs = mapping(d, field(d, root, "input")?)?;
    let input_names = inputs
        .iter()
        .map(|&(id, _)| text(d, id))
        .collect::<Result<Vec<_>, _>>()?;
    let domain = text(d, field(d, root, "domain")?)?;
    if input_names.contains(&domain) {
        findings.push(PreflightFinding::DomainInputCollision {
            domain: domain.into(),
        });
    }
    for (position, &id) in sequence(d, field(d, root, "keys")?)?.iter().enumerate() {
        let name = text(d, id)?;
        if !names.contains(&name) {
            findings.push(PreflightFinding::UndeclaredKey {
                position,
                column: name.into(),
            });
        }
    }
    if rows.is_empty() && spec.default_driver().is_none() {
        findings.push(PreflightFinding::DriverUnavailable { dataset: None });
    }
    if let Some(id) = d
        .field(root, "base")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        let base = text(d, id)?;
        if !input_names.contains(&base) {
            findings.push(PreflightFinding::DriverUnavailable {
                dataset: Some(base.into()),
            });
        }
    }
    if !rows.is_empty() && present(d, root, "filter") {
        findings.push(PreflightFinding::ConflictingRowConstruction);
    }
    let intermediate_names = d
        .field(root, "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
        .map(|id| {
            sequence(d, id)?
                .iter()
                .map(|&id| text(d, field(d, id, "id")?))
                .collect::<Result<Vec<_>, PrepareError>>()
        })
        .transpose()?
        .unwrap_or_default();
    for (index, &row) in rows.iter().enumerate() {
        let identity = text(d, field(d, row, "id")?)?;
        let driver = d
            .field(row, "dataset")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| text(d, id))
            .transpose()?
            .or_else(|| (input_names.len() == 1).then(|| input_names[0]));
        if !driver
            .is_some_and(|name| input_names.contains(&name) || intermediate_names.contains(&name))
        {
            findings.push(PreflightFinding::RowDriverUnavailable {
                index,
                row: identity.into(),
                dataset: driver.map(String::from),
            });
        }
        if let Some(group) = d
            .field(row, "group_by")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let groups = sequence(d, group)?
                .iter()
                .map(|&id| text(d, id).map(String::from))
                .collect::<Result<Vec<_>, _>>()?;
            if groups.is_empty()
                || groups
                    .iter()
                    .enumerate()
                    .any(|(i, name)| groups[..i].contains(name))
            {
                findings.push(PreflightFinding::InvalidGroup {
                    index,
                    row: identity.into(),
                    groups,
                });
            } else if let Some(driver) = driver {
                for name in groups {
                    if !name.starts_with(&format!("{driver}.")) || name.matches('.').count() != 1 {
                        findings.push(PreflightFinding::GroupReference {
                            index,
                            row: identity.into(),
                            name,
                            dataset: driver.into(),
                        });
                    }
                }
            }
        }
    }
    Ok(findings)
}

impl PreparedSpecification {
    /// Reject unsupported declarations before requesting any source bytes. Effective
    /// source empty-string behavior is missing unless explicitly declared otherwise.
    pub fn prepare(spec: &SpecificationDocument) -> Result<Self, PrepareError> {
        Self::prepare_with_limits(spec, CompilationLimits::default())
    }
    pub fn prepare_with_limits(
        spec: &SpecificationDocument,
        limits: CompilationLimits,
    ) -> Result<Self, PrepareError> {
        Self::prepare_implementation(spec, limits, None, None, Default::default(), None)
    }
    /// Prepare calls against statically admitted environment definitions. No
    /// project imports, lock verification, test invocation or study reads occur.
    pub fn prepare_with_project(
        spec: &SpecificationDocument,
        functions: &[crate::project_function::Function],
    ) -> Result<Self, PrepareError> {
        Self::prepare_implementation(
            spec,
            CompilationLimits::default(),
            Some(functions),
            None,
            Default::default(),
            None,
        )
    }
    /// Compile project calls and fixed terminology against the same admitted metadata.
    pub fn prepare_with_environment(
        spec: &SpecificationDocument,
        environment: &crate::project_environment::ExecutionEnvironment,
    ) -> Result<Self, PrepareError> {
        Self::prepare_with_environment_limits(
            spec,
            environment,
            CompilationLimits::default(),
            Default::default(),
        )
    }
    pub fn prepare_with_environment_limits(
        spec: &SpecificationDocument,
        environment: &crate::project_environment::ExecutionEnvironment,
        limits: CompilationLimits,
        terminology_limits: crate::project_limits::Limits,
    ) -> Result<Self, PrepareError> {
        Self::prepare_implementation(
            spec,
            limits,
            Some(environment.functions()),
            Some(environment.catalogue()),
            terminology_limits,
            None,
        )
    }
    /// This representation stays inside the sealed producer metadata result.
    /// No public constructor exposes a producer-backed executable plan.
    pub(crate) fn prepare_producer_metadata(
        spec: &SpecificationDocument,
        limits: CompilationLimits,
        environment: Option<&crate::project_environment::ExecutionEnvironment>,
        producers: &[crate::producer_admission::Declaration],
    ) -> Result<Self, PrepareError> {
        Self::prepare_implementation(
            spec,
            limits,
            environment.map(|e| e.functions()),
            environment.map(|e| e.catalogue()),
            Default::default(),
            Some(producers),
        )
    }
    fn prepare_implementation(
        spec: &SpecificationDocument,
        limits: CompilationLimits,
        functions: Option<&[crate::project_function::Function]>,
        catalogue: Option<&crate::project_terminology::Catalogue>,
        terminology_limits: crate::project_limits::Limits,
        producers: Option<&[crate::producer_admission::Declaration]>,
    ) -> Result<Self, PrepareError> {
        let d = spec.document();
        let root = d.root();
        for (field_name, limit) in [("columns", limits.columns), ("keys", limits.keys)] {
            if sequence(d, field(d, root, field_name)?)?.len() > limit {
                return Err(PrepareError::Limit(field_name));
            }
        }
        if mapping(d, field(d, root, "input")?)?.len() > limits.inputs {
            return Err(PrepareError::Limit("inputs"));
        }
        let output = field(d, root, "output")?;
        if sequence(d, field(d, output, "columns")?)?.len() > limits.projected_columns {
            return Err(PrepareError::Limit("projected_columns"));
        }
        let mut model_text_bytes = 0usize;
        for node in d.nodes() {
            if let N::Text(value) | N::Integer(value) = node {
                model_text_bytes = model_text_bytes
                    .checked_add(value.len())
                    .filter(|&n| n <= limits.model_text_bytes)
                    .ok_or(PrepareError::Limit("model_text_bytes"))?;
            }
        }
        // Per-expression parser limits remain in force. Charge the entire sum
        // before compiling any formula, including malformed/unsupported formulas.
        let mut numeric_bytes = 0usize;
        if let Some(filter) = d
            .field(root, "filter")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            numeric_bytes = text(d, filter)?.len();
            if numeric_bytes > limits.numeric_bytes {
                return Err(PrepareError::Limit("numeric_bytes"));
            }
        }
        for &column in sequence(d, field(d, root, "columns")?)? {
            let expr = d
                .field(column, "derivation")
                .and_then(|id| d.field(id, "value"))
                .and_then(|id| d.field(id, "compute"))
                .and_then(|id| d.field(id, "expr"));
            let value = d
                .field(column, "derivation")
                .and_then(|id| d.field(id, "value"));
            if let Some(value) = value {
                cases::charge(d, value, &mut numeric_bytes, limits.numeric_bytes, 0)?;
            }
            let operands = value
                .and_then(|id| d.field(id, "first_available"))
                .and_then(|id| d.field(id, "sources"))
                .map(|id| sequence(d, id))
                .transpose()?
                .unwrap_or(&[]);
            if operands.len() > limits.source_operands {
                return Err(PrepareError::Limit("source_operands"));
            }
            let filter = d
                .field(column, "derivation")
                .and_then(|id| d.field(id, "value"))
                .and_then(|id| d.field(id, "source"))
                .and_then(|id| d.field(id, "filter"))
                .filter(|&id| !matches!(d.nodes()[id], N::Null));
            let selection_filters = operands
                .iter()
                .filter_map(|&id| d.field(id, "filter"))
                .filter(|&id| !matches!(d.nodes()[id], N::Null));
            for id in expr.into_iter().chain(filter).chain(selection_filters) {
                numeric_bytes = numeric_bytes
                    .checked_add(text(d, id)?.len())
                    .filter(|&n| n <= limits.numeric_bytes)
                    .ok_or(PrepareError::Limit("numeric_bytes"))?;
            }
        }
        // Row expressions are charged across the entire document before any
        // aggregate parsing, owned row declarations, or literal conversion.
        if let Some(rows) = d
            .field(root, "rows")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let rows = sequence(d, rows)?;
            if rows.len() > 16 {
                return Err(PrepareError::Limit("row_templates"));
            }
            for &row in rows {
                if let Some(filter) = d
                    .field(row, "filter")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    numeric_bytes = numeric_bytes
                        .checked_add(text(d, filter)?.len())
                        .filter(|&n| n <= limits.numeric_bytes)
                        .ok_or(PrepareError::Limit("numeric_bytes"))?;
                }
                if let Some(group) = d
                    .field(row, "group_by")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    if sequence(d, group)?.len() > limits.keys {
                        return Err(PrepareError::Limit("group_keys"));
                    }
                }
                let declarations = mapping(d, field(d, row, "derivations")?)?;
                if declarations.len() > limits.columns {
                    return Err(PrepareError::Limit("row_columns"));
                }
                for &(_, declaration) in declarations {
                    let value = field(d, declaration, "value")?;
                    cases::charge(d, value, &mut numeric_bytes, limits.numeric_bytes, 0)?;
                    for operation in ["compute", "aggregate"] {
                        if let Some(expr) =
                            d.field(value, operation).and_then(|id| d.field(id, "expr"))
                        {
                            numeric_bytes = numeric_bytes
                                .checked_add(text(d, expr)?.len())
                                .filter(|&n| n <= limits.numeric_bytes)
                                .ok_or(PrepareError::Limit("numeric_bytes"))?;
                        }
                    }
                }
            }
        }
        if let Some(verifications) = d
            .field(root, "verifications")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            for &verification in sequence(d, verifications)? {
                if let Some(assertion) = d.field(verification, "assert") {
                    for name in ["when", "require"] {
                        if let Some(N::Text(expression)) =
                            d.field(assertion, name).map(|id| &d.nodes()[id])
                        {
                            numeric_bytes = numeric_bytes
                                .checked_add(expression.len())
                                .filter(|&n| n <= limits.numeric_bytes)
                                .ok_or(PrepareError::Limit("numeric_bytes"))?;
                        }
                    }
                }
            }
        }
        match_values::declaration_admission(d, limits)?;
        let mut findings = preflight(spec)?;
        let columns = sequence(d, field(d, root, "columns")?)?;
        let output = TableSchema::new(
            columns
                .iter()
                .map(|&id| {
                    Ok(Column {
                        name: text(d, field(d, id, "name")?)?.into(),
                        kind: match text(d, field(d, id, "type")?)? {
                            "str" => ColumnType::Str,
                            "int" => ColumnType::Int,
                            "float" => ColumnType::Float,
                            "date" => ColumnType::Date,
                            "datetime" => ColumnType::DateTime,
                            _ => return Err(PrepareError::Internal),
                        },
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(PrepareError::OutputSchema)?;
        let project = match functions
            .map(|functions| functions::prepare(d, columns, functions))
            .transpose()
        {
            Ok(project) => project,
            Err(PrepareError::Invalid(errors)) => {
                findings.extend(errors);
                None
            }
            Err(error) => return Err(error),
        };
        let mut column_verifications = columns
            .iter()
            .enumerate()
            .map(|(column, &id)| {
                verifications::Verifications::prepare_column(d, &output, id, column)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(catalogue) = catalogue {
            match terminology::apply(
                d,
                columns,
                &output,
                catalogue,
                terminology_limits,
                &mut column_verifications,
            ) {
                Ok(()) => (),
                Err(PrepareError::Invalid(errors)) => findings.extend(errors),
                Err(error) => return Err(error),
            }
        }
        rounding::declaration_findings(d, columns, &output, &mut findings)?;
        temporal::declaration_findings(d, columns, &output, &mut findings)?;
        imputation::declaration_findings(d, columns, &output, &mut findings)?;
        str_pad::declaration_admission(d, columns, &output)?;
        flags::declaration_admission(d, columns, &output)?;
        str_case::declaration_findings(d, columns, &output, &mut findings)?;
        scalar::declaration_findings(d, &output, &mut findings)?;
        let extraction = str_extract::Prepared::new(d, &output, &mut findings)?;
        let templates = str_template::Prepared::new(d, &output, &mut findings)?;
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let mut extra = Vec::new();
        let has_rows = d
            .field(root, "rows")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| sequence(d, id))
            .transpose()?
            .is_some_and(|rows| !rows.is_empty());
        optional_features(d, root, &["parents", "submission"], "", &mut extra);
        let root_filter = root_filter::RootFilter::prepare(d, &mut extra)?;
        metadata_features(d, root, "", false, &mut extra)?;
        for &column in sequence(d, field(d, root, "columns")?)? {
            let name = text(d, field(d, column, "name")?)?;
            metadata_features(d, column, &format!("columns.{name}"), true, &mut extra)?;
            terminology::submission_features(
                d,
                column,
                &format!("columns.{name}"),
                catalogue.is_some(),
                &mut extra,
            )?;
        }
        let inputs = mapping(d, field(d, root, "input")?)?;
        let mut source_findings = Vec::new();
        let mut derivation_values = columns
            .iter()
            .filter_map(|&column| {
                d.field(column, "derivation")
                    .and_then(|derivation| d.field(derivation, "value"))
            })
            .collect::<Vec<_>>();
        if let Some(rows) = d
            .field(root, "rows")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            for &row in sequence(d, rows)? {
                for &(_, derivation) in mapping(d, field(d, row, "derivations")?)? {
                    derivation_values.extend(d.field(derivation, "value"));
                }
            }
        }
        let mut odm_datasets = alloc::collections::BTreeSet::new();
        let mut visited_values = alloc::collections::BTreeSet::new();
        while let Some(value) = derivation_values.pop() {
            if !visited_values.insert(value) {
                continue;
            }
            if let Some(payload) = d.field(value, "odm") {
                let item = match &d.nodes()[payload] {
                    N::Text(item) => Some(item.as_str()),
                    _ => d.field(payload, "item").and_then(|id| text(d, id).ok()),
                };
                if let Some((dataset, _)) = item.and_then(|item| item.split_once('.')) {
                    odm_datasets.insert(dataset);
                }
            }
            if let Some(case) = d.field(value, "case") {
                for &branch in sequence(d, case)? {
                    for name in ["then", "otherwise"] {
                        derivation_values.extend(d.field(branch, name));
                    }
                }
            }
            if let Some(concat) = d.field(value, "str_concat") {
                derivation_values
                    .extend(sequence(d, field(d, concat, "sources")?)?.iter().copied());
            }
        }
        let driver = if has_rows {
            row_intermediates::common_driver(d, inputs)?
        } else {
            match spec.default_driver() {
                Some(driver) => driver,
                None if !extra.is_empty() => return Err(PrepareError::Unsupported(extra)),
                None => return Err(PrepareError::Internal),
            }
        };
        let driver_index = inputs
            .iter()
            .position(|&(name, _)| text(d, name).is_ok_and(|name| name == driver))
            .ok_or(PrepareError::Internal)?;
        for &(name, id) in inputs {
            let prefix = format!("input.{}", text(d, name)?);
            if producers.is_none() {
                optional_features(d, id, &["schema"], &prefix, &mut extra);
            }
            let profile = SourceProfile::from_path(text(d, field(d, id, "path")?)?);
            if profile == Some(SourceProfile::Csv)
                && d.field(id, "empty_string")
                    .is_some_and(|id| !matches!(&d.nodes()[id],N::Text(v) if v=="missing"))
            {
                source_findings.push(PreflightFinding::EmptyStringPresentUnsupported {
                    dataset: text(d, name)?.into(),
                });
            }
            if profile.is_none() {
                source_findings.push(PreflightFinding::UnknownSourceProfile {
                    dataset: text(d, name)?.into(),
                    path: text(d, field(d, id, "path")?)?.into(),
                });
            }
            if present(d, id, "ordinal") && odm_datasets.contains(text(d, name)?) {
                source_findings.push(PreflightFinding::OdmSourceOrdinal {
                    dataset: text(d, name)?.into(),
                });
            }
            if profile == Some(SourceProfile::Parquet) {
                if let Some(types) = d
                    .field(id, "types")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                {
                    let fields = mapping(d, types)?;
                    if fields.len() > limits.source_fields {
                        return Err(PrepareError::Limit("source_fields"));
                    }
                    for &(field_name, kind) in fields {
                        let kind = match text(d, kind)? {
                            "str" => ColumnType::Str,
                            "int" => ColumnType::Int,
                            "float" => ColumnType::Float,
                            "date" => ColumnType::Date,
                            "datetime" => ColumnType::DateTime,
                            _ => return Err(PrepareError::Internal),
                        };
                        source_findings.push(PreflightFinding::RedundantSourceType {
                            dataset: text(d, name)?.into(),
                            field: text(d, field_name)?.into(),
                            kind,
                        });
                    }
                }
            }
        }
        if !source_findings.is_empty() {
            return Err(PrepareError::Invalid(source_findings));
        }
        // Unsupported row/intermediate/metadata semantics must never reach the
        // closed no-row lowering representation, even when their shapes are valid.
        if extra
            .iter()
            .any(|feature| matches!(feature.operation.as_str(), "parents" | "submission"))
        {
            return Err(PrepareError::Unsupported(extra));
        }
        let mut sources = Vec::new();
        for &(name, selected) in inputs {
            let mut source_types = Vec::new();
            if let Some(id) = d
                .field(selected, "types")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let fields = mapping(d, id)?;
                if fields.len() > limits.source_fields {
                    return Err(PrepareError::Limit("source_fields"));
                }
                for &(name, kind) in fields {
                    let kind = match text(d, kind)? {
                        "str" => ColumnType::Str,
                        "int" => ColumnType::Int,
                        "float" => ColumnType::Float,
                        "date" => ColumnType::Date,
                        "datetime" => ColumnType::DateTime,
                        _ => return Err(PrepareError::Internal),
                    };
                    source_types.push((text(d, name)?.into(), kind));
                }
            }
            sources.push(SourceDeclaration {
                name: text(d, name)?.into(),
                types: source_types,
                path: text(d, field(d, selected, "path")?)?.into(),
                // Unsupported formats never produce an accepted plan.
                profile: SourceProfile::from_path(text(d, field(d, selected, "path")?)?)
                    .unwrap_or(SourceProfile::Csv),
                empty_string_present: d.field(selected, "empty_string").is_some_and(
                    |id| matches!(&d.nodes()[id], N::Text(value) if value == "present"),
                ),
                ordinal: d
                    .field(selected, "ordinal")
                    .filter(|&id| !matches!(d.nodes()[id], N::Null))
                    .map(|id| text(d, id).map(String::from))
                    .transpose()?,
            });
            if let Some(producer) = producers.and_then(|p| {
                p.iter()
                    .find(|p| p.dataset() == text(d, name).unwrap_or(""))
            }) {
                // A producer's selected output contract is the sole type authority.
                // The private metadata wrapper never exposes these as file reads.
                sources.last_mut().ok_or(PrepareError::Internal)?.types = producer
                    .contract()
                    .fields()
                    .iter()
                    .map(|f| (f.name.clone(), f.kind))
                    .collect();
            }
        }
        let output_id = field(d, root, "output")?;
        let output_order = d
            .field(output_id, "order_by")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| {
                sequence(d, id)?
                    .iter()
                    .map(|&term| {
                        if let N::Text(name) = &d.nodes()[term] {
                            return Ok((name.clone(), false, false));
                        }
                        let name = text(d, field(d, term, "variable")?)?.into();
                        let descending = d.field(term, "direction").is_some_and(
                            |id| matches!(&d.nodes()[id], N::Text(value) if value == "desc"),
                        );
                        let nulls_first = d.field(term, "nulls").is_some_and(
                            |id| matches!(&d.nodes()[id], N::Text(value) if value == "first"),
                        );
                        Ok((name, descending, nulls_first))
                    })
                    .collect::<Result<Vec<_>, PrepareError>>()
            })
            .transpose()?
            .unwrap_or_default();
        let output_path = String::from(text(d, field(d, output_id, "path")?)?);
        // Keep arbitrary-width schema integers. Output declaration validation
        // retains its existing post-verification phase; width is a codec quota.
        let output_decimals = d
            .field(output_id, "decimals")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .map(|id| match &d.nodes()[id] {
                N::Integer(value) => Ok(value.clone()),
                _ => Err(PrepareError::Internal),
            })
            .transpose()?;
        // Output declaration errors retain their post-verification phase.
        let projection = sequence(d, field(d, output_id, "columns")?)?
            .iter()
            .map(|&id| text(d, id).map(String::from))
            .collect::<Result<Vec<_>, _>>()?;
        let columns = sequence(d, field(d, root, "columns")?)?;
        let names = output
            .columns()
            .iter()
            .enumerate()
            .map(|(id, c)| (c.name.as_str(), id))
            .collect::<BTreeMap<_, _>>();
        let keys = sequence(d, field(d, root, "keys")?)?
            .iter()
            .map(|&id| {
                let name = text(d, id)?;
                names.get(name).copied().ok_or(PrepareError::Internal)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let verifications = verifications::Verifications::prepare(d, &output)?;
        let warnings = column_verifications
            .iter()
            .flat_map(|group| &group.declared_warning_paths)
            .chain(&verifications.declared_warning_paths)
            .cloned()
            .collect::<Vec<_>>();
        let (output_logs, log_findings) =
            OutputLogs::prepare(d, output_id, &output_path, warnings)?;
        if !log_findings.is_empty() {
            return Err(PrepareError::Invalid(
                log_findings
                    .into_iter()
                    .map(PreflightFinding::Log)
                    .collect(),
            ));
        }
        let intermediates =
            intermediates::Declarations::prepare(d, &sources, driver_index, limits, &mut extra)?;
        if has_rows {
            if !extra.is_empty() {
                return Err(PrepareError::Unsupported(extra));
            }
            let rows = rows::Rows::prepare(
                d,
                &output,
                driver,
                limits,
                project.as_ref(),
                catalogue.is_some(),
                &extraction,
                &templates,
                &intermediates,
            )?;
            let project = match project {
                Some(project) => {
                    let uses = rows.call_uses(&project.calls)?;
                    Some(functions::select_rows(
                        project,
                        functions.ok_or(PrepareError::Internal)?,
                        &uses,
                    )?)
                }
                None => None,
            };
            return Ok(Self {
                project_calls: project.map(|p| p.calls),
                sources,
                driver: driver_index,
                intermediates,
                output,
                projection,
                output_path,
                output_logs,
                output_decimals,
                output_order,
                root_filter,
                keys,
                declarations: Vec::new(),
                rows: Some(rows),
                verifications,
                column_verifications,
            });
        }
        let mut declarations = Vec::new();
        for (column, &id) in columns.iter().enumerate() {
            let prefix = format!("columns.{}", output.columns()[column].name);

            let derivation = d
                .field(id, "derivation")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .ok_or(PrepareError::Internal)?;
            let handler = literal_handler(d, derivation, &format!("{prefix}.derivation"))?;
            let ops = mapping(d, field(d, derivation, "value")?)?;
            let &[(op, payload)] = ops else {
                return Err(PrepareError::Internal);
            };
            let op = text(d, op)?;
            let path = format!("{prefix}.derivation.{op}");
            let operation = match op {
                "source" => {
                    let source = source_expressions::Declaration::prepare(
                        d,
                        payload,
                        &path,
                        &mut extra,
                        &intermediates,
                    )?;
                    if source
                        .variable
                        .split_once('.')
                        .is_some_and(|(relation, _)| {
                            relation != driver && !intermediates.contains(relation)
                        })
                    {
                        reject(&mut extra, "secondary_source_expression", path.clone());
                    }
                    if keys.contains(&column)
                        && source.has_filter()
                        && source
                            .variable
                            .split_once('.')
                            .is_some_and(|(name, _)| name == driver)
                    {
                        reject(&mut extra, "filtered_key_source", path.clone());
                    }
                    Some(Operation::Source(alloc::boxed::Box::new(source)))
                }
                "first_available" => {
                    let selection = source_expressions::FirstAvailable::prepare(
                        d,
                        payload,
                        &path,
                        &mut extra,
                        &intermediates,
                    )?;
                    if keys.contains(&column) {
                        reject(&mut extra, "selection_key_source", path.clone());
                    }
                    for (index, source) in selection.sources.iter().enumerate() {
                        if let Some((relation, _)) = source.variable.split_once('.') {
                            if intermediates.contains(relation) {
                                reject(
                                    &mut extra,
                                    "selection_intermediate_source",
                                    format!("{path}.sources[{index}]"),
                                );
                            } else if relation != driver {
                                reject(
                                    &mut extra,
                                    "secondary_source_expression",
                                    format!("{path}.sources[{index}]"),
                                );
                            }
                        }
                    }
                    Some(Operation::FirstAvailable(alloc::boxed::Box::new(selection)))
                }
                "row_number"
                | "rank"
                | "row_value"
                | "previous_non_missing"
                | "locf"
                | "baseline_flag" => Some(Operation::Window(alloc::boxed::Box::new(
                    windows::Declaration::prepare(d, payload, op, &path, limits, &mut extra)?,
                ))),
                "function" if project.is_none() => {
                    reject(&mut extra, "function", path.clone());
                    None
                }
                _ => match derivations::prepare_value_with_driver(
                    d,
                    field(d, derivation, "value")?,
                    &format!("{prefix}.derivation"),
                    project.as_ref(),
                    &intermediates,
                    keys.contains(&column),
                    0,
                    &extraction,
                    &templates,
                    Some(driver),
                    limits,
                ) {
                    Ok(branch) => Some(*branch.operation),
                    Err(PrepareError::Unsupported(features)) => {
                        extra.extend(features);
                        None
                    }
                    Err(error) => return Err(error),
                },
            };
            if let Some(operation) = operation {
                scalar::unsupported_relations(&operation, driver, &intermediates, &mut extra);
                declarations.push(Declaration {
                    handler,
                    path,
                    operation,
                });
            }
        }
        if !extra.is_empty() {
            return Err(PrepareError::Unsupported(extra));
        }
        Ok(Self {
            project_calls: project.map(|p| p.calls),
            sources,
            driver: driver_index,
            intermediates,
            output,
            projection,
            output_path,
            output_logs,
            output_decimals,
            output_order,
            root_filter,
            keys,
            declarations,
            rows: None,
            verifications,
            column_verifications,
        })
    }
    pub fn source(&self) -> &SourceDeclaration {
        &self.sources[self.driver]
    }
    pub fn sources(&self) -> &[SourceDeclaration] {
        &self.sources
    }
    pub fn driver_index(&self) -> usize {
        self.driver
    }
    pub fn key_names(&self) -> impl Iterator<Item = &str> {
        self.keys
            .iter()
            .map(|&column| self.output.columns()[column].name.as_str())
    }
    pub fn output(&self) -> &TableSchema {
        &self.output
    }
    pub fn projection(&self) -> &[String] {
        &self.projection
    }
    pub fn verifications(&self) -> &[crate::dataset::Verification] {
        &self.verifications.checks
    }
    pub fn verification_severity(&self, path: &str) -> crate::dataset::VerificationSeverity {
        if self
            .verifications
            .warning_paths
            .iter()
            .chain(
                self.column_verifications
                    .iter()
                    .flat_map(|group| &group.warning_paths),
            )
            .any(|warning| warning == path)
        {
            crate::dataset::VerificationSeverity::Warning
        } else {
            crate::dataset::VerificationSeverity::Error
        }
    }
    fn warning_verification_paths(&self) -> Vec<String> {
        self.verifications
            .warning_paths
            .iter()
            .chain(
                self.column_verifications
                    .iter()
                    .flat_map(|group| &group.warning_paths),
            )
            .cloned()
            .collect()
    }
    /// Inspect known declaration findings without reading sources or evaluating checks.
    /// Column findings retain declaration order, followed by dataset findings.
    pub fn verification_declaration_diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.verification_declaration_findings()
            .map(VerificationDeclarationFinding::diagnostic)
            .collect()
    }
    /// Borrow findings before a boundary admits their count and complete text.
    /// This iterator cannot bind sources or expose the compiled dataset plan.
    pub fn verification_declaration_findings(
        &self,
    ) -> impl Iterator<Item = VerificationDeclarationFinding<'_>> {
        use crate::dataset::Check;
        self.column_verifications
            .iter()
            .chain([&self.verifications])
            .flat_map(|group| &group.checks)
            .filter_map(|verification| match &verification.check {
                Check::InvalidDiagnostic(diagnostic) => {
                    Some(VerificationDeclarationFinding::Diagnostic(diagnostic))
                }
                Check::InvalidDeclaration {
                    condition,
                    requirement,
                    reason,
                } => Some(VerificationDeclarationFinding::Declaration {
                    path: &verification.path,
                    condition,
                    requirement,
                    reason,
                }),
                _ => None,
            })
    }
    pub fn verification_identity(&self, path: &str) -> Option<&str> {
        core::iter::once(&self.verifications)
            .chain(&self.column_verifications)
            .find_map(|group| {
                group
                    .checks
                    .iter()
                    .position(|c| c.path == path)
                    .and_then(|index| group.identities[index].as_deref())
            })
    }
    pub fn verification_target(&self, path: &str) -> Option<&str> {
        self.column_verifications
            .iter()
            .position(|group| group.checks.iter().any(|check| check.path == path))
            .map(|column| self.output.columns()[column].name.as_str())
    }
    pub fn column_check(&self, path: &str) -> Option<&crate::dataset::Check> {
        self.column_verifications
            .iter()
            .flat_map(|group| &group.checks)
            .find(|check| check.path == path)
            .map(|verification| &verification.check)
    }
    fn column_verification_groups(&self) -> Vec<crate::dataset::ColumnVerifications> {
        self.column_verifications
            .iter()
            .enumerate()
            .filter(|(_, group)| !group.checks.is_empty())
            .map(|(column, group)| crate::dataset::ColumnVerifications {
                column,
                checks: group.checks.clone(),
            })
            .collect()
    }
    pub fn output_path(&self) -> &str {
        &self.output_path
    }
    pub fn output_logs(&self) -> &OutputLogs {
        &self.output_logs
    }
    pub fn output_profile(&self) -> Option<&'static str> {
        output_profile(&self.output_path)
    }
    /// Canonical arbitrary-width declaration, never a runtime signed-i64 value.
    pub fn output_decimals(&self) -> Option<&str> {
        self.output_decimals.as_deref()
    }
    pub fn output_name(&self) -> &str {
        let name = output_basename(&self.output_path);
        name.rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .map_or(name, |(stem, _)| stem)
    }
    /// Invoke only after derivation, output-key checks, and verification succeed.
    pub fn output_findings(&self) -> Vec<OutputFinding> {
        let mut findings = Vec::new();
        if output_profile(&self.output_path).is_none() {
            findings.push(OutputFinding::UnknownProfile {
                path: self.output_path.clone(),
            });
        }
        if let Some(value) = &self.output_decimals {
            if value.starts_with('-') {
                findings.push(OutputFinding::InvalidDecimals {
                    value: value.clone(),
                });
            } else if self.output_profile() == Some("parquet") {
                findings.push(OutputFinding::DecimalsNotApplicable {
                    path: self.output_path.clone(),
                });
            }
        }
        for (position, name) in self.projection.iter().enumerate() {
            if self.projection[..position].contains(name) {
                findings.push(OutputFinding::DuplicateColumn {
                    position,
                    name: name.clone(),
                });
            } else if !self.output.columns().iter().any(|c| c.name == *name) {
                findings.push(OutputFinding::UndeclaredColumn {
                    position,
                    name: name.clone(),
                });
            }
        }
        for (position, &column) in self.keys.iter().enumerate() {
            let name = &self.output.columns()[column].name;
            if !self.projection.contains(name) {
                findings.push(OutputFinding::InternalKey {
                    position,
                    name: name.clone(),
                });
            }
        }
        for (position, (name, _, _)) in self.output_order.iter().enumerate() {
            if self.output_order[..position]
                .iter()
                .any(|(previous, _, _)| previous == name)
            {
                findings.push(OutputFinding::DuplicateOrderTerm {
                    position,
                    name: name.clone(),
                });
            } else if !self
                .output
                .columns()
                .iter()
                .any(|column| &column.name == name)
            {
                findings.push(OutputFinding::UndeclaredOrderColumn {
                    position,
                    name: name.clone(),
                });
            }
        }
        findings
    }

    fn bound_output_order(&self) -> Vec<crate::dataset::OrderTerm> {
        // Output declaration findings are emitted after successful verification.
        // A rejected declaration must not enter presentation or its resource work.
        if !self.output_findings().is_empty() {
            return Vec::new();
        }
        self.output_order
            .iter()
            .map(
                |(name, descending, nulls_first)| crate::dataset::OrderTerm {
                    column: self
                        .output
                        .columns()
                        .iter()
                        .position(|column| &column.name == name)
                        .expect("validated output order names a declared column"),
                    descending: *descending,
                    nulls_first: *nulls_first,
                },
            )
            .collect()
    }

    /// Selected environment indices are in declaration order, matching activation slots.
    pub fn called_functions(&self) -> &[usize] {
        self.project_calls
            .as_ref()
            .map_or(&[], |calls| calls.selected())
    }
    pub fn project_calls(&self) -> Option<&crate::project_calls::ProjectCalls> {
        self.project_calls.as_ref()
    }
    /// Bind only immutable source metadata. No cell reads or expression evaluation.
    pub fn bind(&self, source: &TableSchema) -> Result<DatasetPlan, BindError> {
        self.bind_sources(&[source])
    }
    /// Schemas follow authored input order, independently of the selected driver.
    pub fn bind_sources(&self, schemas: &[&TableSchema]) -> Result<DatasetPlan, BindError> {
        if schemas.len() != self.sources.len() {
            return Err(BindError::SourceCount);
        }
        let source = schemas[self.driver];
        if let Some(rows) = &self.rows {
            rows.admit_sources(schemas, &self.output)?;
            let mut findings = Vec::new();
            let (secondary, intermediates) = self.intermediates.bind(
                &self.sources,
                self.driver,
                schemas,
                &self.output,
                &self.keys,
                &mut findings,
            )?;
            return rows
                .bind(
                    source,
                    &self.source().name,
                    &self.output,
                    &self.keys,
                    self.verifications(),
                    self.project_calls.as_ref(),
                    &self.intermediates,
                    secondary,
                    intermediates,
                    findings,
                )
                .and_then(|plan| {
                    plan.with_column_verifications(self.column_verification_groups())
                        .and_then(|plan| {
                            plan.with_warning_verifications(self.warning_verification_paths())
                        })
                        .and_then(|plan| plan.with_output_order(self.bound_output_order()))
                        .map_err(BindError::InvalidPlan)
                });
        }
        let output_fields = self
            .output
            .columns()
            .iter()
            .map(|c| reference_binding::Field {
                name: &c.name,
                column_type: c.kind,
            })
            .collect::<Vec<_>>();
        let source_fields = source
            .columns()
            .iter()
            .map(|c| reference_binding::Field {
                name: &c.name,
                column_type: c.kind,
            })
            .collect::<Vec<_>>();
        let catalog = Catalog::compile(
            &output_fields,
            &[reference_binding::Dataset {
                name: &self.source().name,
                fields: &source_fields,
            }],
            Default::default(),
        )
        .map_err(BindError::Catalog)?;
        let mut findings = Vec::new();
        let (secondary, intermediates) = self.intermediates.bind(
            &self.sources,
            self.driver,
            schemas,
            &self.output,
            &self.keys,
            &mut findings,
        )?;
        let mut assignments = BTreeMap::new();
        let mut dependencies = Vec::new();
        for (column, declaration) in self.declarations.iter().enumerate() {
            let declarations_reported =
                cases::padding_findings(&declaration.operation, &mut findings)?;
            let mut edges = Vec::new();
            let path = &declaration.path;
            let reference_path = if matches!(
                declaration.operation,
                Operation::Compute(_) | Operation::InvalidNumeric { .. }
            ) {
                format!("{path}.expr")
            } else {
                path.clone()
            };
            let start = findings.len();
            let bind = |name: &str,
                        findings: &mut Vec<BindFinding>|
             -> Result<Option<reference_binding::Binding>, BindError> {
                if !name.contains('.') {
                    if let Some(finding) = catalog
                        .validate_output(name, None, None, &[0])
                        .map_err(BindError::Catalog)?
                    {
                        findings.push(BindFinding::OutputReference {
                            path: reference_path.clone(),
                            name: name.into(),
                            finding,
                        });
                        return Ok(None);
                    }
                }
                let binding = catalog.bind(name).map_err(BindError::Catalog)?;
                if binding.is_none() {
                    findings.push(BindFinding::UnknownReference {
                        path: reference_path.clone(),
                        name: name.into(),
                    });
                }
                Ok(binding)
            };
            let expression = match &declaration.operation {
                Operation::Flag(operation) => flags::bind(
                    operation,
                    column,
                    &self.keys,
                    &self.source().name,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                    declarations_reported,
                )?,
                Operation::Case(branches) => cases::bind(
                    self,
                    branches,
                    column,
                    &catalog,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                    declarations_reported,
                )?,
                Operation::ProjectFunction(call) => {
                    let calls = self.project_calls.as_ref().ok_or(BindError::Internal)?;
                    let record = self.keys.contains(&column);
                    let context = crate::project_call_binding::Context {
                        source_dataset: 0,
                        source_mode: if record {
                            crate::project_call_binding::SourceMode::Record
                        } else {
                            crate::project_call_binding::SourceMode::Collect
                        },
                        available_outputs: None,
                        scope: crate::reference_scope::Scope {
                            drivers: &[self.source().name.as_str()],
                            current_driver: true,
                            reach: if record {
                                crate::reference_scope::Reach::Record
                            } else {
                                crate::reference_scope::Reach::Relation
                            },
                            joined: false,
                            phase: crate::reference_scope::Phase::Column { groups: &[] },
                        },
                    };
                    match crate::project_call_binding::bind(calls, *call, &catalog, context) {
                        Ok(bound) => {
                            edges.extend(bound.dependencies);
                            Some(Expression::ProjectFunction(bound.function))
                        }
                        Err(crate::project_call_binding::Error::Findings(errors)) => {
                            findings.extend(functions::bind_findings(calls, *call, errors, None)?);
                            None
                        }
                        Err(crate::project_call_binding::Error::Reference(error)) => {
                            return Err(BindError::Catalog(error))
                        }
                        Err(_) => return Err(BindError::Internal),
                    }
                }
                Operation::Literal(value) => Some(Expression::Literal(value.clone())),
                Operation::Round(rounding) => rounding::bind(
                    rounding,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::StrTemplate(operation) => str_template::bind(
                    operation,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::Scalar(operation) => scalar::bind(
                    self,
                    operation,
                    column,
                    &catalog,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                    declarations_reported,
                )?,
                Operation::StrContains(operation) => scalar::bind_contains(
                    self,
                    operation,
                    column,
                    &catalog,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::StrExtract(operation) => str_extract::bind(
                    operation,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::StrCase(operation) => str_case::bind(
                    operation,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::StrPad(operation) => str_pad::bind(
                    operation,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::Imputation(operation) => imputation::bind(
                    operation,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::Temporal(operation) => temporal::bind(
                    operation,
                    column,
                    &self.keys,
                    &catalog,
                    &self.intermediates,
                    &secondary,
                    &intermediates,
                    &mut edges,
                    &mut findings,
                )?,
                Operation::Window(window) => window
                    .bind(&catalog, &mut edges, &mut findings)?
                    .map(Expression::Window),
                Operation::InvalidNumeric { expression, error } => {
                    findings.push(BindFinding::Numeric {
                        path: reference_path.clone(),
                        expression: expression.clone(),
                        error: error.clone(),
                    });
                    None
                }
                Operation::Source(declaration) => {
                    let name = &declaration.variable;
                    let filter = declaration.bind_filter(
                        path,
                        self.source(),
                        source,
                        &self.intermediates,
                        &mut findings,
                    )?;
                    if let Some((index, field)) = self.intermediates.reference(name) {
                        if let Some(item) = &intermediates[index] {
                            edges.extend(item.output_dependencies());
                            let column = secondary[item.source]
                                .schema
                                .columns()
                                .iter()
                                .position(|c| c.name == field);
                            if column.is_none() {
                                findings.push(BindFinding::Lookup(LookupFinding::reference(
                                    path,
                                    name,
                                    None,
                                    ReferenceCause::Value,
                                    None,
                                )));
                            }
                            column.map(|column| Expression::Intermediate { index, column })
                        } else {
                            findings.push(BindFinding::UnknownReference {
                                path: path.clone(),
                                name: name.clone(),
                            });
                            None
                        }
                    } else {
                        bind(name, &mut findings)?.map(|binding| match binding {
                            reference_binding::Binding::Dataset { field, .. } => {
                                if self.keys.contains(&column) && !declaration.has_filter() {
                                    Expression::Source(field)
                                } else {
                                    Expression::Collect {
                                        column: field,
                                        identifier: name.clone(),
                                        filter,
                                        selection: None,
                                    }
                                }
                            }
                            reference_binding::Binding::Output { column, .. } => {
                                edges.push(column);
                                Expression::Column(column)
                            }
                        })
                    }
                }
                Operation::FirstAvailable(selection) => {
                    let mut operands = Vec::new();
                    for (index, operand) in selection.sources.iter().enumerate() {
                        let operand_path = format!("{path}.sources[{index}]");
                        let filter = operand.bind_filter(
                            &operand_path,
                            self.source(),
                            source,
                            &self.intermediates,
                            &mut findings,
                        )?;
                        let name = &operand.variable;
                        if !name.contains('.') {
                            if let Some(finding) = catalog
                                .validate_output(name, None, None, &[0])
                                .map_err(BindError::Catalog)?
                            {
                                findings.push(BindFinding::OutputReference {
                                    path: operand_path,
                                    name: name.clone(),
                                    finding,
                                });
                                continue;
                            }
                        }
                        let Some(binding) = catalog.bind(name).map_err(BindError::Catalog)? else {
                            findings.push(BindFinding::UnknownReference {
                                path: operand_path,
                                name: name.clone(),
                            });
                            continue;
                        };
                        let read = match binding {
                            reference_binding::Binding::Dataset { field, .. } => {
                                crate::dataset::SelectionRead::Collect {
                                    column: field,
                                    identifier: name.clone(),
                                    filter: filter.map(alloc::boxed::Box::new),
                                }
                            }
                            reference_binding::Binding::Output { column, .. } => {
                                edges.push(column);
                                crate::dataset::SelectionRead::Column(column)
                            }
                        };
                        operands.push(crate::dataset::SelectionSource {
                            path: operand_path,
                            read,
                        });
                    }
                    Some(Expression::FirstAvailable(alloc::boxed::Box::new(
                        crate::dataset::FirstAvailable::new(operands, selection.missing.clone()),
                    )))
                }
                Operation::Compute(compiled) => {
                    // The reference emits expression-local qualification findings first,
                    // then validates bare references, preserving written occurrence order.
                    for identifier in compiled.identifiers().filter(|name| {
                        name.contains('.') && self.intermediates.reference(name).is_none()
                    }) {
                        findings.push(BindFinding::QualifiedNumericReference {
                            path: reference_path.clone(),
                            expression: compiled.expression().into(),
                            identifier: identifier.into(),
                        });
                    }
                    let mut bindings = Vec::new();
                    for name in compiled
                        .identifiers()
                        .filter(|name| self.intermediates.reference(name).is_some())
                    {
                        let (index, field) = self
                            .intermediates
                            .reference(name)
                            .ok_or(BindError::Internal)?;
                        if let Some(item) = &intermediates[index] {
                            edges.extend(item.output_dependencies());
                            if let Some(column) = secondary[item.source]
                                .schema
                                .columns()
                                .iter()
                                .position(|c| c.name == field)
                            {
                                bindings.push(Binding {
                                    name: name.into(),
                                    read: Read::Intermediate { index, column },
                                });
                            } else {
                                findings.push(BindFinding::UnknownReference {
                                    path: reference_path.clone(),
                                    name: name.into(),
                                });
                            }
                        } else {
                            findings.push(BindFinding::UnknownReference {
                                path: reference_path.clone(),
                                name: name.into(),
                            });
                        }
                    }
                    for name in compiled.identifiers().filter(|name| !name.contains('.')) {
                        if let Some(binding) = bind(name, &mut findings)? {
                            let read = match binding {
                                reference_binding::Binding::Output { column, .. } => {
                                    edges.push(column);
                                    Read::Column(column)
                                }
                                reference_binding::Binding::Dataset { field, .. } => {
                                    Read::Source(field)
                                }
                            };
                            bindings.push(Binding {
                                name: name.into(),
                                read,
                            });
                        }
                    }
                    if findings.len() == start {
                        Some(Expression::Compute(
                            BoundNumeric::new(compiled.clone(), bindings)
                                .map_err(BindError::InvalidNumericBinding)?,
                        ))
                    } else {
                        None
                    }
                }
            };
            // Invalid declarations remain in the graph so independent cycles and
            // key dependencies are still reported after the reference findings.
            dependencies.push(Some(edges));
            if let Some(expression) = expression {
                assignments.insert(
                    column,
                    Assignment {
                        column,
                        expression,
                        path: path.clone(),
                    },
                );
            }
        }
        let analysis =
            column_dependencies::analyze(&dependencies, &self.keys, false, Default::default())
                .map_err(BindError::DependencyPolicy)?;
        if !analysis.diagnostics.is_empty() {
            findings.push(BindFinding::Dependencies {
                columns: self
                    .output
                    .columns()
                    .iter()
                    .map(|c| c.name.clone())
                    .collect(),
                paths: self.declarations.iter().map(|d| d.path.clone()).collect(),
                diagnostics: analysis.diagnostics,
            });
        }
        let root_filter = self
            .root_filter
            .as_ref()
            .map(|filter| filter.bind(&self.source().name, source, &mut findings))
            .transpose()?
            .flatten();
        if !findings.is_empty() {
            return Err(BindError::Invalid(findings));
        }
        let mut key_assignments = Vec::new();
        let mut columns = Vec::new();
        for column in analysis.order {
            let assignment = assignments.remove(&column).expect("analyzed declaration");
            if self.keys.contains(&column) {
                key_assignments.push(assignment)
            } else {
                columns.push(assignment)
            }
        }
        DatasetPlan::new_with_intermediates(
            crate::dataset::SourceSchemas {
                primary: source.clone(),
                secondary,
            },
            intermediates
                .into_iter()
                .map(|item| item.expect("validated intermediate"))
                .collect(),
            self.output.clone(),
            vec![RowTemplate {
                mode: RowMode::Keys,
                assignments: key_assignments,
                filter: root_filter,
            }],
            columns,
            self.keys.clone(),
            self.verifications.checks.clone(),
        )
        .and_then(|plan| plan.with_column_verifications(self.column_verification_groups()))
        .and_then(|plan| plan.with_warning_verifications(self.warning_verification_paths()))
        .and_then(|plan| plan.with_output_order(self.bound_output_order()))
        .and_then(|plan| {
            plan.with_conversion_handlers(
                self.declarations
                    .iter()
                    .filter_map(|declaration| {
                        declaration.handler.as_ref().map(|handler| {
                            crate::dataset::ConversionHandler {
                                assignment_path: declaration.path.clone(),
                                handler: handler.clone(),
                            }
                        })
                    })
                    .collect(),
            )
        })
        .map_err(BindError::InvalidPlan)
    }
}

#[derive(Debug)]
pub enum OutputFinding {
    UnknownProfile { path: String },
    InvalidDecimals { value: String },
    DecimalsNotApplicable { path: String },
    DuplicateColumn { position: usize, name: String },
    UndeclaredColumn { position: usize, name: String },
    InternalKey { position: usize, name: String },
    DuplicateOrderTerm { position: usize, name: String },
    UndeclaredOrderColumn { position: usize, name: String },
}
fn output_basename(path: &str) -> &str {
    path.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .next_back()
        .unwrap_or("")
}
fn output_profile(path: &str) -> Option<&'static str> {
    let (stem, suffix) = output_basename(path).rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    if suffix.eq_ignore_ascii_case("csv") {
        Some("csv")
    } else if suffix.eq_ignore_ascii_case("parquet") {
        Some("parquet")
    } else {
        None
    }
}
