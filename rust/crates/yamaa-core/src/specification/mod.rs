//! Bounded original-specification compiler: declared typed sources,
//! implicit key grain, source columns and checked numeric computations.
//! Admission never reads study data; binding consumes only the captured source schema.
use crate::{
    ast, column_dependencies,
    expression::{compile, CompileError, CompiledExpression, Context as ExpressionContext},
    reference_binding::{self, Catalog},
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::{Column, TableSchema},
    value::ColumnType,
};
use crate::{
    bound_expression::{Binding, Read},
    dataset::{
        Assignment, BoundNumeric, DatasetPlan, Expression, ResolvedPlan, RowMode, RowTemplate,
    },
};
use std::{collections::BTreeMap, vec};

mod compiler_limits;
pub use compiler_limits::CompilerLimit;
mod binder;
use binder::BindContext;
mod binding_diagnostics;
mod checks;
mod intermediate_admission;
mod intermediate_derivations;
mod intermediate_range;
mod intermediates;
mod lookup_diagnostics;
mod lookup_range;
mod lowering_audit;
mod match_values;
mod relation_predicates;
mod types;
pub use lookup_diagnostics::LookupFinding;
use lookup_diagnostics::ReferenceCause;
mod output_diagnostics;
mod phases;
mod preflight_diagnostics;
mod root_filter;
mod source_expressions;
pub use source_expressions::SourceFinding;
mod column_checks;
mod report;
mod row_drivers;
mod row_intermediates;
mod rows;
mod self_relations;
mod verifications;
pub use report::ReportFinding;
mod windows;
pub use windows::WindowFinding;
mod functions;
pub use functions::{Cause as FunctionCause, FunctionFinding};
mod aggregates;
mod case_predicates;
mod cases;
mod derivations;
mod flags;
mod imputation;
mod mandatory;
mod mapping_cut;
mod odm;
mod rounding;
mod scalar;
mod str_case;
mod str_extract;
mod str_pad;
mod str_template;
mod submission;
mod temporal;
mod terminology;
mod value_level;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedFeature {
    pub operation: String,
    pub path: String,
}
#[derive(Debug, thiserror::Error)]
pub enum PrepareError {
    #[error("specification limit: {0:?}")]
    Limit(CompilerLimit),
    #[error("specification unsupported: {0:?}")]
    Unsupported(Vec<UnsupportedFeature>),
    #[error("specification numeric: path={path:?}, error={error:?}")]
    Numeric { path: String, error: CompileError },
    #[error("specification invalid: {0:?}")]
    Invalid(Vec<PreflightFinding>),
    #[error("specification output schema: {0:?}")]
    OutputSchema(crate::table::SchemaError),
    #[error("specification internal")]
    Internal,
}
/// Maximum declared CSV display precision under CSV-6.
pub const MAX_OUTPUT_DECIMALS: u8 = 18;

