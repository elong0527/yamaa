//! Verification admission and deferred declaration diagnostics, read from the
//! typed tree. Every declaration's vocabulary is admitted before any payload
//! compiles; the valid prefix executes only after derivation and output keys.
use super::*;
mod lookups;
use crate::json::Value as JsonValue;
use crate::{
    ast::{
        self, ColumnCheckKind, DatasetCheck, DatasetCheckKind, ExprId, ExprKind, Literal, Severity,
    },
    bound_expression::BoundPredicate,
    dataset::{Check, CountBoundError, RowCount, RowCountBound, Verification},
    diagnostic::{ConditionCode, Diagnostic},
    value::Value,
};

/// The operations a column's `verifications` admit.
const COLUMN_OPERATIONS: [&str; 5] = [
    "not_missing",
    "allowed_values",
    "range",
    "max_length",
    "matches",
];
/// The operations the root `verifications` admit.
const DATASET_OPERATIONS: [&str; 4] = ["unique", "all_or_none", "assert", "row_count"];

enum DeclarationFinding {
    Reason {
        path: String,
        code: ConditionCode,
        reason: String,
    },
    Diagnostic(Diagnostic),
}

/// Why compiling a check ends the valid prefix: a declaration finding, after
/// any predicate checkpoint that precedes it, or a refusal of the whole spec.
enum Stop {
    Defer(Box<(DeclarationFinding, Option<Verification>)>),
    Refuse(PrepareError),
}
impl From<PrepareError> for Stop {
    fn from(error: PrepareError) -> Self {
        Self::Refuse(error)
    }
}
impl From<DeclarationFinding> for Stop {
    fn from(finding: DeclarationFinding) -> Self {
        Self::Defer(Box::new((finding, None)))
    }
}
fn defer(path: impl Into<String>, code: ConditionCode, reason: impl Into<String>) -> Stop {
    DeclarationFinding::Reason {
        path: path.into(),
        code,
        reason: reason.into(),
    }
    .into()
}
fn deferred(diagnostic: Diagnostic) -> Stop {
    DeclarationFinding::Diagnostic(diagnostic).into()
}

/// Compile one dataset predicate: grammar text over the output columns and
/// the fields of named intermediates.
fn predicate(
    spec: &ast::Spec,
    root: ExprId,
    output: &TableSchema,
) -> Result<Result<BoundPredicate, Diagnostic>, PrepareError> {
    let expr = spec.expr(root);
    let path = spec.cite(expr.meta);
    let ExprKind::Formula(formula) = &expr.kind else {
        return Ok(Err(Diagnostic {
            code: ConditionCode::VerificationDeclarationRequired,
            spec_paths: vec![path],
            context: [(
                "reason".into(),
                JsonValue::String("a predicate must be text".into()),
            )]
            .into(),
            source_span: None,
            operand_route: None,
        }));
    };
    let plan = match &formula.compiled {
        Ok(plan) => plan,
        // Every grammar failure, including an invalid expression, reports the
        // shared structured {position, predicate} context that `when` and
        // `filter` report; verifications add no prose-only variant.
        Err(CompileError::Invalid {
            position, failure, ..
        }) => {
            return Ok(Err(failure.diagnostic(
                ExpressionContext::Predicate,
                &path,
                &formula.text.text,
                position.character,
            )));
        }
        Err(error) => return Err(PrepareError::expression(error.clone(), Some(&path))),
    };
    let column = |name: &str| {
        output
            .columns()
            .iter()
            .position(|column| column.name == name)
    };
    if let Some(name) = plan
        .identifiers()
        .filter(|name| column(name).is_none() && lookups::declared(spec, name).is_none())
        .min()
    {
        return Ok(Err(Diagnostic {
            code: ConditionCode::VerificationUnknownReference,
            spec_paths: vec![path],
            context: [("identifier".into(), JsonValue::String(name.into()))].into(),
            source_span: None,
            operand_route: None,
        }));
    }
    let names: std::collections::BTreeSet<_> = plan.identifiers().collect();
    let bindings = names
        .into_iter()
        .map(|name| Binding {
            name: name.into(),
            read: match column(name) {
                Some(column) => Read::Column(column),
                None => Read::Intermediate {
                    index: lookups::declared(spec, name).expect("declared lookup"),
                    column: 0,
                },
            },
        })
        .collect();
    // An output column has its declared type; a lookup's field is typed when
    // its schema is captured.
    let typed = plan.typed(|name| {
        crate::result_type::ResultType::declared(column(name).map(|c| output.columns()[c].kind))
    });
    BoundPredicate::new_contextual(typed, bindings, Vec::new())
        .map(Ok)
        .map_err(|_| PrepareError::Internal)
}

