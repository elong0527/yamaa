//! Bounded original-specification compiler: declared typed sources,
//! implicit key grain, source columns and checked numeric computations.
//! Admission never reads study data; binding consumes only the captured source schema.
use crate::ast::{ExprId, ExprKind};
use crate::{
    ast, column_dependencies,
    expression::{CompileError, CompiledExpression, Context as ExpressionContext},
    reference_binding::{self, Catalog},
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
mod input_schemas;
use input_schemas::{InputSchema, InputSchemas};
mod intermediate_derivations;
mod intermediate_range;
mod intermediates;
pub use intermediates::IntermediateCheck;
mod lookup_diagnostics;
mod lookup_range;
mod match_values;
mod pairs;
mod relation_predicates;
mod types;
pub use lookup_diagnostics::LookupFinding;
use lookup_diagnostics::ReferenceCause;
mod output_diagnostics;
pub use output_diagnostics::unknown_artifact_profile;
mod phases;
mod preflight_diagnostics;
mod root_filter;
mod source_expressions;
pub use source_expressions::SourceFinding;
mod column_checks;
mod reads;
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
#[cfg(test)]
mod declared_keys;
mod derivations;
mod flags;
mod mandatory;
mod odm;
mod submission;
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
    /// A diagnostic as its cause renders it, a scalar verb's among them.
    Scalar(crate::diagnostic::Diagnostic),
    Report(ReportFinding),
    ProjectFunction(FunctionFinding),
    ProjectCodelist(std::boxed::Box<crate::project_codelist_binding::Finding>),
    RowPhase {
        path: String,
        identifier: String,
        row: String,
    },
    UndeclaredRowColumn {
        /// The row template's path, as diagnostics cite it.
        row_path: String,
        column: String,
    },
    DuplicateRowDefault {
        column: String,
        rows: Vec<String>,
        /// Why the column-level derivation is not row-local (SPEC-6): it, or
        /// a column it reads, holds an `aggregate` or a `window`, or reads a
        /// `SELF` intermediate (`self_intermediate`).
        not_row_local: String,
    },
    MissingRowDerivation {
        column: String,
        rows: Vec<String>,
    },
    /// A root filter beside the row templates, named by their ids (ROW-9).
    ConflictingRowConstruction {
        rows: Vec<String>,
    },
    InvalidGroup {
        row_path: String,
        row: String,
        groups: Vec<String>,
    },
    GroupReference {
        row_path: String,
        row: String,
        name: String,
        dataset: String,
    },
    RowDriverUnavailable {
        row_path: String,
        row: String,
        dataset: Option<String>,
    },
    MissingDerivation {
        column: String,
    },
    EmptyKeys,
    DuplicateKey {
        position: usize,
        identifier: String,
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
    /// A diagnostic as its cause renders it, a scalar verb's among them.
    Scalar(crate::diagnostic::Diagnostic),
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
    /// A finding the schema source does not decide: whether binding makes
    /// it, or how it words it, depends on whether an input stores a field no
    /// declaration states (#3805). Binding goes on as it does for the finding
    /// itself, but `check()` withholds it, and each read of a lookup it alone
    /// leaves unbound; `run()`, whose captured schemas decide everything,
    /// never holds one.
    Withheld(std::boxed::Box<BindFinding>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDeclaration {
    pub name: String,
    pub path: String,
    pub types: Vec<(String, ColumnType)>,
    pub profile: SourceProfile,
    pub encoding: Option<String>,
    /// The input declares that its csv source begins with one UTF-8
    /// byte-order mark, which is skipped (CSV-10).
    pub byte_order_mark: bool,
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
    /// An ODM 1.3 or 2.0 document read into the clinical-item table
    /// (`crate::odm_items`; INPUT-1, ODM-1).
    OdmXml,
    /// A Dataset-JSON 1.1 file in its single-object representation
    /// (INPUT-1, DSJSON-1).
    DatasetJson,
    /// A Dataset-JSON 1.1 file in its newline-delimited representation
    /// (INPUT-1, DSJSON-1).
    DatasetNdjson,
}
/// Core-selected semantic source interpretation, independent of storage profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceInterpretation {
    Ordinary,
    Odm,
}
impl SourceProfile {
    /// The profile a path's case-insensitive extension names (INPUT-1).
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
        } else if extension.eq_ignore_ascii_case("xml") {
            Some(Self::OdmXml)
        } else if extension.eq_ignore_ascii_case("json") {
            Some(Self::DatasetJson)
        } else if extension.eq_ignore_ascii_case("ndjson") {
            Some(Self::DatasetNdjson)
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

/// A derivation's answer for a value its declared type cannot hold
/// (`unconvertible`), kept with the path that owns it.
fn literal_handler(
    spec: &ast::Spec,
    derivation: &ast::Derivation,
) -> Result<Option<crate::conversion::LiteralHandler>, PrepareError> {
    derivation
        .unconvertible
        .as_ref()
        .map(|handler| {
            let spec_path = checks::render(spec, handler.meta);
            Ok(crate::conversion::LiteralHandler {
                value: constant(&handler.value, &spec_path)?,
                spec_path,
            })
        })
        .transpose()
}

/// A written scalar's value (COLUMN-10): an integer beyond 64 bits and a
/// value outside the language are refused at `path`, as preparation always
/// refused them.
pub(super) fn constant(
    literal: &ast::Literal,
    path: &str,
) -> Result<crate::value::Value, PrepareError> {
    literal
        .value()
        .ok_or_else(|| refused_literal(literal, path))
}

/// The refusal at `path` of a written scalar that holds no value.
fn refused_literal(literal: &ast::Literal, path: &str) -> PrepareError {
    match literal {
        ast::Literal::Int(value) => {
            PrepareError::Invalid(vec![PreflightFinding::IntegerLiteralOverflow {
                path: path.into(),
                value: value.clone(),
            }])
        }
        _ => PrepareError::Unsupported(vec![UnsupportedFeature {
            operation: "literal".into(),
            path: path.into(),
        }]),
    }
}

/// What refuses a scalar verb before any source is read, as preparation
/// reports it.
pub(super) fn refused_verb(refusal: crate::scalar_verb::Refusal) -> PrepareError {
    use crate::scalar_verb::Refusal;
    match refusal {
        Refusal::Findings(findings) => {
            PrepareError::Invalid(findings.into_iter().map(PreflightFinding::Scalar).collect())
        }
        Refusal::Literal { literal, path } => refused_literal(&literal, &path),
        Refusal::Pattern {
            resource,
            limit,
            path,
        } => PrepareError::limit(
            compiler_limits::regex_resource(resource),
            limit,
            None,
            Some(&path),
        ),
        Refusal::Unsupported { operation, path } => {
            PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: operation.into(),
                path,
            }])
        }
        Refusal::Internal => PrepareError::Internal,
    }
}