#[derive(Clone, Debug, PartialEq)]
pub enum PreflightFinding {
    IntegerLiteralOverflow {
        path: String,
        value: String,
    },
    Odm(crate::diagnostic::Diagnostic),
    Scalar(crate::diagnostic::Diagnostic),
    TextExtract(crate::diagnostic::Diagnostic),
    TextTemplate(crate::diagnostic::Diagnostic),
    TextCase(crate::diagnostic::Diagnostic),
    Report(ReportFinding),
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
    ProjectCodelist(std::boxed::Box<crate::project_codelist_binding::Finding>),
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
        /// Why the column-level derivation is not row-local (SPEC-6): the
        /// root cause kind, such as `aggregate`, `lookup`, `window` or
        /// `named_intermediate`.
        not_row_local: String,
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
#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("specification internal")]
    Internal,
    #[error("specification invalid diagnostic")]
    InvalidDiagnostic,
    #[error("specification source count")]
    SourceCount,
    #[error("specification expression policy: {0:?}")]
    ExpressionPolicy(CompileError),
    #[error("specification invalid predicate binding: {0:?}")]
    InvalidPredicateBinding(crate::bound_expression::BindingError),
    #[error("specification invalid: {0:?}")]
    Invalid(Vec<BindFinding>),
    #[error("specification catalog: {0:?}")]
    Catalog(reference_binding::Error),
    #[error("specification dependency policy: {0:?}")]
    DependencyPolicy(column_dependencies::Error),
    #[error("specification invalid numeric binding: {0:?}")]
    InvalidNumericBinding(crate::bound_expression::BindingError),
    #[error("specification invalid plan: {0:?}")]
    InvalidPlan(crate::dataset::PlanError),
    /// The bound plan failed a resolution check (#2840); refused as internal.
    #[error("specification unresolved plan: {0}")]
    Resolution(crate::dataset::ResolutionError),
}
#[derive(Debug)]
pub enum BindFinding {
    Odm(crate::diagnostic::Diagnostic),
    AggregateDiagnostic(crate::diagnostic::Diagnostic),
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
        error: crate::expression::Failure,
    },
    AggregateScope {
        path: String,
        expression: String,
        relation: Option<String>,
        driver: String,
    },
    AggregateGroupScope {
        path: String,
        expression: String,
        relation: String,
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
        /// The row template of a row-phase read; absent in the column phase.
        row: Option<String>,
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
    pub encoding: Option<String>,
    pub empty_string_present: bool,
    pub ordinal: Option<String>,
}
/// The closed source profile is selected by the authored path, never its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProfile {
    Csv,
    Parquet,
    Xport,
    Sas7bdat,
}
/// Core-selected semantic source interpretation, independent of storage profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceInterpretation {
    Ordinary,
    Odm,
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
        } else if extension.eq_ignore_ascii_case("xpt") {
            Some(Self::Xport)
        } else if extension.eq_ignore_ascii_case("sas7bdat") {
            Some(Self::Sas7bdat)
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
    Odm(std::boxed::Box<odm::Declaration>),
    Aggregate(std::boxed::Box<aggregates::Declaration>),
    MappingCut(std::boxed::Box<mapping_cut::Declaration>),
    Scalar(std::boxed::Box<scalar::Declaration>),
    Flag(flags::Declaration),
    StrExtract(crate::text_extract::StringExtract),
    StrReplace(crate::text_replace::StringReplace),
    StrTemplate(crate::text_template::StringTemplate),
    Case(Vec<cases::Branch>),
    ProjectFunction {
        call: usize,
    },
    Literal(crate::value::Value),
    Window(std::boxed::Box<windows::Declaration>),
    Source(std::boxed::Box<source_expressions::Declaration>),
    FirstAvailable(std::boxed::Box<source_expressions::FirstAvailable>),
    Compute(CompiledExpression),
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
    /// The document node of the derivation's operation.
    value: usize,
    operation: Operation,
}
/// Private admitted representation: no caller-provided typed plan or binding indices.
#[derive(Debug)]
pub struct PreparedSpecification {
    check_expressions: Vec<crate::diagnostic::Diagnostic>,
    /// The intermediate filter type faults that written declarations decide.
    written_filter_types: Vec<crate::diagnostic::Diagnostic>,
    project_calls: Option<crate::project_calls::ProjectCalls>,
    sources: Vec<SourceDeclaration>,
    source_interpretations: Vec<SourceInterpretation>,
    /// The producing specification's output contract for each linked source.
    source_contracts: Vec<Option<crate::producer_contract::Contract>>,
    /// Inputs declaring `require_match: true` (LOOKUP-24).
    required_matches: Vec<String>,
    driver: usize,
    intermediates: intermediates::Declarations,
    output: TableSchema,
    projection: Vec<String>,
    output_path: String,
    output_report: Option<String>,
    output_decimals: Option<String>,
    output_order: Vec<(String, bool, bool)>,
    keys: Vec<usize>,
    /// How the rows are built: the implicit template of a spec without
    /// `rows`, or its row templates.
    rows: rows::Rows,
    verifications: verifications::Verifications,
    column_verifications: Vec<verifications::Verifications>,
    /// The typed tree of the same admitted document, which result typing reads.
    tree: types::Tree,
}

