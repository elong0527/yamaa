//! The declaration findings `check()` reports: what binding finds that the
//! declarations decide (#2844 decision 3), over the tree and typed by
//! [`DeclaredTypes`]. `run()` reports the same findings from binding, in
//! binding's order, and this pass reports them in that order too: the
//! intermediates in the order they are derived, then each column (or each
//! row template's filter and derivations and then the column phase), column
//! order, the root filter of a spec without row templates, the lookup ranges
//! each row reads, and the scopes of the lookups and aggregate keys the rows
//! read ([`super::scopes`]). Every read of a lookup whose key types fail is
//! refused where it is read, as binding binds nothing of that lookup, and so
//! is every read of a field qualified by nothing declared; a lookup bound
//! without its derivations admits none of their reads for a row. An
//! aggregate's filter reads only what the aggregate reduces, and binding
//! binds nothing of a lookup whose filter reads a field qualified by nothing
//! declared (LOOKUP-7), which this reports where the declarations decide the
//! finding's suggestion (D220). Under
//! row templates each derivation is checked node by node in binding's order,
//! with what binding decides of each read ([`super::boundaries`]), and so is
//! each lookup's own derivation and each aggregate's derive binding, over the
//! donor's records ([`super::donors`]). A read of another 1.0 input that
//! declares no `key`, through its implicit join or by a qualified aggregate
//! without `key`, fails at the read in either construction, whatever the
//! input stores (D111), and so does one whose declared key column the read
//! cannot pair (D111 item 2, [`super::declared_key`]). Each formula's
//! operands are typed once its names bind, over the names that bound
//! (FORMULA-12), and a flag's condition once every name it reads binds.
//!
//! Binding also decides from the declarations alone whether a key reads
//! other records than its own ([`key_record`]); it calls the same check.
//!
//! Each node kind's checks are a module of their own: the intermediates
//! ([`intermediate`]) and the pair relations among them ([`pairs`]), an
//! aggregate ([`aggregate`]), the rows ([`template`]) and one derivation
//! node by node ([`derivation`]). This module holds what they share: the
//! state of one pass, its entry points and the checks several kinds call.
use super::*;
use crate::ast::{Slot, WindowFunction};
use crate::diagnostic::{
    lookup::{self, Owner},
    ConditionCode as C, Diagnostic,
};
use crate::expression_table::Verb;
use crate::reference_scope::Phase;
use crate::result_type::ResultType;
use crate::scalar_verb::{CheckTyping, Operation, Read};
use std::collections::BTreeSet;

mod aggregate;
mod derivation;
mod intermediate;
mod pairs;
mod template;

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
    /// The columns a row-phase derivation may read: those its template
    /// derives in the row phase. `None` reads any.
    available: Option<&'a [usize]>,
}

/// Which derivations and filters bound, as binding admits the reads of the
/// rows once they are bound ([`super::scopes`]).
enum Bound {
    /// Without row templates, whether each column's derivation bound.
    Implicit(Vec<bool>),
    Templates(TemplatesBound),
}

/// Under row templates: whether each column-phase derivation bound, and for
/// each row template the derivations that bound, in the order they
/// complete, and whether its filter bound; and each template's derivations
/// that bound in written order, a cycle's among them.
struct TemplatesBound {
    columns: Vec<bool>,
    templates: Vec<(Vec<ExprId>, bool)>,
    derived: Vec<Vec<ExprId>>,
}

struct Checks<'a> {
    spec: &'a Spec,
    prepared: &'a PreparedSpecification,
    declared: types::DeclaredTypes<'a>,
    /// The intermediates whose key types fail, which binding does not bind,
    /// once [`Self::intermediates`] has checked them.
    refused: BTreeSet<usize>,
    /// The intermediates binding binds without their derivations, once
    /// [`Self::intermediates`] has checked them, so it admits none of their
    /// reads for a row ([`super::scopes`]).
    underived: BTreeSet<usize>,
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
) -> Result<Vec<BindFinding>, BindError> {
    let mut checks = Checks {
        spec: prepared.tree.spec(),
        prepared,
        declared: prepared.declared_types(),
        refused: BTreeSet::new(),
        underived: BTreeSet::new(),
    };
    let mut findings = Vec::new();
    (checks.refused, checks.underived) = checks.intermediates(&mut findings)?;
    let bound = match &prepared.rows {
        rows::Rows::Implicit(_) => {
            let bound = checks.base(&mut findings)?;
            if let Some(filter) = checks.spec.rows.filter {
                checks.root_filter(filter, &mut findings)?;
            }
            Bound::Implicit(bound)
        }
        rows::Rows::Templates(rows) => Bound::Templates(checks.rows(rows, &mut findings)?),
    };
    checks.ranges(&bound, &mut findings)?;
    checks.scopes(&bound, &mut findings);
    Ok(findings)
}

