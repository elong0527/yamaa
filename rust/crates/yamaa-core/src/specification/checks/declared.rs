//! The declaration findings `check()` reports: what binding finds that the
//! declarations decide (#2844 decision 3), over the tree and typed by
//! [`DeclaredTypes`]. `run()` reports the same findings from binding, in
//! binding's order, and this pass reports them in that order too: the root
//! filter, the intermediates in the order they are derived, then each column
//! (or each row template's derivations and the column phase), column order,
//! and the lookup ranges each row reads. Each row-phase derivation's phase
//! boundaries follow its own defects ([`super::boundaries`]). Group scope and
//! driver scope inside row templates are not among them yet.
//!
//! Binding also decides from the declarations alone whether a key reads
//! other records than its own ([`key_record`]); it calls the same check.
use super::*;
use crate::ast::{ScalarOp, Slot, WindowFunction};
use crate::diagnostic::{
    lookup::{self, Owner},
    ConditionCode as C, Diagnostic,
};
use crate::reference_scope::Phase;
use crate::result_type::ResultType;
use std::collections::BTreeSet;

/// The rows a derivation evaluates on, as binding scopes its reads.
#[derive(Clone, Copy, Debug)]
struct Scope<'a> {
    /// The dataset whose records build those rows, once for each binding:
    /// the driver without row templates, a row template's own driver, or in
    /// the column phase the driver of each binding the derivation gets.
    drivers: &'a [&'a str],
    /// The row template of a row-phase derivation.
    row: Option<&'a str>,
    /// The grouping phase under row templates; without them there is none.
    phase: Option<Phase<'a>>,
}

struct Checks<'a> {
    d: &'a Document,
    lowered: &'a Lowered,
    spec: &'a Spec,
    prepared: &'a PreparedSpecification,
    declared: types::DeclaredTypes<'a>,
}

/// How a check types a field of a record: by its owner and field name.
type Typing<'a> = dyn Fn(&str, &str) -> Option<ResultType> + 'a;

/// An intermediate's donor record as a check types it.
struct Donor<'s> {
    index: usize,
    item: &'s ast::Intermediate,
    /// Whether the record is a completed row of the rows being built.
    self_rows: bool,
    /// Each derivation's result type, in written order.
    results: Vec<ResultType>,
    /// The derived fields and, for a completed row, every column.
    fields: BTreeMap<String, ResultType>,
}

impl Donor<'_> {
    /// A field of the record: one it derives or completes, or else a field
    /// of the input it reads, as `stored` types it.
    fn field(&self, field: &str, stored: &Typing<'_>) -> Option<ResultType> {
        self.fields.get(field).copied().or_else(|| {
            (!self.self_rows)
                .then(|| stored(&self.item.dataset.name, field))
                .flatten()
        })
    }
}

/// Every declaration finding of `prepared`, in binding's order.
pub(in crate::specification) fn findings(
    prepared: &PreparedSpecification,
    d: &Document,
) -> Result<Vec<BindFinding>, BindError> {
    let checks = Checks {
        d,
        lowered: prepared.tree.lowered(),
        spec: prepared.tree.spec(),
        prepared,
        declared: prepared.declared_types(),
    };
    let mut findings = Vec::new();
    if let Some(filter) = checks.spec.rows.filter {
        checks.grammar(filter, &mut findings)?;
    }
    checks.intermediates(&mut findings)?;
    match &prepared.rows {
        None => checks.base(&mut findings)?,
        Some(rows) => checks.rows(rows, &mut findings)?,
    }
    checks.ranges(&mut findings)?;
    Ok(findings)
}

/// The type faults of each intermediate's filter that written declarations
/// decide. When every input is missing, binding reads no field, and `run()`
/// reports these instead of the missing inputs (`specification_run`).
pub(in crate::specification) fn written_filter_types(
    prepared: &PreparedSpecification,
    d: &Document,
) -> Result<Vec<Diagnostic>, BindError> {
    Checks {
        d,
        lowered: prepared.tree.lowered(),
        spec: prepared.tree.spec(),
        prepared,
        declared: prepared.declared_types(),
    }
    .written_filter_types()
}