/// A static finding borrowed from one admitted compilation. Allocate its
/// portable diagnostic only after the caller has admitted ownership capacity.
#[derive(Clone, Copy, Debug)]
pub enum VerificationDeclarationFinding<'a> {
    Diagnostic(&'a crate::diagnostic::Diagnostic),
    Declaration {
        path: &'a str,
        code: crate::diagnostic::ConditionCode,
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
            Self::Declaration { path, code, reason } => {
                crate::dataset_checks::declaration_diagnostic(path.into(), code, reason.into())
                    .expect("compiled verification finding has a registered cause")
            }
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

/// `{literal: null}`, the one spelling of the missing value a handler, flag,
/// mapping entry or function argument substitutes (COLUMN-10).
pub(crate) fn missing_literal(d: &Document, id: usize) -> bool {
    matches!(&d.nodes()[id], N::Mapping(items) if matches!(
        items.as_slice(),
        &[(key, value)] if matches!(&d.nodes()[key], N::Text(key) if key == "literal")
            && matches!(d.nodes()[value], N::Null)
    ))
}

/// Read an admitted scalar leaf once; rows and columns share the same boundary.
fn literal(d: &Document, id: usize, path: &str) -> Result<crate::value::Value, PrepareError> {
    use crate::value::Value;
    Ok(match &d.nodes()[id] {
        N::Null => Value::Missing,
        N::Mapping(_) if missing_literal(d, id) => Value::Missing,
        N::Text(value) => Value::Str(value.clone()),
        N::Boolean(value) => Value::Bool(*value),
        N::Float(value) => Value::float(*value),
        N::Integer(value) => Value::Int(value.parse().map_err(|_| {
            PrepareError::Invalid(vec![PreflightFinding::IntegerLiteralOverflow {
                path: path.into(),
                value: value.clone(),
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
/// A lookup or structured source asserts a match only with `require_match:
/// true`; omitted or false, a row without a record is answered (LOOKUP-24).
pub(crate) fn requires_match(d: &Document, id: usize) -> bool {
    d.field(id, "require_match")
        .is_some_and(|id| matches!(d.nodes()[id], N::Boolean(true)))
}
/// Decode one admitted order term as `(variable, descending, nulls_first)`.
/// Every `list[order_by_term]` field uses this one reading (VALUE-22): an
/// omitted `nulls` sorts missing lowest, first under `asc` and last under
/// `desc`, while a declared `nulls` is absolute and never flips.
fn order_term(d: &Document, term: usize) -> Result<(String, bool, bool), PrepareError> {
    let (name, direction, nulls) = match &d.nodes()[term] {
        N::Text(name) => (name.as_str(), None, None),
        _ => (
            text(d, field(d, term, "variable")?)?,
            d.field(term, "direction")
                .map(|id| text(d, id))
                .transpose()?,
            d.field(term, "nulls").map(|id| text(d, id)).transpose()?,
        ),
    };
    let descending = match direction {
        None | Some("asc") => false,
        Some("desc") => true,
        Some(_) => return Err(PrepareError::Internal),
    };
    let nulls_first = match nulls {
        None => !descending,
        Some("first") => true,
        Some("last") => false,
        Some(_) => return Err(PrepareError::Internal),
    };
    Ok((name.into(), descending, nulls_first))
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

type BuiltSources = (
    Vec<SourceDeclaration>,
    Vec<SourceInterpretation>,
    Vec<Option<crate::producer_contract::Contract>>,
);

fn build_sources<'a>(
    d: &'a Document,
    inputs: &[(usize, usize)],
    odm_inputs: &std::collections::BTreeSet<&'a str>,
    contracts: Vec<(String, crate::producer_contract::Contract)>,
) -> Result<BuiltSources, PrepareError> {
    let mut sources = Vec::new();
    let mut source_interpretations = Vec::new();
    let mut source_contracts = Vec::new();
    let mut contracts = contracts.into_iter().peekable();
    for &(name, selected) in inputs {
        source_interpretations.push(if odm_inputs.contains(text(d, name)?) {
            SourceInterpretation::Odm
        } else {
            SourceInterpretation::Ordinary
        });
        let mut source_types = Vec::new();
        if let Some(id) = d
            .field(selected, "types")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            let fields = mapping(d, id)?;
            if fields.len() > crate::limits::Limits::COLUMNS {
                return Err(PrepareError::limit(
                    "source_fields",
                    crate::limits::Limits::COLUMNS,
                    Some(fields.len()),
                    Some(&format!("input.{}.types", text(d, name)?)),
                ));
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
            encoding: d
                .field(selected, "encoding")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| text(d, id).map(String::from))
                .transpose()?,
            empty_string_present: d
                .field(selected, "empty_string")
                .is_some_and(|id| matches!(&d.nodes()[id], N::Text(value) if value == "present")),
            ordinal: d
                .field(selected, "ordinal")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .map(|id| text(d, id).map(String::from))
                .transpose()?,
        });
        // Admission returns contracts in authored input order, so a linked
        // source always takes the next contract.
        let contract = contracts
            .next_if(|(dataset, _)| dataset == text(d, name).unwrap_or(""))
            .map(|(_, contract)| contract);
        if let Some(contract) = &contract {
            // The producing specification's output contract is the sole type
            // authority for the stored fields.
            sources.last_mut().ok_or(PrepareError::Internal)?.types = contract
                .fields()
                .iter()
                .map(|f| (f.name.clone(), f.kind))
                .collect();
        }
        source_contracts.push(contract);
    }
    if contracts.next().is_some() {
        return Err(PrepareError::Internal);
    }
    Ok((sources, source_interpretations, source_contracts))
}

struct PreparedOutputDeclarations {
    output_order: Vec<(String, bool, bool)>,
    output_path: String,
    output_decimals: Option<String>,
    projection: Vec<String>,
    keys: Vec<usize>,
    verifications: verifications::Verifications,
    output_report: Option<String>,
}

/// Admit the output declaration, the keys and the dataset verifications,
/// then the run report the declared warnings require.
fn prepare_output_declarations(
    spec: &ast::Spec,
    output: &TableSchema,
    column_verifications: &[verifications::Verifications],
) -> Result<PreparedOutputDeclarations, PrepareError> {
    let declared = &spec.output;
    // Every `list[order_by_term]` field reads its terms one way (VALUE-22): an
    // omitted `nulls` sorts missing lowest, first under `asc` and last under
    // `desc`, while a declared `nulls` is absolute and never flips.
    let output_order = declared
        .order_by
        .iter()
        .map(|term| {
            let ast::ExprKind::Ref(name) = &spec.expr(term.variable).kind else {
                return Err(PrepareError::Internal);
            };
            let descending = term.direction == ast::Direction::Desc;
            let nulls_first = match term.nulls {
                None => !descending,
                Some(ast::Nulls::First) => true,
                Some(ast::Nulls::Last) => false,
            };
            Ok((name.as_str().into(), descending, nulls_first))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output_path = declared.path.text.clone();
    // Retain the complete integer for output-declaration diagnostics, even
    // when it exceeds CSV-6's range. Build validation retains its existing
    // post-verification phase; saved-file size is admitted at publication.
    let output_decimals = declared
        .decimals
        .as_ref()
        .map(|decimals| match &decimals.value {
            ast::Literal::Int(value) => Ok(value.clone()),
            _ => Err(PrepareError::Internal),
        })
        .transpose()?;
    // Output declaration errors retain their post-verification phase.
    let projection = declared
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    let names = output
        .columns()
        .iter()
        .enumerate()
        .map(|(id, c)| (c.name.as_str(), id))
        .collect::<BTreeMap<_, _>>();
    let keys = spec
        .keys
        .iter()
        .map(|key| {
            names
                .get(key.name.as_str())
                .copied()
                .ok_or(PrepareError::Internal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let verifications = verifications::Verifications::prepare(spec, output)?;
    let warnings = column_verifications
        .iter()
        .flat_map(|group| &group.declared_warning_paths)
        .chain(&verifications.declared_warning_paths)
        .cloned()
        .collect::<Vec<_>>();
    let (output_report, report_findings) = report::prepare(declared, warnings);
    if !report_findings.is_empty() {
        return Err(PrepareError::Invalid(
            report_findings
                .into_iter()
                .map(PreflightFinding::Report)
                .collect(),
        ));
    }
    Ok(PreparedOutputDeclarations {
        output_order,
        output_path,
        output_decimals,
        projection,
        keys,
        verifications,
        output_report,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_column_declarations(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    driver: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
    keys: &[usize],
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Vec<Declaration>, PrepareError> {
    let mut declarations = Vec::new();
    for (column, &id) in columns.iter().enumerate() {
        let prefix = format!("columns.{}", output.columns()[column].name);

        let derivation = d
            .field(id, "derivation")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
            .ok_or(PrepareError::Internal)?;
        let handler = literal_handler(d, derivation, &format!("{prefix}.derivation"))?;
        let value = field(d, derivation, "value")?;
        let ops = mapping(d, value)?;
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
                    extra,
                    intermediates,
                )?;
                Some(Operation::Source(std::boxed::Box::new(source)))
            }
            "mapping" | "cut" => Some(Operation::MappingCut(std::boxed::Box::new(
                mapping_cut::prepare(d, payload, op, &path, intermediates, extra)?,
            ))),
            "first_available" => {
                let selection = source_expressions::FirstAvailable::prepare(
                    d,
                    payload,
                    &path,
                    extra,
                    intermediates,
                )?;
                Some(Operation::FirstAvailable(std::boxed::Box::new(selection)))
            }
            "row_number" | "rank" | "row_value" | "locf" | "baseline_flag" => {
                Some(Operation::Window(std::boxed::Box::new(
                    windows::Declaration::prepare(d, payload, op, &path)?,
                )))
            }
            _ => match derivations::prepare_value_with_driver(
                d,
                value,
                &format!("{prefix}.derivation"),
                project,
                intermediates,
                keys.contains(&column),
                0,
                extraction,
                templates,
                Some(driver),
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
            declarations.push(Declaration {
                handler,
                path,
                value,
                operation,
            });
        }
    }
    Ok(declarations)
}

impl PreparedSpecification {
    /// Reject unsupported declarations before requesting any source bytes. Effective
    /// source empty-string behavior is missing unless explicitly declared otherwise.
    pub fn prepare(spec: &SpecificationDocument) -> Result<Self, PrepareError> {
        Self::prepare_implementation(spec, None, None, Default::default(), None, None)
    }
    /// Prepare calls against statically admitted environment definitions. No
    /// project imports, lock verification, test invocation or study reads occur.
    pub fn prepare_with_project(
        spec: &SpecificationDocument,
        functions: &[crate::project_function::Function],
    ) -> Result<Self, PrepareError> {
        Self::prepare_implementation(spec, Some(functions), None, Default::default(), None, None)
    }
    /// Compile project calls and fixed terminology against the same admitted metadata.
    pub fn prepare_with_environment(
        spec: &SpecificationDocument,
        environment: &crate::project_environment::ExecutionEnvironment,
    ) -> Result<Self, PrepareError> {
        Self::prepare_linked(spec, Some(environment), Default::default())
    }
    /// Compile with the producing specifications that `schema` links name. Each
    /// link supplies only its output contract's field names, order and types;
    /// a linked input without supplied metadata is invalid.
    pub fn prepare_linked(
        spec: &SpecificationDocument,
        environment: Option<&crate::project_environment::ExecutionEnvironment>,
        links: crate::producer_contract::Links<'_>,
    ) -> Result<Self, PrepareError> {
        Self::prepare_implementation(
            spec,
            environment.map(|e| e.functions()),
            environment.map(|e| e.catalogue()),
            links,
            environment.map(|e| e.inventory()),
            environment.and_then(|e| e.undeclared_language()),
        )
    }
    fn prepare_implementation(
        spec: &SpecificationDocument,
        functions: Option<&[crate::project_function::Function]>,
        catalogue: Option<&crate::project_terminology::Catalogue>,
        links: crate::producer_contract::Links<'_>,
        inventory: Option<&crate::submission_inventory::Inventory>,
        undeclared: Option<crate::project_function::Language>,
    ) -> Result<Self, PrepareError> {
        let d = spec.document();
        let root = d.root();
        // The typed tree of the same admitted document. Project calls are
        // analysed on it; every other declaration is still compiled from
        // the document below.
        let lowered = crate::front_end::v0_5::lower(spec, None);
        checks::root::limits(lowered.spec())?;
        // Link admission precedes every other declaration finding: a refused
        // type authority or contract leaves nothing to compile against.
        let contracts = crate::producer_contract::admit(spec, links).map_err(|error| {
            use crate::producer_contract::AdmissionError;
            match error {
                AdmissionError::Limit(limit) => {
                    PrepareError::limit(limit.resource, limit.limit, limit.actual, None)
                }
                AdmissionError::Invalid(findings) => PrepareError::Invalid(
                    findings.into_iter().map(PreflightFinding::Scalar).collect(),
                ),
                AdmissionError::Boundary => PrepareError::Internal,
            }
        })?;
        match_values::declaration_admission(d)?;
        let mut findings = checks::root::findings(lowered.spec());
        findings.extend(checks::odm::context_findings(lowered.spec())?);
        let columns = sequence(d, field(d, root, "columns")?)?;
        if findings.iter().any(|finding| {
            matches!(finding,
                PreflightFinding::Scalar(diagnostic)
                if diagnostic.code == crate::diagnostic::ConditionCode::PreflightDuplicateColumn
            )
        }) {
            return Err(PrepareError::Invalid(findings));
        }
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
        // Absence supplies no callable definitions. It does not invent an
        // environment document, and unknown calls retain ordinary static causes.
        // A project without an environment for the host names that cause instead.
        let functions = functions.unwrap_or(&[]);
        let project = match functions::prepare(d, &lowered, columns, functions, undeclared) {
            Ok(project) => Some(project),
            Err(PrepareError::Invalid(errors)) => {
                findings.extend(errors);
                None
            }
            Err(error) => return Err(error),
        };
        let inventory = inventory.map_or(&[][..], |inventory| inventory.names());
        let column_verifications = verifications::prepare_columns(
            lowered.spec(),
            &output,
            catalogue,
            inventory,
            &mut findings,
        )?;
        // Operand types before capture: each link's producer types its input.
        let declared = types::DeclaredTypes::new(
            lowered.spec(),
            contracts
                .iter()
                .map(|(dataset, contract)| (dataset.as_str(), contract)),
        );
        let checks::verbs::Text {
            extraction,
            templates,
        } = checks::verbs::findings(d, &lowered, &declared, &mut findings)?;
        drop(declared);
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
        optional_features(d, root, &["parent"], "", &mut extra);
        let root_filter = root_filter::RootFilter::prepare(d)?;
        let inputs = mapping(d, field(d, root, "input")?)?;
        let odm_inputs = checks::odm::inputs(lowered.spec())?;
        let driver = if has_rows {
            checks::lookups::common_driver(lowered.spec(), d)?
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
        let source_findings = checks::root::source_findings(lowered.spec(), d, &odm_inputs)?;
        if !source_findings.is_empty() {
            return Err(PrepareError::Invalid(source_findings));
        }
        // Unsupported row/intermediate/metadata semantics must never reach the
        // closed no-row lowering representation, even when their shapes are valid.
        if extra.iter().any(|feature| feature.operation == "parent") {
            return Err(PrepareError::Unsupported(extra));
        }
        let (sources, source_interpretations, source_contracts) =
            build_sources(d, inputs, &odm_inputs, contracts)?;
        // Every implicit join through these inputs must find a record.
        let required_matches = inputs
            .iter()
            .filter(|&&(_, id)| requires_match(d, id))
            .map(|&(name, _)| text(d, name).map(String::from))
            .collect::<Result<Vec<_>, _>>()?;
        let PreparedOutputDeclarations {
            output_order,
            output_path,
            output_decimals,
            projection,
            keys,
            verifications,
            output_report,
        } = prepare_output_declarations(lowered.spec(), &output, &column_verifications)?;
        let intermediates = intermediates::Declarations::prepare(
            d,
            lowered.spec(),
            &sources,
            driver_index,
            &output,
            &keys,
            project.as_ref(),
            &extraction,
            &templates,
            &mut extra,
        )?;
        if has_rows {
            if !extra.is_empty() {
                return Err(PrepareError::Unsupported(extra));
            }
            let rows = rows::Templates::prepare(
                d,
                &lowered,
                &output,
                driver,
                project.as_ref(),
                catalogue.is_some(),
                &extraction,
                &templates,
                &intermediates,
            )?;
            let project = match project {
                Some(project) => {
                    let uses = project.row_uses(rows.declaration_values())?;
                    Some(functions::select_rows(project, functions, &uses)?)
                }
                None => None,
            };
            let tree = types::Tree::new(lowered, d, project.as_ref());
            return Self {
                check_expressions: Vec::new(),
                written_filter_types: Vec::new(),
                project_calls: project.map(|p| p.calls),
                sources,
                source_interpretations,
                source_contracts,
                required_matches,
                driver: driver_index,
                intermediates,
                output,
                projection,
                output_path,
                output_report,
                output_decimals,
                output_order,
                keys,
                rows: rows::Rows::Templates(rows),
                verifications,
                column_verifications,
                tree,
            }
            .finish(d);
        }
        let declarations = prepare_column_declarations(
            d,
            columns,
            &output,
            driver,
            project.as_ref(),
            &intermediates,
            &extraction,
            &templates,
            &keys,
            &mut extra,
        )?;
        if !extra.is_empty() {
            return Err(PrepareError::Unsupported(extra));
        }
        let tree = types::Tree::new(lowered, d, project.as_ref());
        Self {
            check_expressions: Vec::new(),
            written_filter_types: Vec::new(),
            project_calls: project.map(|p| p.calls),
            sources,
            source_interpretations,
            source_contracts,
            required_matches,
            driver: driver_index,
            intermediates,
            output,
            projection,
            output_path,
            output_report,
            output_decimals,
            output_order,
            keys,
            rows: rows::Rows::Implicit(rows::Implicit::new(declarations, root_filter)),
            verifications,
            column_verifications,
            tree,
        }
        .finish(d)
    }
    /// Keep the declaration findings `check()` reports, then audit the
    /// lowered tree in debug builds.
    fn finish(mut self, d: &Document) -> Result<Self, PrepareError> {
        let refusal = |error| match error {
            BindError::ExpressionPolicy(error) => PrepareError::expression(error, None),
            _ => PrepareError::Internal,
        };
        let findings = checks::declared::findings(&self, d).map_err(refusal)?;
        self.written_filter_types =
            checks::declared::written_filter_types(&self, d).map_err(refusal)?;
        for finding in findings {
            let diagnostics = finding
                .diagnostics(self.source())
                .ok_or(PrepareError::Internal)?;
            self.check_expressions.extend(diagnostics);
        }
        Ok(self.audit_lowering())
    }
    /// All intrinsically known declaration findings, without study binding.
    /// Actual dataset fields and values remain a build-time responsibility.
    pub fn check_declaration_findings(
        &self,
    ) -> impl Iterator<Item = VerificationDeclarationFinding<'_>> {
        self.check_expressions
            .iter()
            .map(VerificationDeclarationFinding::Diagnostic)
            .chain(self.verification_declaration_findings())
    }
    pub fn check_declaration_diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        // Output-declaration findings are intrinsically known too, so `check`
        // reports the same findings `domain` reaches at its output stage.
        // They follow the declaration findings, matching build order: output
        // declaration checks come after derivation, key checks and
        // verification, so a first issue agrees between the two routes.
        let mut diagnostics = self
            .check_declaration_findings()
            .map(VerificationDeclarationFinding::diagnostic)
            .collect::<Vec<_>>();
        diagnostics.extend(self.output_findings().iter().map(OutputFinding::diagnostic));
        diagnostics
    }
    /// Declared types can refuse a donor filter even when source schemas are
    /// unavailable: the faults written declarations decide, without the
    /// `str` that INPUT-5 gives an undeclared field once its input is read.
    /// Other declaration checkpoints remain separate.
    pub fn intermediate_filter_type_diagnostics(
        &self,
    ) -> impl Iterator<Item = &crate::diagnostic::Diagnostic> {
        self.written_filter_types
            .iter()
            .filter(|finding| self.intermediates.owns_filter_type_finding(finding))
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
    pub fn source_interpretation(&self, index: usize) -> Option<SourceInterpretation> {
        self.source_interpretations.get(index).copied()
    }
    /// The producing specification's output contract a linked source must match.
    pub fn source_contract(&self, index: usize) -> Option<&crate::producer_contract::Contract> {
        self.source_contracts.get(index).and_then(Option::as_ref)
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
                Check::InvalidDeclaration { code, reason } => {
                    Some(VerificationDeclarationFinding::Declaration {
                        path: &verification.path,
                        code: *code,
                        reason,
                    })
                }
                _ => None,
            })
    }
    pub fn verification_identity(&self, path: &str) -> Option<&str> {
        std::iter::once(&self.verifications)
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
    /// The file the run report is written to, when the specification names one.
    pub fn output_report(&self) -> Option<&str> {
        self.output_report.as_deref()
    }
    /// Each row template's stable path and driver name in construction order,
    /// or root `base` and the driver when no row templates are declared.
    pub fn row_constructions(&self) -> Vec<(String, &str)> {
        match &self.rows {
            rows::Rows::Templates(rows) => rows
                .drivers()
                .enumerate()
                .map(|(index, driver)| (std::format!("rows[{index}]"), driver))
                .collect(),
            rows::Rows::Implicit(_) => vec![("base".into(), self.source().name.as_str())],
        }
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
            if !value
                .parse::<u8>()
                .is_ok_and(|digits| digits <= MAX_OUTPUT_DECIMALS)
            {
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
    pub fn bind(&self, source: &TableSchema) -> Result<ResolvedPlan, BindError> {
        self.bind_sources(&[source])
    }
    /// Schemas follow authored input order, independently of the selected driver.
    /// Every bound plan, of the implicit template or of row templates, resolves
    /// once into its slot graph and passes the resolution check. Binding returns that
    /// resolved plan for runs to execute and drops the bound plan it lowered.
    pub fn bind_sources(&self, schemas: &[&TableSchema]) -> Result<ResolvedPlan, BindError> {
        if schemas.len() != self.sources.len() {
            return Err(BindError::SourceCount);
        }
        let plan = self.rows.bind(self, schemas)?;
        // A plan the stage split cannot schedule is refused before resolution
        // with the operation it does not support.
        let unsupported = aggregates::late_reads(&plan);
        if !unsupported.is_empty() {
            return Err(BindError::Invalid(unsupported));
        }
        plan.resolve().map_err(BindError::Resolution)
    }
    /// Without row templates, a key derived by a window gives each retained
    /// base record its own output row: the key table is the records
    /// themselves, not their distinct key combinations.
    pub(super) fn record_rows(&self) -> bool {
        match &self.rows {
            rows::Rows::Implicit(implicit) => implicit.record_rows(&self.keys),
            rows::Rows::Templates(_) => false,
        }
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
        .rfind(|part| !part.is_empty() && *part != ".")
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
