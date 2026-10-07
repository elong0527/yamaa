//! Bounded original-specification compiler prototype: single untyped CSV driver,
//! implicit key grain, source columns and checked numeric computations.
//! Admission never reads study data; binding consumes only the captured source schema.
use crate::{
    dataset::{Assignment, BoundNumeric, DatasetPlan, Expression, RowMode, RowTemplate},
    dataset_predicate::{Binding, Read},
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};
use yamaa_core::{
    column_dependencies,
    numeric_compiler::{compile_numeric, CompileError, CompiledNumeric},
    numeric_parser::ParseError,
    reference_binding::{self, Catalog},
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::{Column, TableSchema},
    value::ColumnType,
};

#[path = "specification_rows.rs"]
mod rows;
#[path = "specification_verifications.rs"]
mod verifications;

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
    OutputSchema(yamaa_core::table::SchemaError),
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
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreflightFinding {
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
}
#[derive(Debug)]
pub enum BindError {
    Invalid(Vec<BindFinding>),
    Catalog(reference_binding::Error),
    DependencyPolicy(column_dependencies::Error),
    InvalidNumericBinding(crate::dataset_predicate::BindingError),
    InvalidPlan(crate::dataset::PlanError),
}
#[derive(Debug)]
pub enum BindFinding {
    QualifiedReference {
        path: String,
        name: String,
        row: Option<String>,
        finding: yamaa_core::reference_scope::Finding,
    },
    Aggregate {
        path: String,
        expression: String,
        error: yamaa_core::aggregate_parser::GrammarFailure,
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
#[derive(Clone, Debug)]
pub struct SourceDeclaration {
    pub name: String,
    pub path: String,
    pub types: Vec<(String, ColumnType)>,
}
#[derive(Clone, Debug)]
enum Operation {
    Source(String),
    Compute(CompiledNumeric),
    /// Reference planning emits formula diagnostics after source ingestion.
    InvalidNumeric {
        expression: String,
        error: CompileError,
    },
}
#[derive(Clone, Debug)]
struct Declaration {
    path: String,
    operation: Operation,
}
/// Private admitted representation: no caller-provided typed plan or binding indices.
#[derive(Debug)]
pub struct PreparedSpecification {
    source: SourceDeclaration,
    output: TableSchema,
    projection: Vec<String>,
    output_path: String,
    keys: Vec<usize>,
    declarations: Vec<Declaration>,
    rows: Option<rows::Rows>,
    verifications: verifications::Verifications,
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
            if let N::Text(value) = node {
                model_text_bytes = model_text_bytes
                    .checked_add(value.len())
                    .filter(|&n| n <= limits.model_text_bytes)
                    .ok_or(PrepareError::Limit("model_text_bytes"))?;
            }
        }
        // Per-expression parser limits remain in force. Charge the entire sum
        // before compiling any formula, including malformed/unsupported formulas.
        let mut numeric_bytes = 0usize;
        for &column in sequence(d, field(d, root, "columns")?)? {
            let expr = d
                .field(column, "derivation")
                .and_then(|id| d.field(id, "value"))
                .and_then(|id| d.field(id, "compute"))
                .and_then(|id| d.field(id, "expr"));
            if let Some(id) = expr {
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
        let findings = preflight(spec)?;
        if !findings.is_empty() {
            return Err(PrepareError::Invalid(findings));
        }
        let mut extra = Vec::new();
        let has_rows = present(d, root, "rows");
        optional_features(
            d,
            root,
            &[
                "parents",
                "filter",
                "intermediates",
                "submission",
                "metadata",
            ],
            "",
            &mut extra,
        );
        let inputs = mapping(d, field(d, root, "input")?)?;
        if inputs.len() != 1 {
            reject(&mut extra, "multiple_sources", "input".into());
        }
        let driver = match spec.default_driver() {
            Some(driver) => driver,
            None if !extra.is_empty() => return Err(PrepareError::Unsupported(extra)),
            None => return Err(PrepareError::Internal),
        };
        let selected = inputs
            .iter()
            .find(|&&(name, _)| text(d, name).is_ok_and(|name| name == driver))
            .ok_or(PrepareError::Internal)?
            .1;
        for &(name, id) in inputs {
            let prefix = format!("input.{}", text(d, name)?);
            optional_features(d, id, &["schema", "ordinal"], &prefix, &mut extra);
            if d.field(id, "empty_string")
                .is_some_and(|id| !matches!(&d.nodes()[id],N::Text(v) if v=="missing"))
            {
                reject(&mut extra, "empty_string", format!("{prefix}.empty_string"));
            }
            if !text(d, field(d, id, "path")?)?.ends_with(".csv") {
                reject(&mut extra, "source_format", format!("{prefix}.path"));
            }
        }
        // Unsupported row/intermediate/metadata semantics must never reach the
        // closed no-row lowering representation, even when their shapes are valid.
        if extra.iter().any(|feature| {
            matches!(
                feature.operation.as_str(),
                "parents" | "intermediates" | "submission"
            )
        }) {
            return Err(PrepareError::Unsupported(extra));
        }
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
        let source = SourceDeclaration {
            name: driver.into(),
            types: source_types,
            path: text(d, field(d, selected, "path")?)?.into(),
        };
        let output_id = field(d, root, "output")?;
        optional_features(
            d,
            output_id,
            &["order_by", "warning_log", "verification_log"],
            "output",
            &mut extra,
        );
        optional_features(d, output_id, &["decimals"], "output", &mut extra);
        let output_path = String::from(text(d, field(d, output_id, "path")?)?);
        if output_profile(&output_path) == Some("parquet") {
            reject(&mut extra, "output_parquet", "output.path".into());
        }
        // Output declaration errors retain their post-verification phase.
        let projection = sequence(d, field(d, output_id, "columns")?)?
            .iter()
            .map(|&id| text(d, id).map(String::from))
            .collect::<Result<Vec<_>, _>>()?;
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
        if has_rows {
            if !extra.is_empty() {
                return Err(PrepareError::Unsupported(extra));
            }
            let rows = rows::Rows::prepare(d, &output, driver, limits)?;
            return Ok(Self {
                source,
                output,
                projection,
                output_path,
                keys,
                declarations: Vec::new(),
                rows: Some(rows),
                verifications,
            });
        }
        let mut declarations = Vec::new();
        for (column, &id) in columns.iter().enumerate() {
            let prefix = format!("columns.{}", output.columns()[column].name);
            optional_features(
                d,
                id,
                &["verifications", "submission", "metadata"],
                &prefix,
                &mut extra,
            );
            let derivation = d
                .field(id, "derivation")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
                .ok_or(PrepareError::Internal)?;
            if d.field(derivation, "unconvertible").is_some() {
                reject(
                    &mut extra,
                    "unconvertible",
                    format!("{prefix}.derivation.unconvertible"),
                );
            }
            let ops = mapping(d, field(d, derivation, "value")?)?;
            let &[(op, payload)] = ops else {
                return Err(PrepareError::Internal);
            };
            let op = text(d, op)?;
            let path = format!("{prefix}.derivation.{op}");
            let operation = match op {
                "source" => {
                    // The expression payload is admitted by captured schema, not the model.
                    for &(name, _) in mapping(d, payload)? {
                        let name = text(d, name)?;
                        if name != "variable" {
                            reject(&mut extra, name, format!("{path}.{name}"));
                        }
                    }
                    Some(Operation::Source(
                        text(d, field(d, payload, "variable")?)?.into(),
                    ))
                }
                "compute" => {
                    for &(name, _) in mapping(d, payload)? {
                        let name = text(d, name)?;
                        if name != "expr" {
                            reject(&mut extra, name, format!("{path}.{name}"));
                        }
                    }
                    let expr = text(d, field(d, payload, "expr")?)?;
                    match compile_numeric(expr, &path, Default::default()) {
                        Ok(compiled) => Some(Operation::Compute(compiled)),
                        Err(error @ CompileError::Parse(ParseError::Grammar { .. })) => {
                            Some(Operation::InvalidNumeric {
                                expression: expr.into(),
                                error,
                            })
                        }
                        Err(CompileError::Unsupported { .. }) => {
                            reject(&mut extra, "numeric_function", path.clone());
                            None
                        }
                        Err(error) => {
                            return Err(PrepareError::Numeric {
                                path: format!("{path}.expr"),
                                error,
                            })
                        }
                    }
                }
                _ => {
                    reject(&mut extra, op, path.clone());
                    None
                }
            };
            if let Some(operation) = operation {
                declarations.push(Declaration { path, operation });
            }
        }
        if !extra.is_empty() {
            return Err(PrepareError::Unsupported(extra));
        }
        Ok(Self {
            source,
            output,
            projection,
            output_path,
            keys,
            declarations,
            rows: None,
            verifications,
        })
    }
    pub fn source(&self) -> &SourceDeclaration {
        &self.source
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
    pub fn verification_identity(&self, path: &str) -> Option<&str> {
        self.verifications
            .checks
            .iter()
            .position(|c| c.path == path)
            .and_then(|index| self.verifications.identities[index].as_deref())
    }
    pub fn output_path(&self) -> &str {
        &self.output_path
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
        findings
    }

    /// Bind only immutable source metadata. No cell reads or expression evaluation.
    pub fn bind(&self, source: &TableSchema) -> Result<DatasetPlan, BindError> {
        if let Some(rows) = &self.rows {
            return rows.bind(
                source,
                &self.source.name,
                &self.output,
                &self.keys,
                self.verifications(),
            );
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
                name: &self.source.name,
                fields: &source_fields,
            }],
            Default::default(),
        )
        .map_err(BindError::Catalog)?;
        let mut assignments = BTreeMap::new();
        let mut dependencies = Vec::new();
        let mut findings = Vec::new();
        for (column, declaration) in self.declarations.iter().enumerate() {
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
                Operation::InvalidNumeric { expression, error } => {
                    findings.push(BindFinding::Numeric {
                        path: reference_path.clone(),
                        expression: expression.clone(),
                        error: error.clone(),
                    });
                    None
                }
                Operation::Source(name) => {
                    bind(name, &mut findings)?.map(|binding| match binding {
                        reference_binding::Binding::Dataset { field, .. } => {
                            if self.keys.contains(&column) {
                                Expression::Source(field)
                            } else {
                                Expression::Collect {
                                    column: field,
                                    identifier: name.clone(),
                                    filter: None,
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
                Operation::Compute(compiled) => {
                    // The reference emits expression-local qualification findings first,
                    // then validates bare references, preserving written occurrence order.
                    for identifier in compiled.identifiers().filter(|name| name.contains('.')) {
                        findings.push(BindFinding::QualifiedNumericReference {
                            path: reference_path.clone(),
                            expression: compiled.expression().into(),
                            identifier: identifier.into(),
                        });
                    }
                    let mut bindings = Vec::new();
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
        DatasetPlan::new(
            source.clone(),
            self.output.clone(),
            vec![RowTemplate {
                mode: RowMode::Keys,
                assignments: key_assignments,
                filter: None,
            }],
            columns,
            self.keys.clone(),
            self.verifications.checks.clone(),
        )
        .map_err(BindError::InvalidPlan)
    }
}

#[derive(Debug)]
pub enum OutputFinding {
    UnknownProfile { path: String },
    DuplicateColumn { position: usize, name: String },
    UndeclaredColumn { position: usize, name: String },
    InternalKey { position: usize, name: String },
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