#[derive(Debug, Default)]
pub(super) struct Verifications {
    pub checks: Vec<Verification>,
    pub identities: Vec<Option<String>>,
    pub warning_paths: Vec<String>,
    pub declared_warning_paths: Vec<String>,
}

fn admit_identity(
    id: Option<&ast::Text>,
    path: &str,
    ids: &mut BTreeMap<String, String>,
) -> Result<Option<String>, Stop> {
    let Some(id) = id else {
        return Ok(None);
    };
    if id.text.is_empty() {
        return Err(defer(
            path,
            ConditionCode::VerificationIdentifierType,
            "a verification id is text",
        ));
    }
    if let Some(previous) = ids.get(&id.text) {
        return Err(defer(
            path,
            ConditionCode::VerificationIdentifierRepeated,
            format!(
                "verification id {} repeats {previous}",
                crate::schema::quoted_diagnostic_text(&id.text)
            ),
        ));
    }
    ids.insert(id.text.clone(), path.into());
    Ok(Some(id.text.clone()))
}

/// A dataset `unique` over exactly the `keys` columns restates the unique,
/// non-missing identity output-key validation already asserts.
fn redundant_unique(spec: &ast::Spec, result: &mut Vec<PreflightFinding>) {
    let keys = spec
        .keys
        .iter()
        .map(|key| key.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for check in &spec.checks {
        let DatasetCheckKind::Unique { columns } = &check.kind else {
            continue;
        };
        let names = columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>();
        if names.is_empty()
            || names
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != keys
        {
            continue;
        }
        result.push(PreflightFinding::Scalar(Diagnostic {
            code: ConditionCode::VerificationUniqueRedundant,
            spec_paths: vec![spec.cite(check.meta)],
            context: [
                (
                    "columns".into(),
                    JsonValue::Array(
                        names
                            .iter()
                            .map(|name| JsonValue::String((*name).into()))
                            .collect(),
                    ),
                ),
                ("stated_by".into(), JsonValue::String("keys".into())),
            ]
            .into(),
            source_span: None,
            operand_route: None,
        }));
    }
}

/// The output column a check names; an unknown column ends the prefix at
/// `{path}.{field}`.
fn named_column(
    named: &ast::Named,
    output: &TableSchema,
    path: &str,
    field: &str,
) -> Result<usize, Stop> {
    output
        .columns()
        .iter()
        .position(|column| column.name == named.name)
        .ok_or_else(|| {
            defer(
                format!("{path}.{field}"),
                ConditionCode::VerificationDeclaredColumnUnknown,
                format!(
                    "unknown column {}",
                    crate::schema::quoted_diagnostic_text(&named.name)
                ),
            )
        })
}

/// A written row-count bound: canonical integer text of any width.
fn count_bound(bound: Option<&ast::Constant>) -> Result<Option<RowCountBound>, PrepareError> {
    bound
        .map(|bound| {
            let Literal::Int(value) = &bound.value else {
                return Err(PrepareError::Internal);
            };
            RowCountBound::new(value).map_err(|error| match error {
                CountBoundError::Invalid => PrepareError::Internal,
            })
        })
        .transpose()
}

/// A written row-count fraction: an integer or a finite binary64 value.
fn fraction(bound: Option<&ast::Constant>) -> Result<Option<f64>, PrepareError> {
    bound
        .map(|bound| match &bound.value {
            Literal::Int(value) => value.parse::<f64>().map_err(|_| PrepareError::Internal),
            Literal::Float(value) => Ok(*value),
            _ => Err(PrepareError::Internal),
        })
        .transpose()
}

/// A dataset check's predicates, compiled before any declaration finding
/// defers: `[when, require]` for `assert`, `[filter, when]` for `row_count`.
type Predicates = [Option<Result<BoundPredicate, Diagnostic>>; 2];

fn compile_predicates(
    spec: &ast::Spec,
    check: &DatasetCheck,
    path: &str,
    output: &TableSchema,
) -> Result<Predicates, PrepareError> {
    let compile = |root: Option<ExprId>| root.map(|root| predicate(spec, root, output)).transpose();
    Ok(match &check.kind {
        DatasetCheckKind::Assert { when, require } => {
            let when = compile(*when)?;
            [when, compile(Some(*require))?]
        }
        // An `assert` that writes no `require` text has nothing to compile.
        DatasetCheckKind::Unsupported(operation) if operation == "assert" => [
            None,
            Some(Err(Diagnostic {
                code: ConditionCode::VerificationDeclarationRequired,
                spec_paths: vec![format!("{path}.require")],
                context: [(
                    "reason".into(),
                    JsonValue::String("a predicate must be text".into()),
                )]
                .into(),
                source_span: None,
                operand_route: None,
            })),
        ],
        DatasetCheckKind::RowCount(count) => {
            let filter = compile(count.filter)?;
            [filter, compile(count.when)?]
        }
        _ => [None, None],
    })
}

/// Compile one dataset check over the output columns and its predicates.
fn dataset_check(
    check: &DatasetCheck,
    path: &str,
    predicates: &mut Predicates,
    output: &TableSchema,
) -> Result<Check, Stop> {
    match &check.kind {
        DatasetCheckKind::Unique { columns } | DatasetCheckKind::AllOrNone { columns } => {
            if columns.is_empty() {
                return Err(defer(
                    path,
                    ConditionCode::VerificationDeclarationRequired,
                    "columns names at least one column",
                ));
            }
            let columns = columns
                .iter()
                .map(|named| named_column(named, output, path, "columns"))
                .collect::<Result<Vec<_>, _>>()?;
            if matches!(check.kind, DatasetCheckKind::Unique { .. }) {
                Ok(Check::Unique(columns))
            } else if columns.iter().all(|column| columns.first() == Some(column)) {
                Err(defer(
                    path,
                    ConditionCode::VerificationCardinality,
                    "all_or_none names two distinct columns",
                ))
            } else {
                Ok(Check::AllOrNone(columns))
            }
        }
        // An `assert` without `require` text lowers as unsupported; its
        // missing predicate is the finding.
        DatasetCheckKind::Assert { .. } | DatasetCheckKind::Unsupported(_) => {
            let [when, require] = predicates;
            let when = when.take().transpose().map_err(deferred)?;
            match require.take() {
                Some(Ok(require)) => Ok(Check::Assert { when, require }),
                // A `when` compiled before an invalid `require` is its checkpoint.
                Some(Err(diagnostic)) => Err(Stop::Defer(Box::new((
                    DeclarationFinding::Diagnostic(diagnostic),
                    when.map(|when| Verification {
                        path: when.plan().spec_path().into(),
                        check: Check::PredicateDeclaration(when),
                    }),
                )))),
                // Any other operation lowers as unsupported only when its
                // payload is outside the language.
                None => Err(PrepareError::Internal.into()),
            }
        }
        DatasetCheckKind::RowCount(count) => row_count(count, path, predicates, output),
    }
}

fn row_count(
    count: &ast::RowCount,
    path: &str,
    predicates: &mut Predicates,
    output: &TableSchema,
) -> Result<Check, Stop> {
    let min = count_bound(count.min.as_ref())?;
    let max = count_bound(count.max.as_ref())?;
    let min_fraction = fraction(count.min_fraction.as_ref())?;
    let max_fraction = fraction(count.max_fraction.as_ref())?;
    for (field, bound) in [
        ("min_fraction", min_fraction),
        ("max_fraction", max_fraction),
    ] {
        if bound.is_some_and(|n| !n.is_finite() || !(0.0..=1.0).contains(&n)) {
            return Err(defer(
                format!("{path}.{field}"),
                ConditionCode::VerificationDeclarationRequired,
                "a row_count fraction is between 0 and 1",
            ));
        }
    }
    let bounds = |reason| Err(defer(path, ConditionCode::VerificationBounds, reason));
    if min.is_none() && max.is_none() && min_fraction.is_none() && max_fraction.is_none() {
        return bounds("row_count requires one bound");
    }
    if matches!((&min, &max), (Some(a), Some(b)) if a > b) {
        return bounds("row_count min exceeds max");
    }
    if matches!((min_fraction, max_fraction), (Some(a), Some(b)) if a > b) {
        return bounds("row_count min_fraction exceeds max_fraction");
    }
    let grouping = |reason| defer(path, ConditionCode::VerificationGrouping, reason);
    let group_by = count
        .group_by
        .as_ref()
        .map(|groups| {
            if groups.is_empty() {
                return Err(grouping("group_by names at least one column"));
            }
            let mut columns = Vec::new();
            for group in groups {
                let column = named_column(group, output, path, "group_by")?;
                if columns.contains(&column) {
                    return Err(grouping("row_count group_by repeats a column"));
                }
                columns.push(column);
            }
            Ok(columns)
        })
        .transpose()?;
    let [filter, when] = predicates;
    let filter = filter.take().transpose().map_err(deferred)?;
    let when = when.take().transpose().map_err(deferred)?;
    Ok(
        if group_by.is_none()
            && filter.is_none()
            && when.is_none()
            && min_fraction.is_none()
            && max_fraction.is_none()
        {
            Check::WideRowCount { min, max }
        } else {
            Check::ScopedRowCount(RowCount {
                min,
                max,
                min_fraction,
                max_fraction,
                group_by,
                filter,
                when,
            })
        },
    )
}

/// Every column's verifications: its declared checks, then those its
/// terminology, value-level entries, `mandatory` and submission metadata
/// imply. Their findings join `findings` in that order, with the dataset
/// `unique` that restates the keys between `mandatory` and submission.
pub(super) fn prepare_columns(
    spec: &ast::Spec,
    output: &TableSchema,
    catalogue: Option<&crate::project_terminology::Catalogue>,
    inventory: &[String],
    findings: &mut Vec<PreflightFinding>,
) -> Result<Vec<Verifications>, PrepareError> {
    let mut groups = (0..spec.columns.len())
        .map(|column| Verifications::prepare_column(spec, output, column))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(catalogue) = catalogue {
        match terminology::apply(spec, output, catalogue, &mut groups) {
            Ok(()) => (),
            Err(PrepareError::Invalid(errors)) => findings.extend(errors),
            Err(error) => return Err(error),
        }
    }
    value_level::admit(spec, output, &mut groups, findings)?;
    mandatory::admit(spec, output, &mut groups, findings);
    redundant_unique(spec, findings);
    submission::admit(spec, output, inventory, &mut groups, findings)?;
    Ok(groups)
}

impl Verifications {
    pub(super) fn bind(
        &self,
        spec: &PreparedSpecification,
        secondary: &[crate::dataset::SecondarySource],
        intermediates: &[Option<crate::dataset::Intermediate>],
    ) -> Result<Vec<Verification>, BindError> {
        lookups::bind(&self.checks, spec, secondary, intermediates)
    }

    pub(super) fn append_metadata(&mut self, verification: Verification) {
        // Ordinary explicit/implicit column checks precede terminology hooks.
        let index = self
            .checks
            .iter()
            .position(|verification| {
                matches!(
                    verification.check,
                    crate::dataset::Check::Codelist { .. }
                        | crate::dataset::Check::ValueCodelists { .. }
                        | crate::dataset::Check::Inventory { .. }
                )
            })
            .unwrap_or(self.checks.len());
        self.checks.insert(index, verification);
        self.identities.insert(index, None);
    }
    pub(super) fn append_codelist(&mut self, verification: Verification) {
        self.checks.push(verification);
        self.identities.push(None);
    }

    /// Refuse every operation outside `admitted`, before any payload
    /// compiles, and record each declared `warning`. Each check is its path
    /// with its unsupported operation, if any, and its severity.
    fn vocabulary<'a>(
        checks: impl Iterator<Item = (&'a str, Option<&'a str>, Severity)>,
        admitted: &[&str],
    ) -> Result<Self, PrepareError> {
        let mut result = Self::default();
        let mut extra = Vec::new();
        for (path, unsupported, severity) in checks {
            if let Some(operation) = unsupported.filter(|operation| !admitted.contains(operation)) {
                reject(&mut extra, operation, path.into());
            } else if severity == Severity::Warning {
                result.declared_warning_paths.push(path.into());
            }
        }
        if !extra.is_empty() {
            return Err(PrepareError::Unsupported(extra));
        }
        Ok(result)
    }

    /// Compile checks in declaration order, each after its identifier, until
    /// one defers a finding: that closes the valid prefix, and the finding is
    /// reported at its checkpoint.
    fn compile_prefix<'a, C>(
        &mut self,
        checks: impl Iterator<Item = (&'a C, String, Option<&'a ast::Text>, Severity)>,
        mut compile: impl FnMut(usize, &'a C, &str) -> Result<Check, Stop>,
    ) -> Result<(), PrepareError>
    where
        C: 'a,
    {
        let mut ids = BTreeMap::<String, String>::new();
        for (index, (check, path, id, severity)) in checks.enumerate() {
            let compiled = admit_identity(id, &path, &mut ids)
                .and_then(|identity| Ok((identity, compile(index, check, &path)?)));
            let (finding, checkpoint) = match compiled {
                Ok((identity, compiled)) => {
                    if severity == Severity::Warning {
                        self.warning_paths.push(path.clone());
                    }
                    self.checks.push(Verification {
                        path,
                        check: compiled,
                    });
                    self.identities.push(identity);
                    continue;
                }
                Err(Stop::Refuse(error)) => return Err(error),
                Err(Stop::Defer(deferral)) => *deferral,
            };
            for verification in checkpoint.into_iter().chain([match finding {
                DeclarationFinding::Reason { path, code, reason } => Verification {
                    path,
                    check: Check::InvalidDeclaration { code, reason },
                },
                DeclarationFinding::Diagnostic(diagnostic) => Verification {
                    path: diagnostic.spec_paths[0].clone(),
                    check: Check::InvalidDiagnostic(diagnostic),
                },
            }]) {
                self.checks.push(verification);
                self.identities.push(None);
            }
            break;
        }
        Ok(())
    }

    /// The root `verifications`, over the completed dataset.
    pub(super) fn prepare(spec: &ast::Spec, output: &TableSchema) -> Result<Self, PrepareError> {
        let paths = spec
            .checks
            .iter()
            .map(|check| spec.cite(check.meta))
            .collect::<Vec<_>>();
        let mut result = Self::vocabulary(
            spec.checks.iter().zip(&paths).map(|(check, path)| {
                let unsupported = match &check.kind {
                    DatasetCheckKind::Unsupported(operation) => Some(operation.as_str()),
                    _ => None,
                };
                (path.as_str(), unsupported, check.severity)
            }),
            &DATASET_OPERATIONS,
        )?;
        // Admit every predicate before any payload finding defers: a later
        // refusal still matters when an earlier finding makes it unreachable.
        let mut predicates = spec
            .checks
            .iter()
            .zip(&paths)
            .map(|(check, path)| compile_predicates(spec, check, path, output))
            .collect::<Result<Vec<_>, _>>()?;
        result.compile_prefix(
            spec.checks
                .iter()
                .zip(paths)
                .map(|(check, path)| (check, path, check.id.as_ref(), check.severity)),
            |index, check, path| dataset_check(check, path, &mut predicates[index], output),
        )?;
        Ok(result)
    }

    /// One column's `verifications`, over its completed values.
    pub(super) fn prepare_column(
        spec: &ast::Spec,
        output: &TableSchema,
        column: usize,
    ) -> Result<Self, PrepareError> {
        let checks = &spec.columns[column].checks;
        let kind = output.columns()[column].kind;
        let paths = checks
            .iter()
            .map(|check| spec.cite(check.meta))
            .collect::<Vec<_>>();
        let mut result = Self::vocabulary(
            checks.iter().zip(&paths).map(|(check, path)| {
                let unsupported = match &check.kind {
                    ColumnCheckKind::Unsupported(operation) => Some(operation.as_str()),
                    _ => None,
                };
                (path.as_str(), unsupported, check.severity)
            }),
            &COLUMN_OPERATIONS,
        )?;
        result.compile_prefix(
            checks
                .iter()
                .zip(paths)
                .map(|(check, path)| (check, path, check.id.as_ref(), check.severity)),
            |_, check, path| match column_checks::prepare(check, path, kind)? {
                Check::InvalidDiagnostic(diagnostic) => Err(deferred(diagnostic)),
                Check::InvalidDeclaration { code, reason } => Err(defer(path, code, reason)),
                compiled => Ok(compiled),
            },
        )?;
        Ok(result)
    }
}