/// Without row templates, each key is computed from one base record before
/// any key combination exists (COLUMN-7). Unless record rows make every
/// record its own row, an aggregate, which reduces the records of a key
/// combination, and an `odm` read, which reads them, fail as
/// `key_dependency` at the operation, also inside a `case` branch.
///
/// With record rows, a key is derived as any column is, but during row
/// construction: an unqualified aggregate, which reduces the constructed
/// rows, is not supported there yet and fails as `unsupported_operation`.
pub(in crate::specification) fn key_record(
    prepared: &PreparedSpecification,
    column: usize,
) -> Vec<BindFinding> {
    let spec = prepared.tree.spec();
    if prepared.rows.is_some() || !prepared.keys.contains(&column) {
        return Vec::new();
    }
    let Some(derivation) = spec
        .columns
        .get(column)
        .and_then(|column| column.derivation.as_ref())
    else {
        return Vec::new();
    };
    let found = positions(spec, derivation.expr)
        .into_iter()
        .filter_map(|e| match &spec.expr(e).kind {
            ExprKind::Aggregate(aggregate) => {
                Some(("aggregate", e, reduces_output(spec, aggregate)))
            }
            ExprKind::Odm(_) => Some(("odm", e, false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if prepared.record_rows() {
        return found
            .into_iter()
            .filter(|&(_, _, output)| output)
            .map(|(_, e, _)| {
                BindFinding::Scalar(crate::application_issue::unsupported(
                    "output_aggregate_key",
                    Some(&path(spec, e)),
                ))
            })
            .collect();
    }
    let name = &prepared.output.columns()[column].name;
    found
        .into_iter()
        .map(|(operation, e, _)| {
            BindFinding::Scalar(Diagnostic {
                code: C::ColumnKeyRecordRead,
                spec_paths: vec![path(spec, e)],
                context: [
                    ("column".into(), crate::json::Value::String(name.clone())),
                    (
                        "operation".into(),
                        crate::json::Value::String(operation.into()),
                    ),
                ]
                .into(),
                source_span: None,
                operand_route: None,
            })
        })
        .collect()
}

/// REQ-0442: a qualified formula name must name a declared record lookup.
/// The test reads declarations alone, so binding and check share it.
pub(in crate::specification) fn qualified_numeric_findings(
    compiled: &CompiledExpression,
    path: &str,
    intermediates: &intermediates::Declarations,
    findings: &mut Vec<BindFinding>,
) {
    for identifier in compiled
        .identifiers()
        .filter(|name| name.contains('.') && intermediates.reference(name).is_none())
    {
        findings.push(BindFinding::QualifiedNumericReference {
            path: path.into(),
            expression: compiled.expression().into(),
            identifier: identifier.into(),
        });
    }
}

/// Whether an aggregate reduces the constructed output rows: neither its
/// expression nor a derive binding names a relation (LOOKUP-18).
fn reduces_output(spec: &Spec, aggregate: &ast::Aggregate) -> bool {
    aggregate.bindings.is_empty()
        && matches!(&spec.expr(aggregate.expr).kind, ExprKind::Formula(ast::Formula {
            compiled: Ok(compiled),
            ..
        }) if !compiled.identifiers().any(|name| name.contains('.'))
            && compiled.record_relations().next().is_none())
}

/// Whether an aggregate reduces a declared relation: a derive binding or a
/// qualified name of its expression names one.
fn qualified(spec: &Spec, aggregate: &ast::Aggregate) -> bool {
    matches!(&spec.expr(aggregate.expr).kind, ExprKind::Formula(ast::Formula {
        compiled: Ok(compiled),
        ..
    }) if !aggregate.bindings.is_empty()
        || compiled.identifiers().any(|name| name.contains('.'))
        || compiled.record_relations().next().is_some())
}

impl Checks<'_> {
    /// A predicate's grammar failure, as its owner reports it. Preparation
    /// refuses every other compile failure, except in an aggregate's filter,
    /// which binding refuses.
    fn predicate(&self, e: ExprId) -> Result<Option<Diagnostic>, BindError> {
        let ExprKind::Formula(formula) = &self.spec.expr(e).kind else {
            return Ok(None);
        };
        match &formula.compiled {
            Ok(_) => Ok(None),
            Err(CompileError::Invalid {
                position, failure, ..
            }) => Ok(Some(failure.diagnostic(
                ExpressionContext::Predicate,
                &render(self.spec, formula.text.meta),
                &formula.text.text,
                position.character,
            ))),
            Err(error) => Err(BindError::ExpressionPolicy(error.clone())),
        }
    }

    fn grammar(&self, e: ExprId, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        findings.extend(self.predicate(e)?.map(BindFinding::Predicate));
        Ok(())
    }

    /// The authored defects at the positions below `root`, which binding
    /// reports before it binds any reference: each case predicate's grammar,
    /// a flag's, a `str_pad` width, a fractional literal an integer operand
    /// receives, and a formula's grammar.
    fn defects(&self, root: ExprId, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let spec = self.spec;
        match &spec.expr(root).kind {
            ExprKind::Case(arms) => {
                for arm in arms {
                    if let Some(condition) = arm.condition {
                        self.grammar(condition, findings)?;
                    }
                    self.defects(arm.result, findings)?;
                }
            }
            ExprKind::Flag {
                condition,
                false_value,
                missing,
                ..
            } => {
                self.grammar(*condition, findings)?;
                if let Some(false_value) = false_value.as_ref().filter(|_| missing.is_none()) {
                    findings.push(BindFinding::Predicate(Diagnostic {
                        code: C::FlagMissingValueRequired,
                        spec_paths: vec![format!("{}.missing", path(spec, root))],
                        context: [(
                            "false_value".into(),
                            crate::json::Value::from(value(&false_value.value)),
                        )]
                        .into(),
                        source_span: None,
                        operand_route: None,
                    }));
                }
            }
            ExprKind::Scalar {
                op: ScalarOp::StrPad { width },
                operands,
                ..
            } => {
                let source = operands
                    .first()
                    .and_then(|operand| name(spec, operand.expr))
                    .unwrap_or_default();
                let path = path(spec, root);
                if let Err(crate::text_pad::DeclarationError::Width) =
                    crate::text_pad::StringPad::new(source, &path, width)
                {
                    findings.extend(
                        crate::text_pad::DeclarationError::Width
                            .diagnostic(&path)
                            .map(BindFinding::Padding),
                    );
                }
            }
            ExprKind::Scalar { op, operands, .. } if temporal(op) => {
                if operands.iter().any(|operand| {
                    matches!(
                        spec.expr(operand.expr).kind,
                        ExprKind::Literal(ast::Literal::Float(_))
                    )
                }) {
                    let (keyword, payload) =
                        operation(self.d, self.lowered, root).map_err(|_| BindError::Internal)?;
                    let operation = temporal::prepare(
                        self.d,
                        payload,
                        keyword,
                        &path(spec, root),
                        None,
                        None,
                        &mut Vec::new(),
                    )
                    .map_err(|_| BindError::Internal)?;
                    for index in 0..operation.sources().len() {
                        if matches!(
                            operation.literal(index),
                            Some(crate::value::Value::Float(_))
                        ) && operation.expected(index) == "int"
                        {
                            findings.push(BindFinding::Temporal(
                                operation.operand_diagnostic(index, "float"),
                            ));
                        }
                    }
                }
            }
            ExprKind::Formula(formula) if formula.context == ExpressionContext::Compute => {
                if let Err(error @ CompileError::Invalid { .. }) = &formula.compiled {
                    findings.push(BindFinding::Numeric {
                        path: render(spec, formula.text.meta),
                        expression: formula.text.text.clone(),
                        error: error.clone(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The record selections at the positions below `root`: each source
    /// binding's declaration and filter, a mapping or cut's source type, each
    /// window's, each aggregate's header and filter, and each ODM read's
    /// filter. `output` types a column window's baseline operands, which only
    /// a spec without row templates does here.
    fn selections(
        &self,
        root: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        for e in positions(spec, root) {
            match &spec.expr(e).kind {
                ExprKind::Source(_) => self.source(e, scope, findings)?,
                ExprKind::FirstAvailable { sources, .. } => {
                    for &source in sources {
                        self.source(source, scope, findings)?;
                    }
                }
                ExprKind::Scalar {
                    op: ScalarOp::Mapping { .. } | ScalarOp::Cut { .. },
                    operands,
                    ..
                } => {
                    for operand in operands {
                        let start = findings.len();
                        self.source(operand.expr, scope, findings)?;
                        if findings.len() == start && scope.is_some() {
                            self.mapping_type(e, operand.expr, findings)?;
                        }
                    }
                }
                ExprKind::Window(window) => {
                    self.window(e, window, scope, findings)?;
                    if output {
                        self.baseline_types(e, window, findings);
                    }
                }
                ExprKind::Aggregate(aggregate) => {
                    let mut compiled = true;
                    if let ExprKind::Formula(formula) = &spec.expr(aggregate.expr).kind {
                        match &formula.compiled {
                            Ok(_) => {}
                            Err(CompileError::Invalid { failure, .. }) => {
                                compiled = false;
                                findings.push(BindFinding::Aggregate {
                                    path: render(spec, formula.text.meta),
                                    expression: formula.text.text.clone(),
                                    error: failure.clone(),
                                })
                            }
                            Err(_) => return Err(BindError::Internal),
                        }
                    }
                    // Binding binds nothing more of a column or row aggregate
                    // whose expression or header fails, and binds its derive
                    // steps only when nothing else of it failed.
                    let start = findings.len();
                    if scope.is_some()
                        && !(compiled && self.aggregate(e, aggregate, scope, findings)?)
                    {
                        continue;
                    }
                    if let Some(filter) = aggregate.filter {
                        self.grammar(filter, findings)?;
                    }
                    if scope.is_some() && findings.len() != start {
                        continue;
                    }
                    for binding in &aggregate.bindings {
                        self.derivation(binding.derivation.expr, None, false, findings)?;
                    }
                }
                ExprKind::Odm(read) => {
                    if let Some(filter) = read.filter {
                        self.grammar(filter, findings)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// A mapping or cut reads a source of a type it accepts (REQ-0304,
    /// REQ-0306): a bare output column's or a stored field's declared type.
    fn mapping_type(
        &self,
        e: ExprId,
        source: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let Some(kind) = variable(self.spec, source)
            .filter(|name| {
                name.split_once('.')
                    .is_none_or(|(owner, _)| !self.prepared.intermediates.contains(owner))
            })
            .and_then(|name| self.declared.name(name).column_type())
        else {
            return Ok(());
        };
        let (keyword, payload) =
            operation(self.d, self.lowered, e).map_err(|_| BindError::Internal)?;
        let declaration = mapping_cut::prepare(
            self.d,
            payload,
            keyword,
            &path(self.spec, e),
            &self.prepared.intermediates,
            &mut Vec::new(),
        )
        .map_err(|_| BindError::Internal)?;
        if !declaration.operation.accepts(kind) {
            findings.push(BindFinding::Scalar(
                declaration.operation.input_diagnostic(type_name(kind)),
            ));
        }
        Ok(())
    }

    /// What binding decides of an aggregate from its declarations before it
    /// reads a field: a reduction of several relations, a derive binding's
    /// names, groups outside its relation, a context it cannot reduce in
    /// and an undeclared relation; then, at column level, each name outside
    /// a reduction that is no group key (REQ-0503), a group that is no key
    /// (REQ-0470) and its operands' types (REQ-0510). A row template's
    /// groups are fields of its driver, which binding reads first. False
    /// when binding binds nothing more of the aggregate.
    fn aggregate(
        &self,
        e: ExprId,
        aggregate: &ast::Aggregate,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<bool, BindError> {
        use crate::diagnostic::{AggregateContextCause as Cause, AggregateFinding as Finding};
        let spec = self.spec;
        let (
            Some(scope),
            ExprKind::Formula(ast::Formula {
                compiled: Ok(compiled),
                text,
                ..
            }),
        ) = (scope, &spec.expr(aggregate.expr).kind)
        else {
            return Ok(true);
        };
        let path = path(spec, e);
        let (at, expression) = (render(spec, text.meta), text.text.as_str());
        let finding = |finding: Finding<'_>, findings: &mut Vec<BindFinding>| {
            findings.push(BindFinding::AggregateDiagnostic(
                finding.diagnostic().map_err(|_| BindError::Internal)?,
            ));
            Ok::<_, BindError>(())
        };
        let phase = scope.phase.unwrap_or(Phase::Column { groups: &[] });
        let driver = scope.drivers.first().copied().unwrap_or_default();
        let mut relations = compiled
            .identifiers()
            .filter_map(|name| {
                name.split_once('.')
                    .map(|(relation, _)| relation.to_owned())
            })
            .chain(compiled.record_relations().map(String::from))
            .collect::<Vec<_>>();
        relations.sort_unstable();
        relations.dedup();
        let bare = compiled.identifiers().any(|name| !name.contains('.'));
        if relations.len() > 1 || (!relations.is_empty() && bare) {
            if bare {
                relations.push("<output>".into());
            }
            finding(
                Finding::MixedRelations {
                    path: &at,
                    expression,
                    relations: &relations,
                },
                findings,
            )?;
            return Ok(false);
        }
        // A derive step reads exactly one relation, which preparation admits.
        let named = &self.prepared.intermediates;
        let derived = aggregate
            .bindings
            .iter()
            .flat_map(|binding| spec.reads(binding.derivation.expr, ast::Scope::Row))
            .filter_map(|read| super::name(spec, read.at).filter(|name| name.contains('.')))
            .chain(
                aggregate
                    .filter
                    .filter(|_| !aggregate.bindings.is_empty())
                    .into_iter()
                    .flat_map(|filter| spec.reads(filter, ast::Scope::Row))
                    .filter_map(|read| super::name(spec, read.at)),
            )
            .filter(|name| {
                name.split_once('.')
                    .is_some_and(|(owner, _)| !named.contains(owner))
            })
            .collect::<Vec<_>>();
        let relation = if aggregate.bindings.is_empty() {
            relations.first().map(String::as_str)
        } else {
            let mut owners = derived
                .iter()
                .filter_map(|name| name.split_once('.'))
                .map(|(owner, _)| owner)
                .collect::<Vec<_>>();
            owners.sort_unstable();
            owners.dedup();
            // Preparation found exactly one; the tree's reads name it.
            let [owner] = owners.as_slice() else {
                return Ok(true);
            };
            Some(*owner)
        };
        let row = matches!(phase, Phase::Row { .. });
        if !aggregate.bindings.is_empty() {
            let start = findings.len();
            for name in compiled.identifiers().filter(|name| {
                !name.contains('.')
                    && !aggregate
                        .bindings
                        .iter()
                        .any(|binding| binding.name.name == *name)
            }) {
                let mut finding = LookupFinding::new(&at, C::AggregateDeriveVariable);
                finding.text("variable", name);
                finding.text("expr", expression);
                findings.push(BindFinding::AggregateDiagnostic(finding.diagnostic()));
            }
            if row {
                let mut finding =
                    LookupFinding::new(&format!("{path}.derive"), C::AggregateDeriveDeclaration);
                finding.text(
                    "reason",
                    "derive binds records of a qualified right side only",
                );
                findings.push(BindFinding::AggregateDiagnostic(finding.diagnostic()));
            }
            if findings.len() != start {
                return Ok(false);
            }
        }
        let groups = aggregate
            .group_by
            .iter()
            .map(|&group| name(spec, group).unwrap_or_default())
            .collect::<Vec<_>>();
        if let (false, Some(relation)) = (row, relation) {
            if groups
                .iter()
                .any(|name| name.split_once('.').map(|(owner, _)| owner) != Some(relation))
            {
                findings.push(BindFinding::AggregateGroupScope {
                    path: format!("{path}.group_by"),
                    expression: expression.into(),
                    relation: relation.into(),
                });
                return Ok(false);
            }
        }
        let keyed = matches!(
            aggregate.key,
            Some(JoinKey::Columns { .. } | JoinKey::Pairs { .. })
        );
        let cause = match phase {
            Phase::Row { group_by: None } => Some((Cause::UngroupedRow, path.clone())),
            Phase::Row { group_by: Some(_) } if relation != Some(driver) => {
                findings.push(BindFinding::AggregateScope {
                    path: path.clone(),
                    expression: expression.into(),
                    relation: relation.map(String::from),
                    driver: driver.into(),
                });
                return Ok(false);
            }
            Phase::Row { .. } if aggregate.between.is_some() => {
                Some((Cause::RowNoRightSide, format!("{path}.between")))
            }
            Phase::Row { .. } if !groups.is_empty() => {
                Some((Cause::RowOwnsGroups, format!("{path}.group_by")))
            }
            Phase::Row { .. } if keyed => Some((Cause::RowNoKeys, format!("{path}.key"))),
            Phase::Column { .. } if relation.is_none() => {
                if aggregate.between.is_some() {
                    Some((Cause::OutputBetween, format!("{path}.between")))
                } else if groups.is_empty() {
                    Some((Cause::OutputNeedsGroups, path.clone()))
                } else if groups.iter().any(|name| name.contains('.')) {
                    Some((Cause::OutputGroupsQualified, format!("{path}.group_by")))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some((cause, path)) = cause {
            finding(
                Finding::InvalidContext {
                    path: &path,
                    expression,
                    cause,
                },
                findings,
            )?;
            return Ok(false);
        }
        // The relation is the rows' driver or another declared input; a
        // relation no binding's driver is either fails in every binding.
        if let Some(relation) = relation.filter(|relation| {
            !scope.drivers.contains(relation)
                && !self
                    .prepared
                    .sources
                    .iter()
                    .any(|input| input.name == *relation)
        }) {
            let reference = derived
                .iter()
                .copied()
                .chain(compiled.identifiers())
                .find(|name| {
                    name.split_once('.')
                        .is_some_and(|(owner, _)| owner == relation)
                });
            findings.push(BindFinding::UnknownReference {
                path: at,
                name: reference.map_or_else(|| format!("{relation}.*"), String::from),
            });
            return Ok(false);
        }
        if row {
            return Ok(true);
        }
        for identifier in compiled.ungrouped_identifiers() {
            if !groups.contains(&identifier) {
                finding(
                    Finding::IdentifierNotGrouped {
                        path: &at,
                        expression,
                        dataset: relation,
                        identifier,
                    },
                    findings,
                )?;
            }
        }
        // A qualified reduction matches donor records on its keys alone, so
        // each group is a key: a declared key's field, or else an output key,
        // which a group the relation stores is then applicable on
        // (REQ-0470).
        if let Some(relation) = relation {
            let applicable = match &aggregate.key {
                Some(JoinKey::Columns { columns, .. }) => {
                    columns.iter().map(|column| column.name.as_str()).collect()
                }
                Some(JoinKey::Pairs { pairs, .. }) => pairs
                    .iter()
                    .map(|(column, _)| column.name.as_str())
                    .collect(),
                _ => spec
                    .keys
                    .iter()
                    .map(|key| key.name.as_str())
                    .collect::<Vec<_>>(),
            };
            if groups.iter().any(|name| {
                name.strip_prefix(relation)
                    .and_then(|tail| tail.strip_prefix('.'))
                    .is_some_and(|field| !applicable.contains(&field))
            }) {
                finding(
                    Finding::InvalidContext {
                        path: &format!("{path}.group_by"),
                        expression,
                        cause: Cause::QualifiedGroupNotOutputKey,
                    },
                    findings,
                )?;
                return Ok(false);
            }
        }
        // A derive binding has its declared type, an output column of the
        // reduced rows its column's, and a field of the relation its input's.
        let kind = |name: &str| {
            if let Some(binding) = aggregate
                .bindings
                .iter()
                .find(|binding| binding.name.name == name)
            {
                return binding.declared.map(Into::into);
            }
            match (relation, name.split_once('.')) {
                (None, None) => self.declared.name(name).single(),
                (Some(relation), Some((owner, _))) if owner == relation => {
                    self.declared.name(name).single()
                }
                _ => None,
            }
        };
        findings.extend(
            types::aggregate_kinds(compiled, kind, false)?
                .into_iter()
                .map(BindFinding::AggregateDiagnostic),
        );
        Ok(true)
    }

    /// One derivation's authored defects, then its record selections.
    fn derivation(
        &self,
        root: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        self.defects(root, findings)?;
        self.selections(root, scope, output, findings)
    }

    /// A structured source binding's declaration defects, reported before
    /// any field is bound: a range without a bound, an unpaired selection
    /// and two absence policies; then a filter on one value (a group key, an
    /// output column, a row's value or a named intermediate's field), the
    /// filter's grammar and each name it reads outside its variable's input.
    fn source(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let ExprKind::Source(source) = &spec.expr(e).kind else {
            return Ok(());
        };
        let path = path(spec, e);
        let variable = name(spec, source.variable).unwrap_or_default();
        let owner = Owner::Source(variable);
        let mut defects = Vec::new();
        if let Some(between) = &source.between {
            if between.lower.is_none() && between.upper.is_none() {
                defects.push(lookup::range_declaration(
                    &render(spec, between.meta),
                    owner,
                ));
            }
        }
        if source.order_by.is_empty() != source.keep.is_none() {
            let (declared, missing) = if source.keep.is_some() {
                ("keep", "order_by")
            } else {
                ("order_by", "keep")
            };
            defects.push(lookup::selection_unpaired(&path, owner, declared, missing));
        }
        if source.require_match && source.absent.is_some() {
            defects.push(lookup::absence_conflict(
                format!("{path}.absent"),
                format!("{path}.require_match"),
                owner,
            ));
        }
        // A declared grouped key is already one scalar in both row and column
        // phase. A row-phase read's driver is its row template's own; the
        // column phase decides from the groups alone.
        if let (
            Some(filter),
            Some(Scope {
                drivers,
                phase: Some(phase),
                ..
            }),
        ) = (source.filter, scope)
        {
            let driver = drivers.first().copied().unwrap_or_default();
            if defects.is_empty() && group_key(variable, driver, phase) {
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Err(BindError::Internal);
                };
                findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                    C::SourceFilterSingleValue,
                    &render(spec, formula.text.meta),
                    &[("identifier", variable)],
                )));
                return Ok(());
            }
        }
        if !defects.is_empty() {
            findings.extend(defects.into_iter().map(|defect| {
                BindFinding::Source(source_expressions::SourceFinding::from_diagnostic(defect))
            }));
            return Ok(());
        }
        let Some(filter) = source.filter else {
            return Ok(());
        };
        let qualifier = variable.split_once('.').map(|(owner, _)| owner);
        match (
            qualifier.filter(|owner| !self.prepared.intermediates.contains(owner)),
            scope,
        ) {
            // Only a stored variable's filter is compiled. It reads only the
            // records of its variable's input, so binding refuses each other
            // name before it binds a field.
            (Some(dataset), scope) => {
                let start = findings.len();
                self.grammar(filter, findings)?;
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Ok(());
                };
                let (Some(_), true, Ok(plan)) = (
                    scope,
                    findings.len() == start
                        && self
                            .prepared
                            .sources
                            .iter()
                            .any(|input| input.name == dataset),
                    &formula.compiled,
                ) else {
                    return Ok(());
                };
                let at = render(spec, formula.text.meta);
                let mut seen = BTreeSet::new();
                for identifier in plan.identifiers() {
                    if seen.insert(identifier)
                        && identifier
                            .split_once('.')
                            .is_none_or(|(owner, _)| owner != dataset)
                    {
                        findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                            C::SourceFilterReference,
                            &at,
                            &[("identifier", identifier), ("dataset", dataset)],
                        )));
                    }
                }
            }
            // An output column, a row's value or a named intermediate's
            // field is one value already: there are no records to filter.
            // Without row templates a bare name that is no output column
            // fails as a reference first, unless the binding answers its
            // absence.
            (None, Some(scope))
                if qualifier.is_some()
                    || scope.phase.is_some()
                    || source.absent.is_some()
                    || spec
                        .columns
                        .iter()
                        .any(|column| column.name.name == variable) =>
            {
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Err(BindError::Internal);
                };
                findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                    C::SourceFilterSingleValue,
                    &render(spec, formula.text.meta),
                    &[("identifier", variable)],
                )));
            }
            (None, _) => {}
        }
        Ok(())
    }

    /// The findings binding reports for a window that the declarations
    /// decide, in binding's order: the grouping refusals, the authored
    /// defects, then each read of a dataset other than the rows' driver.
    /// Without a scope only the authored defects.
    fn window(
        &self,
        e: ExprId,
        window: &ast::Window,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let path = path(spec, e);
        let operands = window_operands(spec, window);
        let driver = |name: &str, scope: Scope<'_>| {
            name.split_once('.')
                .is_some_and(|(qualifier, _)| scope.drivers.contains(&qualifier))
        };
        // Under grouped construction each operand naming a field of the rows'
        // driver must be a group key (REQ-0067 in a row template, REQ-0107 at
        // column level). Binding decides it only for a field the driver
        // stores, and reports any other as unknown; without source schemas
        // every such field is stored.
        if let Some((scope, phase)) = scope.and_then(|scope| Some((scope, scope.phase?))) {
            for (name, path) in operands.iter().filter(|(name, _)| driver(name, scope)) {
                if let Some(finding) = crate::reference_scope::group_finding(name, phase) {
                    findings.push(BindFinding::QualifiedReference {
                        path: path.clone(),
                        name: (*name).into(),
                        row: scope.row.map(String::from),
                        finding,
                    });
                }
            }
        }
        let (operation, baseline) = match &window.function {
            WindowFunction::RowNumber => ("row_number", false),
            WindowFunction::Rank { .. } => ("rank", false),
            WindowFunction::RowValue { offset } => {
                if offset.parse::<i64>() == Ok(0) {
                    findings.push(BindFinding::Window(WindowFinding::ZeroOffset {
                        path: format!("{path}.offset"),
                    }));
                }
                ("row_value", false)
            }
            WindowFunction::Locf { .. } => ("locf", false),
            WindowFunction::BaselineFlag { .. } => ("baseline_flag", true),
        };
        if window_order(window).is_empty() != baseline {
            findings.push(BindFinding::Window(WindowFinding::Order {
                path: format!("{path}.window{}", if baseline { ".order_by" } else { "" }),
                operation: operation.into(),
                forbidden: baseline,
            }));
        }
        // A partition names each variable once (REQ-0293); report every repeat.
        for (index, &group) in window.group_by.iter().enumerate() {
            let named = name(spec, group).unwrap_or_default();
            if window.group_by[..index]
                .iter()
                .any(|&prior| name(spec, prior).unwrap_or_default() == named)
            {
                findings.push(BindFinding::Window(WindowFinding::RepeatedGroup {
                    path: super::path(spec, group),
                    identifier: named.into(),
                }));
            }
        }
        if let Some(filter) = window.filter {
            self.grammar(filter, findings)?;
        }
        // Binding's catalog holds one dataset, the rows' driver, so it refuses
        // a field of any other dataset whether or not that dataset stores it
        // (REQ-0103): the operands, then each filter name once, in sorted order.
        if let Some(scope) = scope {
            let filtered = window
                .filter
                .and_then(|filter| match &spec.expr(filter).kind {
                    ExprKind::Formula(ast::Formula {
                        compiled: Ok(plan),
                        text,
                        ..
                    }) => Some((plan, render(spec, text.meta))),
                    _ => None,
                })
                .into_iter()
                .flat_map(|(plan, path)| {
                    let names = plan.identifiers().collect::<BTreeSet<_>>();
                    names.into_iter().map(move |name| (name, path.clone()))
                });
            for (name, path) in operands.into_iter().chain(filtered) {
                if name.contains('.') && !driver(name, scope) {
                    findings.push(BindFinding::UnknownReference {
                        path,
                        name: name.into(),
                    });
                }
            }
        }
        Ok(())
    }

    /// A column window's baseline whose operands are all bare output columns
    /// has their declared types: its date is a date or datetime, and its
    /// reference date has the date's type (WINDOW-9). Binding decides every
    /// other read.
    fn baseline_types(&self, e: ExprId, window: &ast::Window, findings: &mut Vec<BindFinding>) {
        let spec = self.spec;
        let WindowFunction::BaselineFlag {
            window_order_by, ..
        } = &window.function
        else {
            return;
        };
        let output = |name: &str| spec.columns.iter().find(|column| column.name.name == name);
        let kind = |e: ExprId| name(spec, e).and_then(output).map(|column| column.kind);
        let operand = |slot| {
            window
                .operands
                .iter()
                .find(|operand| operand.slot == slot)
                .map(|operand| operand.expr)
        };
        let (Some(value), Some(reference), Some((date, tie_break))) = (
            operand(Slot::Source),
            operand(Slot::Reference),
            window.order_by.split_first(),
        ) else {
            return;
        };
        let groups = window
            .group_by
            .iter()
            .map(|&group| name(spec, group))
            .collect::<Vec<_>>();
        let filtered = window.filter.is_none_or(|filter| {
            matches!(&spec.expr(filter).kind, ExprKind::Formula(ast::Formula {
                compiled: Ok(plan),
                ..
            }) if plan.identifiers().all(|name| output(name).is_some()))
        });
        if !window_order_by.is_empty()
            || !filtered
            || kind(value).is_none()
            || tie_break.iter().any(|term| kind(term.variable).is_none())
            || window.group_by.iter().enumerate().any(|(index, &group)| {
                kind(group).is_none() || groups[..index].contains(&groups[index])
            })
        {
            return;
        }
        let (Some(date_kind), Some(reference_kind)) = (kind(date.variable), kind(reference)) else {
            return;
        };
        let (operand, source, expected, actual) =
            if !matches!(date_kind, ColumnType::Date | ColumnType::DateTime) {
                ("date", date.variable, "date or datetime", date_kind)
            } else if date_kind != reference_kind {
                (
                    "reference_date",
                    reference,
                    type_name(date_kind),
                    reference_kind,
                )
            } else {
                return;
            };
        let mut finding = LookupFinding::new(
            &format!("{}.{operand}", path(spec, e)),
            C::WindowBaselineInputType,
        );
        for (field, value) in [
            ("operation", "baseline_flag"),
            ("source", name(spec, source).unwrap_or_default()),
            ("expected", expected),
            ("actual", type_name(actual)),
        ] {
            finding.text(field, value);
        }
        findings.push(BindFinding::Scalar(finding.diagnostic()));
    }

    /// Static types over the declared types, as binding checks them: each
    /// flag condition and case predicate (FORMULA-5, FORMULA-12), and each
    /// aggregate derive binding's conversion (VALUE-24).
    fn predicate_types(
        &self,
        root: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let check = |e: ExprId, findings: &mut Vec<BindFinding>| {
            if let ExprKind::Formula(ast::Formula {
                compiled: Ok(plan), ..
            }) = &spec.expr(e).kind
            {
                findings.extend(
                    types::predicate_kinds(plan, |name| self.declared.name(name).single())
                        .into_iter()
                        .map(BindFinding::Predicate),
                );
            }
        };
        match &spec.expr(root).kind {
            ExprKind::Flag { condition, .. } => check(*condition, findings),
            ExprKind::Case(arms) => {
                for arm in arms {
                    if let Some(condition) = arm.condition {
                        check(condition, findings);
                    }
                    self.predicate_types(arm.result, findings)?;
                }
            }
            ExprKind::Aggregate(aggregate) => {
                let env = self.prepared.tree.env(&self.declared);
                let table = ast::Table::new(spec, root, ast::Scope::Row, &env);
                for binding in &aggregate.bindings {
                    let (Some(declared), None) =
                        (binding.declared, &binding.derivation.unconvertible)
                    else {
                        continue;
                    };
                    findings.extend(
                        types::conversion_finding(
                            &render(spec, binding.meta),
                            table.of(binding.derivation.expr),
                            declared,
                        )
                        .map(BindFinding::Scalar),
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The output columns a derivation reads, as binding records them for
    /// column order: each output column it reads, directly or among the
    /// output rows an unqualified aggregate reduces, and the columns each
    /// named intermediate it reads matches on, unless the intermediate drives
    /// the rows. A window reads output columns alone. A field another input
    /// supplies through an implicit join also depends on the output keys that
    /// input stores, which only its captured schema decides.
    fn edges(&self, root: ExprId, driver: &str) -> Vec<usize> {
        let spec = self.spec;
        let output = |name: &str| {
            spec.columns
                .iter()
                .position(|column| column.name.name == name)
        };
        let windowed = positions(spec, root)
            .into_iter()
            .filter(|&e| matches!(spec.expr(e).kind, ExprKind::Window(_)))
            .flat_map(|window| spec.reads(window, ast::Scope::Row))
            .map(|read| read.at)
            .collect::<BTreeSet<_>>();
        let mut edges = Vec::new();
        for read in spec.reads(root, ast::Scope::Row) {
            let name = read.name.as_str();
            let lookup = read.name.owner().is_some_and(|owner| {
                owner != driver && self.prepared.intermediates.contains(owner)
            });
            match read.scope {
                ast::Scope::Row if windowed.contains(&read.at) => edges.extend(output(name)),
                ast::Scope::Relation(owner) => match &spec.expr(owner).kind {
                    ExprKind::Aggregate(aggregate) if reduces_output(spec, aggregate) => {
                        edges.extend(output(name))
                    }
                    ExprKind::Aggregate(_) if lookup => edges.extend(self.matched(name)),
                    _ => {}
                },
                ast::Scope::Row if lookup => edges.extend(self.matched(name)),
                ast::Scope::Row => edges.extend(output(name)),
                _ => {}
            }
        }
        edges
    }

    /// The output columns the named intermediate a name reads matches on,
    /// sorted: those its key and range value read, then those of each
    /// intermediate its range value or derivations read. An inferred key's
    /// columns depend on the fields its input stores, which only capture
    /// decides.
    fn matched(&self, name: &str) -> Vec<usize> {
        let spec = self.spec;
        let output = |name: &str| {
            spec.columns
                .iter()
                .position(|column| column.name.name == name)
        };
        let mut columns = Vec::new();
        let mut followed = Vec::new();
        let mut pending = name
            .split_once('.')
            .map(|(owner, _)| owner)
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(owner) = pending.pop() {
            let Some(item) = spec.intermediates.iter().find(|item| item.id.name == owner) else {
                continue;
            };
            if followed.contains(&owner) {
                continue;
            }
            followed.push(owner);
            let mut reads = match &item.key {
                Some(JoinKey::Columns { columns, .. }) => {
                    columns.iter().map(|column| column.name.as_str()).collect()
                }
                Some(JoinKey::Pairs { pairs, .. }) => pairs
                    .iter()
                    .flat_map(|&(_, value)| spec.reads(value, ast::Scope::Row))
                    .filter_map(|read| super::name(spec, read.at))
                    .collect(),
                _ => Vec::new(),
            };
            reads.extend(
                item.between
                    .as_ref()
                    .and_then(|between| super::name(spec, between.value)),
            );
            for read in reads {
                match read.split_once('.') {
                    None => columns.extend(output(read)),
                    Some((other, _)) => pending.push(other),
                }
            }
            for binding in &item.bindings {
                for read in spec.reads(binding.derivation.expr, ast::Scope::Row) {
                    pending.extend(read.name.owner().and_then(|other| {
                        spec.intermediates
                            .iter()
                            .find(|item| item.id.name == other)
                            .map(|item| item.id.name.as_str())
                    }));
                }
            }
        }
        columns.sort_unstable();
        columns.dedup();
        columns
    }

    /// Column order over each derivation's reads (REQ-0071, REQ-0072,
    /// REQ-0074), named with binding's columns and paths.
    fn order(
        &self,
        dependencies: &[Option<Vec<usize>>],
        paths: Vec<String>,
        rows: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let analysis = column_dependencies::analyze(dependencies, &self.prepared.keys, rows)
            .map_err(BindError::DependencyPolicy)?;
        if !analysis.diagnostics.is_empty() {
            findings.push(BindFinding::Dependencies {
                columns: self.names(),
                paths,
                diagnostics: analysis.diagnostics,
            });
        }
        Ok(())
    }

    /// The output columns' names.
    fn names(&self) -> Vec<String> {
        self.spec
            .columns
            .iter()
            .map(|column| column.name.name.clone())
            .collect()
    }

    /// The result type of a derivation over the declared types.
    fn result_type(&self, root: ExprId, scope: ast::Scope, names: &dyn ast::TypeEnv) -> ResultType {
        ast::result_type(self.spec, root, scope, &self.prepared.tree.env(names))
    }

    /// Each intermediate's donor record in the order binding derives them,
    /// with each derivation's result type. `stored` types a field of an
    /// input's record and `current` a field of the record a lookup reads
    /// from; an earlier intermediate's fields have the types found for it.
    fn donors(
        &self,
        stored: &Typing<'_>,
        current: &Typing<'_>,
    ) -> Result<Vec<Donor<'_>>, BindError> {
        let spec = self.spec;
        let mut known = BTreeMap::<String, ResultType>::new();
        let mut inputs = BTreeMap::<&str, &str>::new();
        let mut donors = Vec::new();
        for (identifier, _, _) in self.prepared.intermediates.audit_entries() {
            let Some((index, item)) = spec
                .intermediates
                .iter()
                .enumerate()
                .find(|(_, item)| item.id.name == identifier)
            else {
                return Err(BindError::Internal);
            };
            let dataset = item.dataset.name.as_str();
            let mut donor = Donor {
                index,
                item,
                self_rows: dataset == "SELF" && has_rows(spec),
                results: Vec::new(),
                fields: BTreeMap::new(),
            };
            if donor.self_rows {
                donor.fields.extend(
                    spec.columns
                        .iter()
                        .map(|column| (column.name.name.clone(), ResultType::column(column.kind))),
                );
            }
            for binding in &item.bindings {
                let input = |name: &str| {
                    let (owner, field) = name.split_once('.').unwrap_or((dataset, name));
                    if owner == dataset {
                        donor.field(field, stored).unwrap_or(ResultType::UNKNOWN)
                    } else if let Some(kind) = known.get(name) {
                        *kind
                    } else {
                        match inputs.get(owner) {
                            Some(input) => stored(input, field),
                            None => current(owner, field),
                        }
                        .unwrap_or(ResultType::UNKNOWN)
                    }
                };
                let result =
                    self.result_type(binding.derivation.expr, ast::Scope::Donor(index), &input);
                let kind = binding.declared.map_or(result, ResultType::column);
                donor.results.push(result);
                donor.fields.insert(binding.name.name.clone(), kind);
            }
            for (field, &kind) in &donor.fields {
                known.insert(format!("{identifier}.{field}"), kind);
            }
            if !donor.self_rows {
                inputs.insert(identifier, dataset);
            }
            donors.push(donor);
        }
        Ok(donors)
    }

    /// The static type faults of an intermediate's filter over its donor
    /// record and the record it is read from (FORMULA-5).
    fn filter_types(
        &self,
        donor: &Donor<'_>,
        stored: &Typing<'_>,
        current: &Typing<'_>,
    ) -> Vec<Diagnostic> {
        let Some(ExprKind::Formula(ast::Formula {
            compiled: Ok(filter),
            ..
        })) = donor.item.filter.map(|filter| &self.spec.expr(filter).kind)
        else {
            return Vec::new();
        };
        let dataset = donor.item.dataset.name.as_str();
        types::predicate_kinds(filter, |name| {
            let (owner, field) = name
                .split_once('.')
                .or_else(|| (dataset == "SELF").then_some((dataset, name)))?;
            if owner == dataset {
                donor.field(field, stored)?.single()
            } else {
                current(owner, field)?.single()
            }
        })
    }

    /// Each intermediate in the order it is derived: its filter's grammar,
    /// each derivation's defects, selections and conversion (VALUE-24), its
    /// filter's operand types, its key types and its range's types, over the
    /// declared types of its donor and of earlier intermediates.
    fn intermediates(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let spec = self.spec;
        let declared = &self.declared;
        // A stored field's declared type: every field of a delimited input is
        // `str` unless `types` or its producer says otherwise (INPUT-5).
        let stored = |owner: &str, field: &str| {
            Some(declared.name(&format!("{owner}.{field}")))
                .filter(|&kind| kind != ResultType::UNKNOWN)
        };
        // A name of the current record, which a lookup reads wherever it is
        // read: the record of another intermediate over the same input holds
        // that intermediate's derived fields too (LOOKUP-15), so a name one
        // derives is known only where the lookup is bound.
        let current = |owner: &str, field: &str| {
            let derived = spec.intermediates.iter().any(|item| {
                item.dataset.name == owner
                    && item
                        .bindings
                        .iter()
                        .any(|binding| binding.name.name == field)
            });
            (!derived).then(|| stored(owner, field)).flatten()
        };
        let current_names = |name: &str| match name.split_once('.') {
            Some((owner, field)) => current(owner, field).unwrap_or(ResultType::UNKNOWN),
            None => declared.name(name),
        };
        let donors = self.donors(&stored, &current)?;
        for (position, donor) in donors.iter().enumerate() {
            let (index, item) = (donor.index, donor.item);
            let identifier = item.id.name.as_str();
            if let Some(filter) = item.filter {
                self.grammar(filter, findings)?;
            }
            for (binding, &result) in item.bindings.iter().zip(&donor.results) {
                self.derivation(binding.derivation.expr, None, false, findings)?;
                if let (Some(typed), None) = (binding.declared, &binding.derivation.unconvertible) {
                    findings.extend(
                        types::conversion_finding(&render(spec, binding.meta), result, typed)
                            .map(BindFinding::Scalar),
                    );
                }
            }
            findings.extend(
                self.filter_types(donor, &stored, &current)
                    .into_iter()
                    .map(BindFinding::Predicate),
            );
            // A key compares each donor column with its current-row value: a
            // listed column with the same-named value, a written name with
            // that name, and any other value by its result type (LOOKUP-6).
            let keys = match &item.key {
                Some(JoinKey::Columns { columns, .. }) => {
                    columns.iter().map(|column| (column, None)).collect()
                }
                Some(JoinKey::Pairs { pairs, .. }) => pairs
                    .iter()
                    .map(|(column, value)| (column, Some(*value)))
                    .collect(),
                _ => Vec::new(),
            };
            for (column, value) in keys {
                let field = column.name.as_str();
                let Some(actual) = donor
                    .field(field, &stored)
                    .and_then(ResultType::column_type)
                else {
                    continue;
                };
                let (expected, display) = match value.map(|value| (value, name(spec, value))) {
                    None => (current_names(field), field.to_owned()),
                    Some((value, Some(read))) if spec.expr(value).meta.path == column.meta.path => {
                        (current_names(read), read.to_owned())
                    }
                    Some((value, read)) => (
                        read.map_or_else(
                            || self.result_type(value, ast::Scope::Row, &current_names),
                            current_names,
                        ),
                        format!("key[{field}]"),
                    ),
                };
                if !expected.comparable(actual.into()) {
                    findings.push(BindFinding::Lookup(LookupFinding::key_type(
                        &format!("intermediates[{index}].key"),
                        identifier,
                        &display,
                        expected,
                        actual,
                    )));
                }
            }
            // A range's value is a value of the reading row: an output
            // column, else a field of the primary input's record, or a field
            // of an earlier intermediate or of an input. Its bounds are donor
            // fields; each stated one must be typed (LOOKUP-9).
            if let Some(between) = &item.between {
                let primary = self.prepared.source().name.as_str();
                let earlier = &donors[..position];
                let value =
                    name(spec, between.value).and_then(|value| match value.split_once('.') {
                        None if spec.columns.iter().any(|column| column.name.name == value) => {
                            Some(declared.name(value))
                        }
                        None => stored(primary, value),
                        Some((owner, field)) => {
                            match earlier.iter().find(|prior| prior.item.id.name == owner) {
                                Some(prior) => prior.field(field, &stored),
                                None => stored(owner, field),
                            }
                        }
                    });
                let bound = |bound: Option<ExprId>| match bound {
                    None => Some(None),
                    Some(bound) => donor.field(name(spec, bound)?, &stored)?.single().map(Some),
                };
                if let (Some(value), Some(lower), Some(upper)) = (
                    value.and_then(ResultType::single),
                    bound(between.lower),
                    bound(between.upper),
                ) {
                    if let Some(types) =
                        crate::dataset::RelationRangeTypes::incomparable(value, lower, upper)
                    {
                        findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                            types
                                .diagnostic(&format!("intermediates[{index}].between"), identifier),
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// The type faults of each intermediate's filter that written
    /// declarations decide: a field no declaration types is not yet known,
    /// as before its input is read.
    fn written_filter_types(&self) -> Result<Vec<Diagnostic>, BindError> {
        let written = |owner: &str, field: &str| {
            Some(self.declared.written(&format!("{owner}.{field}")))
                .filter(|&kind| kind != ResultType::UNKNOWN)
        };
        let mut findings = Vec::new();
        for donor in self.donors(&written, &written)? {
            findings.extend(self.filter_types(&donor, &written, &written));
        }
        Ok(findings)
    }

    /// Without row templates, each column in declaration order: a key that
    /// reads other records, the derivation's defects and selections, its
    /// formula's qualified names (REQ-0442), its static types and its
    /// conversion (VALUE-24); then column order over what every derivation
    /// reads (REQ-0071, REQ-0072, REQ-0074).
    fn base(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let spec = self.spec;
        let prepared = self.prepared;
        let driver = [prepared.source().name.as_str()];
        let scope = Scope {
            drivers: &driver,
            row: None,
            phase: None,
        };
        let mut dependencies = Vec::new();
        let mut paths = Vec::new();
        for (column, declared) in spec.columns.iter().enumerate() {
            let Some(derivation) = &declared.derivation else {
                return Err(BindError::Internal);
            };
            let root = derivation.expr;
            paths.push(path(spec, root));
            let start = findings.len();
            // Binding reports a key that reads other records first and binds
            // nothing else of it (COLUMN-7).
            let refused = key_record(prepared, column);
            if !refused.is_empty() {
                findings.extend(refused);
                dependencies.push(Some(Vec::new()));
                continue;
            }
            self.derivation(root, Some(scope), true, findings)?;
            if let ExprKind::Formula(ast::Formula {
                context: ExpressionContext::Compute,
                compiled: Ok(compiled),
                text,
                ..
            }) = &spec.expr(root).kind
            {
                qualified_numeric_findings(
                    compiled,
                    &render(spec, text.meta),
                    &prepared.intermediates,
                    findings,
                );
            }
            dependencies.push(Some(self.edges(root, driver[0])));
            self.predicate_types(root, findings)?;
            if findings.len() == start && derivation.unconvertible.is_none() {
                let result = self.result_type(root, ast::Scope::Row, &self.declared);
                findings.extend(
                    prepared
                        .conversion_finding(column, result)
                        .map(BindFinding::Scalar),
                );
            }
        }
        self.order(&dependencies, paths, false, findings)
    }

    /// With row templates: each column-phase derivation's defects and
    /// selections, then each template's filter and row-phase derivations',
    /// each scoped to the rows it evaluates on; then the static types and
    /// conversions in binding's order, each template's derivations first;
    /// then each template's cycles and the column phase's order.
    fn rows(&self, rows: &rows::Rows, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let spec = self.spec;
        let primary = self.prepared.source().name.as_str();
        let phases = rows.phases();
        let columns = rows.column_phase();
        let groups = phases
            .iter()
            .filter_map(|template| Some((template.driver, template.groups.clone()?)))
            .collect::<Vec<_>>();
        let column_groups = groups
            .iter()
            .map(|(driver, group)| (*driver, group.as_slice()))
            .collect::<Vec<_>>();
        let root = |derivation: &rows::PhaseDerivation| {
            self.lowered
                .expression(derivation.value)
                .ok_or(BindError::Internal)
        };
        for derivation in &columns {
            let root = root(derivation)?;
            let drivers =
                rows.column_drivers_of(derivation.column, lookups::aggregates(spec, root), primary);
            self.derivation(
                root,
                Some(Scope {
                    drivers: &drivers,
                    row: None,
                    phase: Some(Phase::Column {
                        groups: &column_groups,
                    }),
                }),
                false,
                findings,
            )?;
        }
        for (index, template) in phases.iter().enumerate() {
            if let Some(filter) = spec
                .rows
                .templates
                .get(index)
                .and_then(|declared| declared.filter)
            {
                self.grammar(filter, findings)?;
            }
            let driver = [template.driver];
            for derivation in &template.derivations {
                let root = root(derivation)?;
                // A phase boundary stops binding before the selections.
                self.defects(root, findings)?;
                if !super::boundaries::derivation(spec, template, root, findings) {
                    let scope = Scope {
                        drivers: &driver,
                        row: Some(template.id),
                        phase: Some(Phase::Row {
                            group_by: template.groups.as_deref(),
                        }),
                    };
                    self.selections(root, Some(scope), false, findings)?;
                }
            }
        }
        for derivation in phases
            .iter()
            .flat_map(|template| &template.derivations)
            .chain(&columns)
        {
            let start = findings.len();
            let root = root(derivation)?;
            self.predicate_types(root, findings)?;
            if derivation.handled || findings.len() != start {
                continue;
            }
            let column = &self.prepared.output.columns()[derivation.column];
            let result = self.result_type(root, ast::Scope::Row, &self.declared);
            if let Some(finding) =
                types::conversion_finding(&format!("columns.{}", column.name), result, column.kind)
            {
                if !findings
                    .iter()
                    .any(|prior| matches!(prior, BindFinding::Scalar(prior) if *prior == finding))
                {
                    findings.push(BindFinding::Scalar(finding));
                }
            }
        }
        // Each template's row-phase derivations read its own row-phase
        // columns, which must not form a cycle; a cycle several templates
        // share is one finding.
        let width = spec.columns.len();
        for template in &phases {
            let mut graph = vec![Vec::new(); width];
            let mut paths = vec![String::new(); width];
            for derivation in &template.derivations {
                let root = root(derivation)?;
                graph[derivation.column] = self
                    .edges(root, template.driver)
                    .into_iter()
                    .filter(|&column| template.derivations.iter().any(|d| d.column == column))
                    .collect();
                paths[derivation.column] = path(spec, root);
            }
            let analysis =
                crate::dependency_analysis::analyze(&graph).map_err(|_| BindError::Internal)?;
            let Some(cycle) = analysis.cycle else {
                continue;
            };
            let repeated = findings.iter().any(|prior| {
                matches!(prior, BindFinding::Dependencies { paths: prior, diagnostics, .. }
                if diagnostics.iter().any(|diagnostic| matches!(
                    diagnostic,
                    column_dependencies::Diagnostic::Cycle { columns } if *columns == cycle
                        && cycle.iter().all(|&column| prior.get(column) == paths.get(column))
                )))
            });
            if !repeated {
                findings.push(BindFinding::Dependencies {
                    columns: self.names(),
                    paths,
                    diagnostics: vec![column_dependencies::Diagnostic::Cycle { columns: cycle }],
                });
            }
        }
        // The column phase follows declaration order; a completed row-phase
        // column is already available. Every driver binds the column for its
        // own rows, and a read several bindings make is one edge.
        let mut dependencies = vec![None; width];
        let mut paths = vec![String::new(); width];
        for derivation in &columns {
            let root = root(derivation)?;
            let mut edges = Vec::new();
            for driver in
                rows.column_drivers_of(derivation.column, lookups::aggregates(spec, root), primary)
            {
                for edge in self.edges(root, driver) {
                    if !edges.contains(&edge) {
                        edges.push(edge);
                    }
                }
            }
            dependencies[derivation.column] = Some(edges);
            paths[derivation.column] = path(spec, root);
        }
        self.order(&dependencies, paths, true, findings)
    }

    /// LOOKUP-9 from the declarations alone: a range value qualified by an
    /// input that does not drive every row reading the lookup fails, whether
    /// or not the input stores the field. Each context pairs what its rows
    /// evaluate with the inputs driving them, in binding's order; each
    /// refused range is reported once.
    fn ranges(&self, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let named = &self.prepared.intermediates;
        let inputs = &self.prepared.sources;
        let refused = |value: &str, drivers: &[&str]| {
            value.split_once('.').is_some_and(|(qualifier, _)| {
                inputs.iter().any(|input| input.name == qualifier)
                    && drivers.iter().any(|&driver| driver != qualifier)
            })
        };
        let mut seen = BTreeSet::new();
        for (evaluated, drivers) in self.range_contexts()? {
            for evaluated in evaluated {
                let (ranges, names) = match evaluated {
                    Evaluated::Expression(root, own) => self.lookups(root, own),
                    Evaluated::Name(name) => (Vec::new(), vec![name]),
                };
                for (value, path, owner) in ranges {
                    if refused(value, &drivers) && seen.insert(path.clone()) {
                        findings.push(BindFinding::Scalar(lookup::range_reference(
                            &format!("{path}.value"),
                            owner,
                            value,
                        )));
                    }
                }
                let mut stack = names
                    .into_iter()
                    .filter_map(|name| named.reference(name).map(|(index, _)| index))
                    .collect::<Vec<_>>();
                let mut visited = BTreeSet::new();
                while let Some(index) = stack.pop() {
                    if !visited.insert(index) {
                        continue;
                    }
                    let Some((identifier, range)) = named.range(index) else {
                        continue;
                    };
                    if refused(&range.value, &drivers) && seen.insert(range.path().to_owned()) {
                        findings.push(BindFinding::Scalar(
                            range.unknown_value(Owner::Intermediate(identifier)),
                        ));
                    }
                    if let Some((chained, _)) = named.reference(&range.value) {
                        stack.push(chained);
                    }
                }
            }
        }
        Ok(())
    }

    /// The contexts whose rows read lookup ranges. Without row templates the
    /// base drives every row, and its qualifier reads the row's own record. A
    /// dataset verification reads its lookups for every row, as a column
    /// does. With row templates, the column phase, whose derivations and
    /// dataset verifications evaluate for the rows of every template that
    /// leaves them to it, then each template, whose derivations and filter
    /// evaluate for its rows only; a row's driver is the input behind its
    /// template, named directly or through a named intermediate.
    #[allow(clippy::type_complexity)]
    fn range_contexts(&self) -> Result<Vec<(Vec<Evaluated<'_>>, Vec<&str>)>, BindError> {
        let spec = self.spec;
        let prepared = self.prepared;
        let verified = relation_predicates::verification_predicates(&prepared.verifications.checks)
            .flat_map(crate::bound_expression::BoundPredicate::bindings)
            .filter(|binding| {
                matches!(
                    binding.read,
                    crate::bound_expression::Read::Intermediate { .. }
                )
            })
            .map(|binding| Evaluated::Name(&binding.name));
        let Some(rows) = &prepared.rows else {
            let base = prepared.source().name.as_str();
            let evaluated = spec
                .columns
                .iter()
                .filter_map(|column| column.derivation.as_ref())
                .map(|derivation| Evaluated::Expression(derivation.expr, Some(base)))
                .chain(verified)
                .collect();
            return Ok(vec![(evaluated, vec![base])]);
        };
        let phases = rows.phases();
        let Some(drivers) = phases
            .iter()
            .map(|template| {
                if prepared
                    .sources
                    .iter()
                    .any(|source| source.name == template.driver)
                {
                    Some(template.driver)
                } else {
                    prepared
                        .intermediates
                        .input(template.driver)
                        .and_then(|index| prepared.sources.get(index))
                        .map(|source| source.name.as_str())
                }
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(Vec::new());
        };
        let root = |value: usize| self.lowered.expression(value).ok_or(BindError::Internal);
        let mut contexts = Vec::new();
        // A column derivation evaluates only for the rows of the templates that
        // leave the column to this phase (LOOKUP-9), and every driver binds it
        // for its own rows. A column holding an aggregate binds once against
        // the whole dataset, where every input joins unless every row template
        // is driven by the input behind the first.
        for derivation in rows.column_phase() {
            let root = root(derivation.value)?;
            let leaving = phases.iter().zip(&drivers).filter(|(template, _)| {
                !template
                    .derivations
                    .iter()
                    .any(|derived| derived.column == derivation.column)
            });
            let mut scopes: Vec<(Option<&str>, Vec<&str>)> = Vec::new();
            if lookups::aggregates(spec, root) {
                let own = drivers
                    .first()
                    .copied()
                    .filter(|&primary| phases.iter().all(|template| template.driver == primary));
                scopes.push((own, leaving.map(|(_, &driver)| driver).collect()));
            } else {
                for (template, &driver) in leaving {
                    let own = Some(template.driver);
                    match scopes.iter_mut().find(|(known, _)| *known == own) {
                        Some((_, inputs)) => inputs.push(driver),
                        None => scopes.push((own, vec![driver])),
                    }
                }
            }
            for (own, inputs) in scopes {
                contexts.push((vec![Evaluated::Expression(root, own)], inputs));
            }
        }
        contexts.push((verified.collect(), drivers.clone()));
        for ((index, template), &driver) in phases.iter().enumerate().zip(&drivers) {
            let mut evaluated = Vec::new();
            for derivation in &template.derivations {
                evaluated.push(Evaluated::Expression(
                    root(derivation.value)?,
                    Some(template.driver),
                ));
            }
            if let Some(ExprKind::Formula(ast::Formula {
                compiled: Ok(filter),
                ..
            })) = spec
                .rows
                .templates
                .get(index)
                .and_then(|declared| declared.filter)
                .map(|filter| &spec.expr(filter).kind)
            {
                evaluated.extend(filter.identifiers().map(Evaluated::Name));
            }
            contexts.push((evaluated, vec![driver]));
        }
        Ok(contexts)
    }

    /// What a row reads in one derivation that a lookup range can refuse:
    /// the range of each implicit join and qualified aggregate the row
    /// evaluates itself, with its owner, and every name it reads, which may
    /// read a named intermediate's range. A derive binding evaluates for the
    /// aggregate's records, so its ranges are not the row's. `own` is the
    /// dataset whose qualifier reads the row's own record; with `None` every
    /// input joins. Binding reports a declaration defect, or `between` on a
    /// read other than an implicit join, before it reads the value.
    #[allow(clippy::type_complexity)]
    fn lookups<'s>(
        &'s self,
        root: ExprId,
        own: Option<&str>,
    ) -> (Vec<(&'s str, String, Owner<'s>)>, Vec<&'s str>) {
        let spec = self.spec;
        let bounded = |between: &ast::Between| between.lower.is_some() || between.upper.is_some();
        let mut ranges = Vec::new();
        let mut names = Vec::new();
        spec.walk(root, ast::Scope::Row, |e, scope| {
            let range = match &spec.expr(e).kind {
                ExprKind::Ref(name) => {
                    names.push(name.as_str());
                    None
                }
                ExprKind::Source(source) if scope == ast::Scope::Row => {
                    let variable = name(spec, source.variable).unwrap_or_default();
                    let join = variable.split_once('.').is_some_and(|(qualifier, _)| {
                        own != Some(qualifier)
                            && self
                                .prepared
                                .sources
                                .iter()
                                .any(|input| input.name == qualifier)
                    });
                    source
                        .between
                        .as_ref()
                        .filter(|between| {
                            join && bounded(between)
                                && source.order_by.is_empty() == source.keep.is_none()
                                && !(source.require_match && source.absent.is_some())
                        })
                        .map(|between| (between, Owner::Source(variable)))
                }
                ExprKind::Aggregate(aggregate)
                    if scope == ast::Scope::Row && qualified(spec, aggregate) =>
                {
                    aggregate
                        .between
                        .as_ref()
                        .filter(|between| bounded(between))
                        .map(|between| (between, Owner::Unnamed))
                }
                _ => None,
            };
            if let Some((between, owner)) = range {
                if let Some(value) = name(spec, between.value) {
                    ranges.push((value, render(spec, between.meta), owner));
                }
            }
            ast::Step::Continue
        });
        (ranges, names)
    }
}

/// One thing a row evaluates, as declared: a derivation, with the dataset
/// whose qualifier reads the row's own record there (`None`: every input
/// joins), or a name read outside a derivation.
enum Evaluated<'a> {
    Expression(ExprId, Option<&'a str>),
    Name(&'a str),
}

/// A window's reads in authored order, each with the path that names it:
/// the operation's own operands, then each `window.group_by` and
/// `window.order_by` entry.
fn window_operands<'s>(spec: &'s Spec, window: &ast::Window) -> Vec<(&'s str, String)> {
    let read = |e: ExprId| (name(spec, e).unwrap_or_default(), path(spec, e));
    let operand = |slot: Slot| {
        window
            .operands
            .iter()
            .filter(move |operand| operand.slot == slot)
            .map(|operand| read(operand.expr))
    };
    let mut reads = Vec::new();
    match &window.function {
        WindowFunction::RowValue { .. } | WindowFunction::Locf { .. } => {
            reads.extend(operand(Slot::Source));
        }
        WindowFunction::BaselineFlag { .. } => {
            reads.extend(window.order_by.first().map(|term| read(term.variable)));
            reads.extend(operand(Slot::Reference));
            reads.extend(operand(Slot::Source));
            reads.extend(
                window
                    .order_by
                    .iter()
                    .skip(1)
                    .map(|term| read(term.variable)),
            );
        }
        WindowFunction::RowNumber | WindowFunction::Rank { .. } => {}
    }
    reads.extend(window.group_by.iter().map(|&group| read(group)));
    reads.extend(window_order(window).iter().map(|term| read(term.variable)));
    reads
}

/// A window's written `window.order_by`: for `baseline_flag`, which orders
/// by its date and tie-break, the terms it may not write.
fn window_order(window: &ast::Window) -> &[ast::OrderTerm] {
    match &window.function {
        WindowFunction::BaselineFlag {
            window_order_by, ..
        } => window_order_by,
        _ => &window.order_by,
    }
}

/// Whether a source binding's variable is a group key of the rows that read
/// it, which is already one value in both row and column phase.
fn group_key(variable: &str, driver: &str, phase: Phase<'_>) -> bool {
    let Some((dataset, _)) = variable.split_once('.') else {
        return false;
    };
    match phase {
        Phase::Row { group_by } => {
            dataset == driver && group_by.is_some_and(|group| group.contains(&variable))
        }
        Phase::Column { groups } => {
            let mut own = groups
                .iter()
                .filter(|&&(owner, _)| owner == dataset)
                .peekable();
            own.peek().is_some() && own.all(|(_, group)| group.contains(&variable))
        }
    }
}

fn temporal(op: &ScalarOp) -> bool {
    matches!(
        op,
        ScalarOp::Diff { .. }
            | ScalarOp::StudyDay
            | ScalarOp::ToEpochDay
            | ScalarOp::Shift { .. }
            | ScalarOp::DurationValue { .. }
            | ScalarOp::Dtc
            | ScalarOp::TimeOfDay
    )
}

/// A written substitute value, admitted by preparation.
fn value(literal: &ast::Literal) -> crate::value::Value {
    use crate::value::Value;
    match literal {
        ast::Literal::Bool(value) => Value::Bool(*value),
        ast::Literal::Int(text) => text.parse().map_or(Value::Missing, Value::Int),
        ast::Literal::Float(value) => Value::float(*value),
        ast::Literal::Text(text) => Value::Str(text.clone()),
        ast::Literal::Missing
        | ast::Literal::Date(_)
        | ast::Literal::DateTime(_)
        | ast::Literal::Unsupported => Value::Missing,
    }
}