/// A written value binding reads; preparation has already admitted it.
pub(super) fn bound_constant(literal: &ast::Literal) -> Result<crate::value::Value, BindError> {
    constant(literal, "").map_err(|_| BindError::Internal)
}

fn tree_order_term(
    spec: &ast::Spec,
    term: &ast::OrderTerm,
) -> Result<(String, bool, bool), PrepareError> {
    let name = checks::name(spec, term.variable).ok_or(PrepareError::Internal)?;
    let descending = term.direction == ast::Direction::Desc;
    let nulls_first = match term.nulls {
        None => !descending,
        Some(ast::Nulls::First) => true,
        Some(ast::Nulls::Last) => false,
    };
    Ok((name.into(), descending, nulls_first))
}
fn reject(extra: &mut Vec<UnsupportedFeature>, operation: &str, path: String) {
    extra.push(UnsupportedFeature {
        operation: operation.into(),
        path,
    });
}
type BuiltSources = (
    Vec<SourceDeclaration>,
    Vec<SourceInterpretation>,
    Vec<Option<crate::producer_contract::Contract>>,
);

fn build_sources(
    spec: &ast::Spec,
    odm_inputs: &std::collections::BTreeSet<&str>,
    contracts: Vec<(String, crate::producer_contract::Contract)>,
) -> Result<BuiltSources, PrepareError> {
    let mut sources = Vec::new();
    let mut source_interpretations = Vec::new();
    let mut source_contracts = Vec::new();
    let mut contracts = contracts.into_iter().peekable();
    for input in &spec.inputs {
        let name = input.name.name.as_str();
        source_interpretations.push(if odm_inputs.contains(name) {
            SourceInterpretation::Odm
        } else {
            SourceInterpretation::Ordinary
        });
        if input.source.types.len() > crate::limits::Limits::COLUMNS {
            return Err(PrepareError::limit(
                "source_fields",
                crate::limits::Limits::COLUMNS,
                Some(input.source.types.len()),
                Some(&format!("input.{name}.types")),
            ));
        }
        let path = input.source.path.text.as_str();
        sources.push(SourceDeclaration {
            name: name.into(),
            types: input
                .source
                .types
                .iter()
                .map(|(field, kind)| (field.name.clone(), *kind))
                .collect(),
            path: path.into(),
            // The profile the front end selected for the tree's language
            // version; an input without one never produces an accepted plan.
            profile: match input.source.profile {
                Some(ast::Profile::Csv) | None => SourceProfile::Csv,
                Some(ast::Profile::Parquet) => SourceProfile::Parquet,
                Some(ast::Profile::Xport) => SourceProfile::Xport,
                Some(ast::Profile::Sas7bdat) => SourceProfile::Sas7bdat,
                Some(ast::Profile::OdmXml) => SourceProfile::OdmXml,
                Some(ast::Profile::DatasetJson) => SourceProfile::DatasetJson,
                Some(ast::Profile::DatasetNdjson) => SourceProfile::DatasetNdjson,
            },
            encoding: input.source.encoding.as_ref().map(|text| text.text.clone()),
            byte_order_mark: input.source.byte_order_mark,
            empty_string_present: input.source.empty_string == ast::EmptyString::Present,
            ordinal: input.source.ordinal.as_ref().map(|name| name.name.clone()),
        });
        // Admission returns contracts in authored input order, so a linked
        // source always takes the next contract.
        let contract = contracts
            .next_if(|(dataset, _)| dataset == name)
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

/// Each intermediate `unique` declared at `warning`, in the order of their
/// run-report rows: those of intermediates over an input, then those over
/// `SELF`, each in declaration order (CHECK-23, LOOKUP-17).
fn intermediate_warnings(spec: &ast::Spec) -> Vec<String> {
    let mut warnings = Vec::new();
    for over_self in [false, true] {
        for item in spec
            .intermediates
            .iter()
            .filter(|item| (item.dataset.name == "SELF") == over_self)
        {
            warnings.extend(
                item.checks
                    .iter()
                    .filter(|check| check.severity == ast::Severity::Warning)
                    .map(|check| spec.cite(check.meta)),
            );
        }
    }
    warnings
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
    let warnings = intermediate_warnings(spec)
        .into_iter()
        .chain(
            column_verifications
                .iter()
                .flat_map(|group| &group.declared_warning_paths)
                .chain(&verifications.declared_warning_paths)
                .cloned(),
        )
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

impl PreparedSpecification {
    /// Prepare a 1.x specification its front end lowered into `tree`, whose
    /// resolved document `document` declares its linked inputs, against the
    /// admitted environment and the producing specifications its `schema`
    /// links name, before any source is read. Each link supplies only its
    /// output contract's field names, order and types; a linked input without
    /// supplied metadata is invalid. Preparation and binding read only the
    /// tree (#3030 D76).
    pub fn prepare_tree_linked(
        tree: ast::Spec,
        document: &crate::schema::Document,
        environment: Option<&crate::project_environment::ExecutionEnvironment>,
        links: crate::producer_contract::Links<'_>,
    ) -> Result<Self, PrepareError> {
        // Without an environment, each binding only a project supplies fails
        // validation (CT-13), with the specification's other findings.
        Self::prepare_lowered(
            tree,
            document,
            environment.map(|e| e.functions()),
            environment.map(|e| e.catalogue()),
            links,
            environment.map(|e| e.inventory()),
        )
    }
    /// Admit the links `document` declares, then prepare `tree`, its
    /// lowering.
    fn prepare_lowered(
        tree: ast::Spec,
        document: &crate::schema::Document,
        functions: Option<&[crate::project_function::Function]>,
        catalogue: Option<&crate::project_terminology::Catalogue>,
        links: crate::producer_contract::Links<'_>,
        inventory: Option<&crate::submission_inventory::Inventory>,
    ) -> Result<Self, PrepareError> {
        checks::root::limits(&tree)?;
        // Link admission precedes every other declaration finding: a refused
        // type authority or contract leaves nothing to compile against.
        let contracts = crate::producer_contract::admit(document, links).map_err(|error| {
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
        Self::from_tree(tree, contracts, functions, catalogue, inventory)
    }
    /// Prepare the specification `tree`, whose linked inputs have the
    /// producer `contracts`, before any source is read: every refusal the
    /// declarations decide, then what binding reads.
    fn from_tree(
        mut tree: ast::Spec,
        contracts: Vec<(String, crate::producer_contract::Contract)>,
        functions: Option<&[crate::project_function::Function]>,
        catalogue: Option<&crate::project_terminology::Catalogue>,
        inventory: Option<&crate::submission_inventory::Inventory>,
    ) -> Result<Self, PrepareError> {
        // Historical declarations describe a received metadata record. They
        // never add constraints to data execution; explicit verifications,
        // types, executable keys and all derivations keep their ordinary checks.
        if tree.submission.as_ref().is_some_and(|metadata| {
            metadata.mode == crate::specification_submission_document::SubmissionMode::Historical
        }) {
            tree.submission = None;
            for column in &mut tree.columns {
                column.submission = None;
            }
        }
        let spec = &tree;
        // A 1.x specification prepared without a project file, which is when
        // `prepare_tree_linked` gives none of a project's functions, codelists
        // or inventory, binds nothing a project supplies: each codelist,
        // inventory vocabulary and project function it names fails validation
        // as `project_binding_missing` (CT-13).
        let projectless = functions.is_none() && catalogue.is_none() && inventory.is_none();
        match_values::declaration_admission(spec)?;
        let mut findings = checks::root::findings(spec);
        findings.extend(checks::odm::context_findings(spec)?);
        if findings.iter().any(|finding| {
            matches!(finding,
                PreflightFinding::Scalar(diagnostic)
                if diagnostic.code == crate::diagnostic::ConditionCode::PreflightDuplicateColumn
            )
        }) {
            return Err(PrepareError::Invalid(findings));
        }
        let output = TableSchema::new(
            spec.columns
                .iter()
                .map(|column| Column {
                    name: column.name.name.clone(),
                    kind: column.kind,
                })
                .collect(),
        )
        .map_err(PrepareError::OutputSchema)?;
        // Absence supplies no callable definitions. It does not invent an
        // environment document, and unknown calls retain ordinary static causes.
        let functions = functions.unwrap_or(&[]);
        let project = match functions::prepare(spec, functions, projectless) {
            Ok(project) => Some(project),
            Err(PrepareError::Invalid(errors)) => {
                findings.extend(errors);
                None
            }
            Err(error) => return Err(error),
        };
        let inventory = inventory.map_or(&[][..], |inventory| inventory.names());
        let column_verifications = verifications::prepare_columns(
            spec,
            &output,
            catalogue,
            inventory,
            projectless,
            &mut findings,
        )?;
        // Operand types before capture: each link's producer types its input.
        let declared = types::DeclaredTypes::new(
            spec,
            contracts
                .iter()
                .map(|(dataset, contract)| (dataset.as_str(), contract)),
        );
        checks::verbs::findings(spec, &declared, &mut findings)?;
        drop(declared);
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let mut extra = Vec::new();
        let has_rows = !spec.rows.templates.is_empty();
        for field in ["parent", "include"] {
            if spec.written.contains(field) {
                reject(&mut extra, field, field.into());
            }
        }
        root_filter::admit(spec, spec.rows.filter)?;
        let odm_inputs = checks::odm::inputs(spec)?;
        let driver = if has_rows {
            checks::lookups::common_driver(spec)?
        } else {
            match checks::default_driver(spec) {
                Some(driver) => driver,
                None if !extra.is_empty() => return Err(PrepareError::Unsupported(extra)),
                None => return Err(PrepareError::Internal),
            }
        };
        let driver_index = spec
            .inputs
            .iter()
            .position(|input| input.name.name == driver)
            .ok_or(PrepareError::Internal)?;
        let source_findings = checks::root::source_findings(spec, &odm_inputs)?;
        if !source_findings.is_empty() {
            return Err(PrepareError::Invalid(source_findings));
        }
        // An uncomposed spec never reaches the closed lowering representation,
        // even when its shapes are valid.
        if extra
            .iter()
            .any(|feature| matches!(feature.operation.as_str(), "parent" | "include"))
        {
            return Err(PrepareError::Unsupported(extra));
        }
        let (sources, source_interpretations, source_contracts) =
            build_sources(spec, &odm_inputs, contracts)?;
        // Every implicit join through these inputs must find a record.
        let required_matches = spec
            .inputs
            .iter()
            .filter(|input| input.require_match)
            .map(|input| input.name.name.clone())
            .collect();
        let PreparedOutputDeclarations {
            output_order,
            output_path,
            output_decimals,
            projection,
            keys,
            verifications,
            output_report,
        } = prepare_output_declarations(spec, &output, &column_verifications)?;
        let intermediates =
            intermediates::Declarations::prepare(spec, &sources, project.as_ref(), &mut extra)?;
        let admission = derivations::Admission {
            project: project.as_ref(),
            intermediates: &intermediates,
        };
        let rows = if has_rows {
            if !extra.is_empty() {
                return Err(PrepareError::Unsupported(extra));
            }
            rows::Rows::Templates(rows::Templates::prepare(spec, &output, driver, &admission)?)
        } else {
            let implicit = rows::Implicit::prepare(spec, &keys, &admission, &mut extra)?;
            if !extra.is_empty() {
                return Err(PrepareError::Unsupported(extra));
            }
            rows::Rows::Implicit(implicit)
        };
        // A template's calls take their uses once the per-template defaults
        // and overrides are known.
        let project = match (project, &rows) {
            (Some(project), rows::Rows::Templates(templates)) => {
                let uses = project.row_uses(templates.declaration_roots())?;
                Some(functions::select_rows(project, functions, &uses)?)
            }
            (project, _) => project,
        };
        let tree = types::Tree::new(tree, project.as_ref());
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
            rows,
            verifications,
            column_verifications,
            tree,
        }
        .finish()
    }
    /// Keep the declaration findings `check()` reports: what binding decides
    /// over the declared input schemas, in binding's order, without what it
    /// withholds (#3805), and the filter type faults it decides over the
    /// written ones.
    fn finish(mut self) -> Result<Self, PrepareError> {
        let refusal = |error| match error {
            BindError::ExpressionPolicy(error) => PrepareError::expression(error, None),
            _ => PrepareError::Internal,
        };
        let declared = InputSchemas::declared(&self).map_err(refusal)?;
        self.check_expressions = self.decided(&declared).map_err(refusal)?;
        let written = InputSchemas::written(&self).map_err(refusal)?;
        self.written_filter_types = self.decided(&written).map_err(refusal)?;
        Ok(self)
    }
    /// The rows of every finding binding decides over `schemas`, in its
    /// order.
    fn decided(
        &self,
        schemas: &InputSchemas,
    ) -> Result<Vec<crate::diagnostic::Diagnostic>, BindError> {
        let mut diagnostics = Vec::new();
        for finding in self.bind_findings(schemas)? {
            if !matches!(finding, BindFinding::Withheld(_)) {
                diagnostics.extend(
                    finding
                        .diagnostics(self.source())
                        .ok_or(BindError::Internal)?,
                );
            }
        }
        Ok(diagnostics)
    }
    /// Every finding binding reports over the input schemas `schemas`, in
    /// binding's order. A plan it builds still holds the filter type faults
    /// it defers until the keys a lookup matches on resolve, in the order the
    /// lookups are derived, which execution reports before any record is
    /// selected.
    fn bind_findings(&self, schemas: &InputSchemas) -> Result<Vec<BindFinding>, BindError> {
        match self.rows.bind(self, self.inputs(schemas)) {
            Ok(plan) => Ok(plan
                .intermediates()
                .iter()
                .filter_map(|item| item.filter.as_ref())
                .flat_map(|filter| filter.deferred_type_findings())
                .cloned()
                .map(BindFinding::Predicate)
                .collect()),
            Err(BindError::Invalid(findings)) => Ok(findings),
            Err(error) => Err(error),
        }
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
        if let Some(check) = self.intermediate_check(path) {
            return check.severity;
        }
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
    /// The intermediate `unique` check at `path`, which its run-report row
    /// names (LOOKUP-17, CHECK-24).
    pub fn intermediate_check(&self, path: &str) -> Option<IntermediateCheck<'_>> {
        self.intermediates.unique_check(path, &self.sources)
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
        if let Some(check) = self.intermediate_check(path) {
            return check.identity;
        }
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
                .constructions()
                .map(|(path, driver)| (path.to_string(), driver))
                .collect(),
            rows::Rows::Implicit(_) => vec![("base".into(), self.source().name.as_str())],
        }
    }
    pub fn output_profile(&self) -> Option<&'static str> {
        artifact_profile(&self.output_path)
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
        if artifact_profile(&self.output_path).is_none() {
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
    /// The spec path of the first derivation, in authored order, that calls a
    /// project function the build runs: where a build no host can run is
    /// refused (#3030 D221). Authored order is the order a formatted file
    /// writes derivations in (yaml/schema_spec.yaml): the intermediates',
    /// then the columns', then the row templates', each in declaration order.
    /// A column's derivation that every row template replaces calls nothing
    /// the build runs.
    pub fn first_calling_derivation(&self) -> Option<&str> {
        self.project_calls
            .as_ref()?
            .calls()
            .iter()
            .filter(|call| call.slot().is_some())
            .min_by_key(|call| call.located.authored)
            .map(|call| call.located.derivation.as_str())
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
        let schemas = InputSchemas::captured(schemas);
        let plan = self.rows.bind(self, self.inputs(&schemas))?;
        // A plan the stage split cannot schedule is refused before resolution
        // with the operation it does not support.
        let unsupported = aggregates::late_reads(&plan);
        if !unsupported.is_empty() {
            return Err(BindError::Invalid(unsupported));
        }
        plan.resolve().map_err(BindError::Resolution)
    }
    /// This specification's inputs, with the schemas `schemas` gives them.
    fn inputs<'a>(&'a self, schemas: &'a InputSchemas) -> binder::Inputs<'a> {
        binder::Inputs {
            sources: &self.sources,
            schemas: schemas.schemas(),
            driver: self.driver,
        }
    }
    /// Without row templates, a key derived by a window gives each retained
    /// base record its own output row: the key table is the records
    /// themselves, not their distinct key combinations.
    pub(super) fn record_rows(&self) -> bool {
        match &self.rows {
            rows::Rows::Implicit(implicit) => implicit.record_rows(),
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
/// The profile OUTPUT-5's closed mapping gives `path`: `csv` or `parquet`, by
/// the case-insensitive extension of its last segment, or `None` for any
/// other extension or none. `path` separates its segments with `/`, as a
/// written path does; a name that is only an extension, such as `.csv`, has
/// none.
pub fn artifact_profile(path: &str) -> Option<&'static str> {
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