/// The type faults of each intermediate's filter that written declarations
/// decide. When every input is missing, binding reads no field, and `run()`
/// reports these instead of the missing inputs (`specification_run`).
pub(in crate::specification) fn written_filter_types(
    prepared: &PreparedSpecification,
) -> Result<Vec<Diagnostic>, BindError> {
    Checks {
        spec: prepared.tree.spec(),
        prepared,
        declared: prepared.declared_types(),
        refused: BTreeSet::new(),
        underived: BTreeSet::new(),
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
    if matches!(prepared.rows, rows::Rows::Templates(_)) || !prepared.keys.contains(&column) {
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
                Some((Verb::Aggregate.name(), e, reduces_output(spec, aggregate)))
            }
            ExprKind::Odm(_) => Some((Verb::Odm.name(), e, false)),
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

/// FORMULA-9: a qualified formula name must name a declared record lookup.
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

/// A formula's operand types (FORMULA-12), which binding checks once the
/// formula's names bind, over the names that bound (`BindContext::numeric`):
/// `kind` types each name that bound as the declarations decide it, and a
/// name of no known type is not checked.
fn operand_types(
    compiled: &CompiledExpression,
    kind: impl Fn(&str) -> Option<crate::value::ValueType>,
    findings: &mut Vec<BindFinding>,
) {
    findings.extend(
        types::numeric_kinds(compiled, kind)
            .into_iter()
            .map(BindFinding::Scalar),
    );
}

/// Whether binding takes an operand of `operation` without typing it, as its
/// type authority, the reading scope's catalog, does not type a lookup's
/// field (`lookup_field`), which then binds (`BindContext::scalar`,
/// [`crate::scalar_verb::Typing::Catalog`]).
fn untyped(operation: &Operation, lookup_field: bool) -> bool {
    lookup_field && operation.typing() == crate::scalar_verb::Typing::Catalog
}

/// The declaration finding binding reports once it took every operand
/// (`taken`): an extraction's pattern or group (TEXT-6, TEXT-10).
fn late_refusal(operation: &Operation, taken: bool, findings: &mut Vec<BindFinding>) {
    if taken {
        findings.extend(operation.late_refusal().cloned().map(BindFinding::Scalar));
    }
}

/// A flag's or `case` predicate's static types (FORMULA-5, FORMULA-12),
/// with `kind` typing each name it reads.
fn condition_types(
    spec: &Spec,
    condition: ExprId,
    kind: impl Fn(&str) -> Option<crate::value::ValueType>,
    findings: &mut Vec<BindFinding>,
) {
    if let ExprKind::Formula(ast::Formula {
        compiled: Ok(plan), ..
    }) = &spec.expr(condition).kind
    {
        findings.extend(
            types::predicate_kinds(plan, kind)
                .into_iter()
                .map(BindFinding::Predicate),
        );
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
pub(super) fn qualified(spec: &Spec, aggregate: &ast::Aggregate) -> bool {
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
    /// a flag's, a verb's defect and the written operands it does not take
    /// (a `str_pad` width, a fractional literal an integer operand receives),
    /// and a formula's grammar.
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
            ExprKind::Scalar { .. } => {
                let operation = crate::scalar_verb::Operation::from_tree(spec, root)
                    .map_err(|_| BindError::Internal)?;
                findings.extend(operation.defect().cloned().map(BindFinding::Scalar));
                findings.extend(
                    operation
                        .literal_findings()
                        .into_iter()
                        .map(BindFinding::Scalar),
                );
            }
            // A row formula: derivation text, which binding compiles
            // (FORMULA-27).
            ExprKind::Formula(formula) if formula.context == ExpressionContext::Derivation => {
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

    /// Whether `name` reads a lookup whose key types fail, which binding
    /// does not bind.
    fn refused_lookup(&self, name: &str) -> bool {
        name.split_once('.').is_some_and(|(owner, _)| {
            self.spec
                .intermediates
                .iter()
                .position(|item| item.id.name == owner)
                .is_some_and(|index| self.refused.contains(&index))
        })
    }

    /// Whether `owner` qualifies nothing declared ([`undeclared`]).
    fn undeclared(&self, owner: &str) -> bool {
        undeclared(self.prepared, owner)
    }

    /// The declared type of the column a bare name reads, which types a
    /// formula operand of the implicit template: there a formula binds no
    /// stored field, and a lookup's field has the type its donor's capture
    /// gives it.
    fn column_type(&self, name: &str) -> Option<crate::value::ValueType> {
        (!name.contains('.'))
            .then(|| self.declared_type(name))
            .flatten()
    }

    /// The declared type of a column or a stored field a name reads, when
    /// the declarations give one ([`types::DeclaredTypes`]).
    fn declared_type(&self, name: &str) -> Option<crate::value::ValueType> {
        self.declared.name(name).single()
    }

    /// A verb refuses its operand `index`, which reads `name`, of a declared
    /// type it does not accept (EXPR-11, EXPR-13, TEXT-1): a bare output
    /// column's or a stored field's declared type. A lookup's field is
    /// checked against the values it reads. Whether the verb takes the
    /// declared type; `None` when no declaration types the name.
    fn operand_type(
        &self,
        operation: &Operation,
        index: usize,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) -> Option<bool> {
        let kind = name
            .split_once('.')
            .is_none_or(|(owner, _)| !self.prepared.intermediates.contains(owner))
            .then(|| self.declared.name(name).column_type())
            .flatten()?;
        let refusal = operation.check(index, kind);
        let taken = refusal.is_none();
        findings.extend(refusal.map(BindFinding::Scalar));
        Some(taken)
    }

    /// The result type of a derivation over the declared types.
    fn result_type(&self, root: ExprId, scope: ast::Scope, names: &dyn ast::TypeEnv) -> ResultType {
        ast::result_type(self.spec, root, scope, &self.prepared.tree.env(names))
    }

    /// Whether `name` is a declared column, the output column a bare name
    /// reads (NAME-3).
    fn column(&self, name: &str) -> bool {
        self.spec
            .columns
            .iter()
            .any(|column| column.name.name == name)
    }

    fn unknown_match_value(&self, name: &str) -> bool {
        unknown_match_value(self.spec, self.prepared, name)
    }

    /// Under row templates, what binding finds of the verb a key's match
    /// value at `value` holds, once the value's names bind
    /// (`match_values::Declaration::bind`), as in a derivation: each
    /// operand's declared type where `check()` leaves it to binding, then
    /// the verb's late refusal (TEXT-1, TEXT-6, TEXT-10). Without row
    /// templates preflight types it.
    fn key_verb(&self, value: ExprId, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let spec = self.spec;
        let ExprKind::Scalar { op, .. } = &spec.expr(value).kind else {
            return Ok(());
        };
        if !has_rows(spec) || op.check_typing() != CheckTyping::LeftToBinding {
            return Ok(());
        }
        let operation = crate::scalar_verb::Operation::from_tree(spec, value)
            .map_err(|_| BindError::Internal)?;
        let mut taken = true;
        for (index, operand) in operation.operands().iter().enumerate() {
            taken &= match &operand.read {
                Read::Name(read) => self
                    .operand_type(&operation, index, read, findings)
                    .unwrap_or(false),
                Read::Source(_) => false,
                Read::Literal(_) => true,
            };
        }
        late_refusal(&operation, taken, findings);
        Ok(())
    }
}

/// The names a compiled text reads outside any reduction, each once, with
/// the path binding cites each at.
fn names(spec: &Spec, text: ExprId) -> Vec<(String, &str)> {
    match &spec.expr(text).kind {
        ExprKind::Formula(formula) if formula.compiled.is_ok() => formula
            .names
            .iter()
            .filter(|&&(role, _)| role == ast::Role::Operand)
            .filter_map(|&(_, read)| Some((path(spec, read), name(spec, read)?)))
            .collect(),
        _ => Vec::new(),
    }
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

/// A window's own order, written `window.order_by`: `baseline_flag` orders by
/// its date and tie-break terms alone, so it has none (WINDOW-5).
fn window_order(window: &ast::Window) -> &[ast::OrderTerm] {
    match &window.function {
        WindowFunction::BaselineFlag { .. } => &[],
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

/// The range value of the implicit join a structured source at `e` reads,
/// when the value is a bare name that no column takes: a bare name is an
/// output column alone (NAME-3), so it names nothing the row holds and
/// binding refuses it at `between.value` (LOOKUP-9), naming the variable.
pub(super) fn join_range(spec: &Spec, e: ExprId) -> Option<BindFinding> {
    let ExprKind::Source(source) = &spec.expr(e).kind else {
        return None;
    };
    let between = source
        .between
        .as_ref()
        .filter(|between| between.lower.is_some() || between.upper.is_some())?;
    let value = name(spec, between.value)
        .filter(|value| !value.contains('.'))
        .filter(|value| !spec.columns.iter().any(|column| column.name.name == *value))?;
    Some(unknown_range_value(
        spec,
        between,
        Owner::Source(name(spec, source.variable).unwrap_or_default()),
        value,
    ))
}

/// The range at `between` of a lookup of `relation`, owned by `owner`, whose
/// value and stated bounds the declarations type and whose types cannot
/// compare (`incomparable_range_types`, LOOKUP-9), as binding
/// reports it once the range's fields bind: the value an output column or a
/// field of an input's record, each bound a field of the relation. A value
/// read from a lookup is typed only once its relation is captured.
pub(super) fn range_types(
    spec: &Spec,
    prepared: &PreparedSpecification,
    between: &ast::Between,
    relation: &str,
    owner: Owner<'_>,
) -> Option<BindFinding> {
    let declared = prepared.declared_types();
    let value = name(spec, between.value)?;
    let typed = match value.split_once('.') {
        None => spec.columns.iter().any(|column| column.name.name == value),
        Some((owner, _)) => reads_as_input(prepared, owner),
    };
    let value_type = typed.then(|| declared.name(value).single()).flatten()?;
    let bound = |bound: Option<ExprId>| match bound {
        None => Some(None),
        Some(bound) => declared
            .name(&format!("{relation}.{}", name(spec, bound)?))
            .single()
            .map(Some),
    };
    let types = crate::dataset::RelationRangeTypes::incomparable(
        value_type,
        bound(between.lower)?,
        bound(between.upper)?,
    )?;
    let mut diagnostic = types.diagnostic_for(&render(spec, between.meta), None);
    owner.add(&mut diagnostic.context);
    Some(BindFinding::Scalar(diagnostic))
}

/// What binding refuses of the range of the implicit join a structured
/// source at `e` reads, once its read binds: a bare value that no
/// column takes ([`join_range`]), or else types that cannot compare
/// ([`range_types`]).
pub(super) fn join_range_findings(
    spec: &Spec,
    prepared: &PreparedSpecification,
    e: ExprId,
) -> Option<BindFinding> {
    join_range(spec, e).or_else(|| {
        let ExprKind::Source(source) = &spec.expr(e).kind else {
            return None;
        };
        let variable = name(spec, source.variable)?;
        let (relation, _) = variable.split_once('.')?;
        let between = source
            .between
            .as_ref()
            .filter(|between| between.lower.is_some() || between.upper.is_some())?;
        range_types(spec, prepared, between, relation, Owner::Source(variable))
    })
}

/// `require_match` asserts what only an implicit join can miss, and
/// `between` narrows what only an implicit join matches: on a structured
/// source that reads a lookup, the rows' driver or a column, binding refuses
/// each once that read binds, `require_match` first (LOOKUP-24, LOOKUP-9).
pub(super) fn join_options(spec: &Spec, e: ExprId, findings: &mut Vec<BindFinding>) {
    let ExprKind::Source(source) = &spec.expr(e).kind else {
        return;
    };
    let at = path(spec, e);
    let variable = name(spec, source.variable).unwrap_or_default();
    for (refused, code, field) in [
        (
            source.require_match,
            C::SourceRequireMatchScope,
            "require_match",
        ),
        (source.between.is_some(), C::SourceRangeScope, "between"),
    ] {
        if refused {
            findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                code,
                &format!("{at}.{field}"),
                &[("identifier", variable), ("construct", field)],
            )));
        }
    }
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
