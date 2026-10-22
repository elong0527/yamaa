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
//! without its derivations admits none of their reads for a row. Under
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
use super::*;
use crate::ast::{Slot, WindowFunction};
use crate::diagnostic::{
    lookup::{self, Owner},
    ConditionCode as C, Diagnostic,
};
use crate::reference_scope::Phase;
use crate::result_type::ResultType;
use crate::scalar_verb::{CheckTyping, Operation, Read};
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
            // A row formula: 0.5's `compute` text, or 1.0's derivation text,
            // which binding compiles alike (FORMULA-27).
            ExprKind::Formula(formula)
                if matches!(
                    formula.context,
                    ExpressionContext::Compute | ExpressionContext::Derivation
                ) =>
            {
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

    /// The record selections at the positions below `root`, in binding's
    /// order: each source binding's declaration and filter, a mapping or
    /// cut's source type, each window's, each aggregate's header and filter,
    /// and each ODM read's filter. Without row templates (`scope`), also each
    /// read of another input through its implicit join ([`Self::join`]), a
    /// `case` predicate's before its result, and the static types binding
    /// checks once those reads bind: each `case` predicate's, a flag's once
    /// every name it reads binds, and a `case` result formula's operands'.
    /// `output` types a column window's baseline operands, which only a spec
    /// without row templates does here.
    fn selections(
        &self,
        root: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        self.select(root, scope, output, false, findings)
    }

    /// [`Self::selections`] at the position `e`, a `case` result when
    /// `branch`.
    fn select(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        output: bool,
        branch: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        match &spec.expr(e).kind {
            // The implicit template types each predicate once its names are
            // read, before its result (`case_predicates::implicit_predicate`).
            ExprKind::Case(arms) => {
                for arm in arms {
                    if let (Some(scope), Some(condition)) = (scope, arm.condition) {
                        self.predicate_joins(scope, condition, findings);
                        condition_types(spec, condition, |name| self.declared_type(name), findings);
                    }
                    self.select(arm.result, scope, output, true, findings)?;
                }
            }
            ExprKind::Ref(read) => {
                if let Some(scope) = scope {
                    self.read(scope, &path(spec, e), read.as_str(), findings);
                }
            }
            ExprKind::Source(_) => self.selection(e, scope, false, findings)?,
            ExprKind::FirstAvailable { sources, .. } => {
                for &source in sources {
                    self.selection(source, scope, branch, findings)?;
                }
            }
            // Each operand is read where binding reads it, a source binding
            // through its selection; the verb's operand types once a source
            // binding reads without a finding (EXPR-11, EXPR-13), where
            // `check()` types them ([`CheckTyping::Always`]). Preflight typed
            // every other name a declaration types, and refused the spec when
            // it refused one or the verb's late refusal followed; binding
            // takes a lookup's field untyped, so the late refusal follows
            // its read here (TEXT-6, TEXT-10).
            ExprKind::Scalar { op, .. } => {
                let operation = crate::scalar_verb::Operation::from_tree(spec, e)
                    .map_err(|_| BindError::Internal)?;
                let mut taken = scope.is_some();
                for (index, operand) in operation.operands().iter().enumerate() {
                    match &operand.read {
                        Read::Source(source) => {
                            let start = findings.len();
                            self.selection(*source, scope, false, findings)?;
                            let typed = findings.len() == start
                                && scope.is_some()
                                && op.check_typing() == CheckTyping::Always;
                            if let Some(read) = variable(spec, *source).filter(|_| typed) {
                                self.operand_type(&operation, index, read, findings);
                            }
                            taken = false;
                        }
                        Read::Name(read) => {
                            if let Some(scope) = scope {
                                taken &= self.read(scope, &operand.path, read, findings)
                                    && untyped(&operation, self.lookup_field(read));
                            }
                        }
                        Read::Literal(_) => {}
                    }
                }
                late_refusal(&operation, taken, findings);
            }
            // A flag is typed only once every name it reads binds
            // (`flags::bind`).
            ExprKind::Flag { condition, .. } => {
                if let Some(scope) = scope {
                    let start = findings.len();
                    self.predicate_joins(scope, *condition, findings);
                    if findings.len() == start {
                        condition_types(
                            spec,
                            *condition,
                            |name| self.declared_type(name),
                            findings,
                        );
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
                self.reduction(e, aggregate, scope, findings)?;
            }
            ExprKind::Odm(read) => {
                if let Some(filter) = read.filter {
                    self.grammar(filter, findings)?;
                }
            }
            ExprKind::Call { args, .. } => {
                if let Some(scope) = scope {
                    for &(_, value) in args {
                        if let Some(read) = name(spec, value) {
                            self.join(scope, &path(spec, value), read, findings);
                        }
                    }
                }
            }
            ExprKind::Formula(ast::Formula {
                compiled: Ok(compiled),
                text,
                ..
            }) if branch && scope.is_some() => {
                self.branch_formula(compiled, &render(spec, text.meta), findings);
            }
            ExprKind::Literal(_) | ExprKind::Formula(_) | ExprKind::Unsupported(_) => {}
        }
        Ok(())
    }

    /// Without row templates, a read at `path` of another input through its
    /// implicit join, which on a 1.0 tree matches on the input's declared
    /// `key`: an input that declares none fails at the read, whatever it
    /// stores (LOOKUP-5, D111), and so does one whose key column the current
    /// row cannot pair ([`declared_key`]). True when the read binds, as far
    /// as this decides.
    fn join(
        &self,
        scope: Scope<'_>,
        path: &str,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let Some((owner, _)) = name.split_once('.') else {
            return true;
        };
        if scope.drivers.contains(&owner)
            || !self
                .prepared
                .sources
                .iter()
                .any(|input| input.name == owner)
        {
            return true;
        }
        let refused = if keyless(self.spec, owner) {
            Some(undeclared_key(path, owner))
        } else {
            declared_key(
                self.spec,
                self.prepared,
                &self.declared,
                path,
                owner,
                Pairing::Column,
            )
        };
        let Some(finding) = refused else {
            return true;
        };
        findings.push(finding);
        false
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

    /// Whether `name` is a lookup's field, which the implicit template
    /// reads through the lookup's selector.
    fn lookup_field(&self, name: &str) -> bool {
        name.split_once('.')
            .is_some_and(|(owner, _)| self.prepared.intermediates.contains(owner))
    }

    /// Whether `owner` qualifies nothing declared, no input or lookup, so a
    /// field it qualifies binds nothing, whatever any input stores.
    fn undeclared(&self, owner: &str) -> bool {
        !self.prepared.intermediates.contains(owner)
            && !self
                .prepared
                .sources
                .iter()
                .any(|input| input.name == owner)
    }

    /// Without row templates, a read at `path`: of a lookup binding does not
    /// bind, or of a field qualified by nothing declared, each of which fails
    /// as unknown (NAME-3), or of another input through its implicit join
    /// ([`Self::join`]). True when it binds, as far as this decides.
    fn read(
        &self,
        scope: Scope<'_>,
        path: &str,
        name: &str,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        if self.refused_lookup(name)
            || name
                .split_once('.')
                .is_some_and(|(owner, _)| self.undeclared(owner) && !scope.drivers.contains(&owner))
        {
            findings.push(BindFinding::UnknownReference {
                path: path.into(),
                name: name.into(),
            });
            return false;
        }
        self.join(scope, path, name, findings)
    }

    /// Without row templates, a formula in a `case` result, at its text
    /// `path`, reads only lookups and columns (`BindContext::branch_formula`):
    /// each name, once, that is no column, no lookup's field, or a field of a
    /// lookup binding does not bind fails as unknown (NAME-3). Then its
    /// operands are typed over the names that bound ([`Self::column_type`],
    /// FORMULA-12).
    fn branch_formula(
        &self,
        compiled: &CompiledExpression,
        path: &str,
        findings: &mut Vec<BindFinding>,
    ) {
        for name in compiled.identifiers() {
            let unknown = match name.split_once('.') {
                Some((owner, _)) if self.prepared.intermediates.contains(owner) => {
                    self.refused_lookup(name)
                }
                Some(_) => true,
                None => !self
                    .spec
                    .columns
                    .iter()
                    .any(|column| column.name.name == name),
            };
            if unknown {
                findings.push(BindFinding::UnknownReference {
                    path: path.into(),
                    name: name.into(),
                });
            }
        }
        operand_types(compiled, |name| self.column_type(name), findings);
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

    /// Without row templates, each name a flag's or `case` predicate's
    /// condition reads, once, which a name that binding cannot read there
    /// leaves unknown to the predicate (FORMULA-9): one qualified by nothing
    /// an input or a lookup is, a field of a lookup binding does not bind,
    /// or a read of another input through its implicit join that fails
    /// ([`Self::join`]), whose refusal comes first.
    fn predicate_joins(
        &self,
        scope: Scope<'_>,
        condition: ExprId,
        findings: &mut Vec<BindFinding>,
    ) {
        for (at, read) in names(self.spec, condition) {
            let undeclared = read
                .split_once('.')
                .is_some_and(|(owner, _)| self.undeclared(owner));
            if undeclared || self.refused_lookup(read) || !self.join(scope, &at, read, findings) {
                findings.push(predicate_unknown(&at, read));
            }
        }
    }

    /// A source operand's record selection ([`Self::source`]); then, without
    /// row templates, `require_match` and `between` on a read that binds and
    /// is no implicit join ([`join_options`]). Whatever its filter reports,
    /// the implicit template binds a field of the base, a column, or a field
    /// of a lookup whose key types do not fail, and on the 0.5 surface a
    /// bare name its absence answers (D67), and reads another input through
    /// its implicit join ([`Self::join`]). A `first_available` source of a
    /// `case` result (`branch`) binds as a row template's does, only with no
    /// finding of its own.
    fn selection(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        branch: bool,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let start = findings.len();
        // A name, in either rules, reads another input through its join.
        if let (ExprKind::Ref(read), Some(scope)) = (&spec.expr(e).kind, scope) {
            self.read(scope, &path(spec, e), read.as_str(), findings);
            return Ok(());
        }
        let (false, Some(scope), ExprKind::Source(source)) =
            (self.source(e, scope, findings)?, scope, &spec.expr(e).kind)
        else {
            return Ok(());
        };
        if branch {
            super::boundaries::Reads::column(
                spec,
                self.prepared,
                &self.declared,
                scope.drivers,
                true,
                &[],
                &self.refused,
            )
            .source(e, start, findings);
            return Ok(());
        }
        let variable = name(spec, source.variable).unwrap_or_default();
        if !self.read(scope, &path(spec, e), variable, findings) {
            return Ok(());
        }
        // A join that binds keeps its range, whose value reads the current
        // row; a lookup binding does not bind gives it none, nor does a
        // record nothing declared names (LOOKUP-9).
        if let Some((between, value)) = source
            .between
            .as_ref()
            .and_then(|between| Some((between, name(spec, between.value)?)))
            .filter(|_| {
                variable.split_once('.').is_some_and(|(owner, _)| {
                    !scope.drivers.contains(&owner)
                        && self
                            .prepared
                            .sources
                            .iter()
                            .any(|input| input.name == owner)
                })
            })
            .filter(|&(_, value)| {
                self.refused_lookup(value)
                    || value
                        .split_once('.')
                        .is_some_and(|(owner, _)| self.undeclared(owner))
            })
        {
            findings.push(unknown_range_value(
                spec,
                between,
                Owner::Source(variable),
                value,
            ));
            return Ok(());
        }
        // A read of another input through its implicit join binds its range.
        if variable.split_once('.').is_some_and(|(owner, _)| {
            !scope.drivers.contains(&owner)
                && self
                    .prepared
                    .sources
                    .iter()
                    .any(|input| input.name == owner)
        }) {
            findings.extend(join_range_findings(spec, self.prepared, e));
            return Ok(());
        }
        let binds = match variable.split_once('.') {
            Some((owner, _)) => match spec
                .intermediates
                .iter()
                .position(|item| item.id.name == owner)
            {
                Some(index) => !self.refused.contains(&index),
                None => scope.drivers.contains(&owner),
            },
            None => {
                spec.columns
                    .iter()
                    .any(|column| column.name.name == variable)
                    || (source.absent.is_some()
                        && spec.header.surface.absent_answers_unknown_variable())
            }
        };
        if binds {
            join_options(spec, e, findings);
        }
        Ok(())
    }

    /// An aggregate's expression grammar and header, then its filter's
    /// grammar, the key a column aggregate matches on, and its derive
    /// bindings, each followed by its conversion once it binds (VALUE-24).
    /// Binding binds nothing more of a column or row aggregate whose
    /// expression or header fails, and binds its derive steps only when
    /// nothing else of it failed.
    fn reduction(
        &self,
        e: ExprId,
        aggregate: &ast::Aggregate,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
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
        let start = findings.len();
        let relation = match scope {
            Some(_) if compiled => match self.aggregate(e, aggregate, scope, findings)? {
                Some(relation) => relation,
                None => return Ok(()),
            },
            Some(_) => return Ok(()),
            None => None,
        };
        if let Some(filter) = aggregate.filter {
            self.grammar(filter, findings)?;
        }
        // Without `key` of its own a column aggregate matches the relation it
        // reduces on that input's declared `key` (D63), which a 1.0 input
        // that declares none does not give (LOOKUP-5, D111), and whose
        // columns pair with the current row's (D111 item 2).
        if let (Some(scope), Some(relation), Some(JoinKey::FromInput)) =
            (scope, relation, &aggregate.key)
        {
            if !matches!(scope.phase, Some(Phase::Row { .. }))
                && self
                    .prepared
                    .sources
                    .iter()
                    .any(|input| input.name == relation)
            {
                if keyless(spec, relation) {
                    findings.push(BindFinding::AggregateDiagnostic(
                        lookup::undeclared_input_key(&path(spec, e), relation, true),
                    ));
                } else {
                    self.aggregate_key(aggregate, relation, findings);
                }
            }
        }
        let column = scope.is_some_and(|scope| !matches!(scope.phase, Some(Phase::Row { .. })));
        let one_meaning = spec.header.surface.one_meaning_per_form();
        if let (true, true, Some(relation)) = (column, one_meaning, relation) {
            self.aggregate_keys(e, aggregate, relation, findings)?;
        }
        if scope.is_some() && findings.len() != start {
            return Ok(());
        }
        // Once nothing else of it fails, binding binds its range: on a 1.0
        // tree a bare value is an output column alone, and one that no column
        // takes names nothing the row holds, nor does a field of a lookup
        // binding does not bind or one qualified by nothing declared
        // (LOOKUP-9).
        if let Some((between, value)) = aggregate
            .between
            .as_ref()
            .filter(|between| {
                column && relation.is_some() && (between.lower.is_some() || between.upper.is_some())
            })
            .and_then(|between| Some((between, name(spec, between.value)?)))
            .filter(|&(_, value)| match value.split_once('.') {
                None => one_meaning && !self.column(value),
                Some((owner, _)) => self.undeclared(owner) || self.refused_lookup(value),
            })
        {
            findings.push(unknown_range_value(spec, between, Owner::Unnamed, value));
        } else if let (true, Some(relation), Some(between)) = (
            column,
            relation,
            aggregate
                .between
                .as_ref()
                .filter(|between| between.lower.is_some() || between.upper.is_some()),
        ) {
            findings.extend(range_types(
                spec,
                self.prepared,
                between,
                relation,
                Owner::Unnamed,
            ));
        }
        if aggregate.bindings.is_empty() {
            return Ok(());
        }
        // Each derive binding reads the relation the aggregate reduces, in
        // an ungrouped row phase ([`Self::donor_walk`]), typed by the
        // relation's stored fields and the earlier bindings' declared types.
        let derived = aggregate
            .bindings
            .iter()
            .map(|binding| binding.name.name.as_str())
            .collect::<Vec<_>>();
        let typing = |name: &str| match name.split_once('.') {
            None => aggregate
                .bindings
                .iter()
                .find(|binding| binding.name.name == name)
                .and_then(|binding| binding.declared),
            Some(_) => self.declared.name(name).column_type(),
        };
        let env = self.prepared.tree.env(&self.declared);
        let table = ast::Table::new(spec, e, ast::Scope::Row, &env);
        for (at, binding) in aggregate.bindings.iter().enumerate() {
            let start = findings.len();
            match (scope, relation) {
                (Some(_), Some(relation)) => {
                    let drivers = [relation];
                    let donor = Scope {
                        drivers: &drivers,
                        row: None,
                        phase: Some(Phase::Row { group_by: None }),
                        available: None,
                    };
                    let reads = super::donors::DonorReads::derive(
                        spec,
                        self.prepared,
                        relation,
                        &derived,
                        at,
                        &self.refused,
                        &typing,
                    );
                    self.defects(binding.derivation.expr, findings)?;
                    self.donor_walk(&reads, donor, binding.derivation.expr, findings)?;
                }
                _ => self.derivation(binding.derivation.expr, None, false, findings)?,
            }
            // A binding that bound converts to its declared type like a
            // column (VALUE-24), as binding states it once the binding binds
            // (`aggregates::bind_derivations`).
            if let (Some(declared), None, true) = (
                binding.declared,
                &binding.derivation.unconvertible,
                findings.len() == start,
            ) {
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
        Ok(())
    }

    /// A qualified column aggregate without `key` of its own pairs each
    /// column of its input's declared `key` with the current row's
    /// same-named column (D63, D111 item 2). Binding checks every column, in
    /// declared order, and reports each refusal at the aggregate's key path
    /// (`aggregates::bound_relation`): a column the input does not store, as
    /// far as [`stores`] decides (`unknown_field`, LOOKUP-5, identifier
    /// `<input>.<column>`); else one the specification declares no column of
    /// (LOOKUP-5, identifier `<column>`), or a pair whose declared types
    /// cannot compare (`incompatible_input_type`, LOOKUP-9).
    fn aggregate_key(
        &self,
        aggregate: &ast::Aggregate,
        relation: &str,
        findings: &mut Vec<BindFinding>,
    ) {
        let spec = self.spec;
        let ExprKind::Formula(ast::Formula {
            compiled: Ok(compiled),
            text,
            ..
        }) = &spec.expr(aggregate.expr).kind
        else {
            return;
        };
        // A reduction that names no field of its relation takes its pairing's
        // path from a filter that reads the relation.
        let derived = aggregate.bindings.iter().any(|binding| {
            spec.reads(binding.derivation.expr, ast::Scope::Row)
                .iter()
                .any(|read| {
                    read.name
                        .owner()
                        .is_some_and(|owner| !self.prepared.intermediates.contains(owner))
                })
        });
        let expression = render(spec, text.meta);
        let at = if compiled.identifiers().any(|name| name.contains('.'))
            || derived
            || !aggregate.group_by.is_empty()
        {
            expression
        } else {
            aggregate
                .filter
                .and_then(|filter| match &spec.expr(filter).kind {
                    ExprKind::Formula(ast::Formula {
                        compiled: Ok(plan),
                        text,
                        ..
                    }) if plan.identifiers().any(|name| {
                        name.split_once('.')
                            .is_some_and(|(owner, _)| owner == relation)
                    }) =>
                    {
                        Some(render(spec, text.meta))
                    }
                    _ => None,
                })
                .unwrap_or(expression)
        };
        for column in spec.input_key(relation).unwrap_or_default() {
            let field = column.name.as_str();
            let diagnostic = if !stores(self.prepared, relation, field) {
                lookup::donor_key_reference(&at, Owner::Unnamed, &format!("{relation}.{field}"))
            } else if let Some(output) =
                spec.columns.iter().find(|column| column.name.name == field)
            {
                let Some(actual) =
                    stored_type(self.prepared, &self.declared, relation, field).column_type()
                else {
                    continue;
                };
                let expected = ResultType::column(output.kind);
                if expected.comparable(actual.into()) {
                    continue;
                }
                lookup::key_type(
                    &at,
                    Owner::Unnamed,
                    field,
                    expected,
                    ResultType::column(actual),
                )
            } else {
                lookup::match_value_reference(&at, Owner::Unnamed, field)
            };
            findings.push(BindFinding::AggregateDiagnostic(diagnostic));
        }
    }

    /// One derivation of a row template or the column phase, node by node
    /// in binding's order (`BindContext::bind`): a structured source's
    /// declaration and selection before its variable, and its join options
    /// after it, a window's grouping and defects before its reads, a
    /// mapping's source type and a placeholder's type once their read binds,
    /// an aggregate's header alone, and each other read where binding binds
    /// it ([`super::boundaries`]). The authored defects precede it.
    fn walk(
        &self,
        reads: &super::boundaries::Reads<'_>,
        scope: Scope<'_>,
        e: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let names = |text: ExprId| names(spec, text);
        // A source operand: a structured binding unless its declaration or
        // selection stops binding before its variable, or a name. True when
        // it binds.
        let source = |e: ExprId, findings: &mut Vec<BindFinding>| -> Result<bool, BindError> {
            Ok(match &spec.expr(e).kind {
                ExprKind::Source(_) => {
                    let start = findings.len();
                    !self.source(e, Some(scope), findings)? && reads.source(e, start, findings)
                }
                ExprKind::Ref(read) => reads.read(&path(spec, e), read.as_str(), findings),
                _ => true,
            })
        };
        match &spec.expr(e).kind {
            ExprKind::Literal(_) | ExprKind::Unsupported(_) => {}
            ExprKind::Ref(_) | ExprKind::Source(_) => {
                source(e, findings)?;
            }
            // Each name, then the operand types over the names that bound: a
            // column's or a stored field's declared type (FORMULA-12).
            ExprKind::Formula(formula) => {
                if let Ok(compiled) = &formula.compiled {
                    let mut bound = Vec::new();
                    for (at, read) in names(e) {
                        if reads.formula(&at, compiled.expression(), read, findings) {
                            bound.push(read);
                        }
                    }
                    operand_types(
                        compiled,
                        |name| {
                            bound
                                .contains(&name)
                                .then(|| self.declared.name(name).single())
                                .flatten()
                        },
                        findings,
                    );
                }
            }
            ExprKind::Case(arms) => {
                for arm in arms {
                    for (at, read) in arm.condition.map(names).unwrap_or_default() {
                        reads.predicate(&at, read, findings);
                    }
                    self.walk(reads, scope, arm.result, findings)?;
                }
            }
            // Each name, then the condition's types once every name binds
            // (`flags::bind`).
            ExprKind::Flag { condition, .. } => {
                let start = findings.len();
                for (at, read) in names(*condition) {
                    reads.flag(&at, read, findings);
                }
                if findings.len() == start {
                    condition_types(spec, *condition, |name| self.declared_type(name), findings);
                }
            }
            ExprKind::FirstAvailable { sources, .. } => {
                for &operand in sources {
                    source(operand, findings)?;
                }
            }
            // Each operand is read where binding reads it, a source binding
            // as binding binds it; once a read binds, the verb's operand type
            // where `check()` types it (EXPR-11, EXPR-13, TEXT-1); and once
            // binding takes every operand, the verb's late refusal (TEXT-6,
            // TEXT-10).
            ExprKind::Scalar { op, .. } => {
                let operation = crate::scalar_verb::Operation::from_tree(spec, e)
                    .map_err(|_| BindError::Internal)?;
                let mut taken = true;
                for (index, operand) in operation.operands().iter().enumerate() {
                    let (read, bound) = match &operand.read {
                        Read::Source(node) => (variable(spec, *node), source(*node, findings)?),
                        Read::Name(name) => (
                            Some(name.as_str()),
                            reads.read(&operand.path, name, findings),
                        ),
                        Read::Literal(_) => continue,
                    };
                    let Some(read) = read.filter(|_| bound) else {
                        taken = false;
                        continue;
                    };
                    let typed = (op.check_typing() != CheckTyping::Never)
                        .then(|| self.operand_type(&operation, index, read, findings))
                        .flatten();
                    taken &= typed.unwrap_or_else(|| untyped(&operation, reads.lookup_field(read)));
                }
                late_refusal(&operation, taken, findings);
            }
            ExprKind::Window(window) => self.window(e, window, Some(scope), findings)?,
            ExprKind::Aggregate(aggregate) => {
                self.reduction(e, aggregate, Some(scope), findings)?
            }
            ExprKind::Odm(read) => {
                if let Some(filter) = read.filter {
                    self.grammar(filter, findings)?;
                }
            }
            ExprKind::Call { args, .. } => {
                for &(_, value) in args {
                    if let Some(read) = name(spec, value) {
                        reads.argument(&path(spec, value), read, findings);
                    }
                }
            }
        }
        Ok(())
    }

    /// One derivation of a lookup, or one derive binding of an aggregate,
    /// node by node in binding's order over the donor's records, which
    /// `scope` names ([`super::donors`]): a structured source's declaration
    /// and filter before its read, and its join options once the read binds;
    /// each other read where binding reads it; a mapping's source type and a
    /// placeholder's type once their read binds; a window's defects before
    /// its reads; and an aggregate as in any ungrouped row. The authored
    /// defects precede it.
    fn donor_walk(
        &self,
        reads: &super::donors::DonorReads<'_>,
        scope: Scope<'_>,
        e: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        // A source operand: a structured binding unless its declaration or
        // filter stops binding before its variable, or a name. True when it
        // binds.
        let source = |e: ExprId, findings: &mut Vec<BindFinding>| -> Result<bool, BindError> {
            Ok(match &spec.expr(e).kind {
                ExprKind::Source(source) => {
                    let start = findings.len();
                    let refused = self.source(e, Some(scope), findings)?;
                    reads.name_unknown(&mut findings[start..]);
                    if refused {
                        return Ok(false);
                    }
                    let variable = name(spec, source.variable).unwrap_or_default();
                    let bound =
                        reads.read(&path(spec, e), variable, findings) && findings.len() == start;
                    if bound {
                        join_options(spec, e, findings);
                    }
                    bound
                }
                ExprKind::Ref(read) => reads.read(&path(spec, e), read.as_str(), findings),
                _ => true,
            })
        };
        match &spec.expr(e).kind {
            ExprKind::Literal(_) | ExprKind::Unsupported(_) | ExprKind::Call { .. } => {}
            ExprKind::Ref(_) | ExprKind::Source(_) => {
                source(e, findings)?;
            }
            // Each name, then the operand types over the names that bound, as
            // binding's captured types type them there (FORMULA-12).
            ExprKind::Formula(formula) => {
                let mut bound = Vec::new();
                for (at, read) in names(spec, e) {
                    if reads.read(&at, read, findings) {
                        bound.push(read);
                    }
                }
                if let Ok(compiled) = &formula.compiled {
                    operand_types(
                        compiled,
                        |name| {
                            bound
                                .contains(&name)
                                .then(|| reads.kind(name).map(Into::into))
                                .flatten()
                        },
                        findings,
                    );
                }
            }
            ExprKind::Case(arms) => {
                for arm in arms {
                    for (at, read) in arm
                        .condition
                        .map(|condition| names(spec, condition))
                        .unwrap_or_default()
                    {
                        reads.predicate(&at, read, false, findings);
                    }
                    self.donor_walk(reads, scope, arm.result, findings)?;
                }
            }
            // Each name, then the condition's types once every name binds
            // (`flags::bind`).
            ExprKind::Flag { condition, .. } => {
                let start = findings.len();
                let mut bound = true;
                for (at, read) in names(spec, *condition) {
                    bound &= reads.predicate(&at, read, true, findings);
                }
                if bound && findings.len() == start {
                    condition_types(
                        spec,
                        *condition,
                        |name| reads.kind(name).map(Into::into),
                        findings,
                    );
                }
            }
            ExprKind::FirstAvailable { sources, .. } => {
                for &operand in sources {
                    source(operand, findings)?;
                }
            }
            // Each operand is read where binding reads it, a source binding
            // as binding binds it; once a read binds, the verb's operand type
            // where `check()` types it, as the donor's records type it
            // (EXPR-11, EXPR-13, TEXT-1); and once binding takes every
            // operand, the verb's late refusal (TEXT-6, TEXT-10). Binding
            // takes a lookup's field untyped there, and its late refusal
            // leaves the field's type unchecked.
            ExprKind::Scalar { op, .. } => {
                let operation = crate::scalar_verb::Operation::from_tree(spec, e)
                    .map_err(|_| BindError::Internal)?;
                let mut taken = true;
                for (index, operand) in operation.operands().iter().enumerate() {
                    let (read, bound) = match &operand.read {
                        Read::Source(node) => (variable(spec, *node), source(*node, findings)?),
                        Read::Name(name) => (
                            Some(name.as_str()),
                            reads.read(&operand.path, name, findings),
                        ),
                        Read::Literal(_) => continue,
                    };
                    let Some(read) = read.filter(|_| bound) else {
                        taken = false;
                        continue;
                    };
                    if operation.late_refusal().is_some()
                        && untyped(&operation, reads.lookup_field(read))
                    {
                        continue;
                    }
                    let kind = reads
                        .kind(read)
                        .filter(|_| op.check_typing() != CheckTyping::Never);
                    let refusal = kind.and_then(|kind| operation.check(index, kind));
                    taken &= kind.is_some() && refusal.is_none();
                    findings.extend(refusal.map(BindFinding::Scalar));
                }
                late_refusal(&operation, taken, findings);
            }
            // The window's defects, then its operands and each name of its
            // filter once, in sorted order.
            ExprKind::Window(window) => {
                self.window(e, window, None, findings)?;
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
                for (read, at) in window_operands(spec, window).into_iter().chain(filtered) {
                    reads.window(&at, read, findings);
                }
            }
            ExprKind::Aggregate(aggregate) => {
                self.reduction(e, aggregate, Some(scope), findings)?;
            }
            ExprKind::Odm(read) => {
                if let Some(filter) = read.filter {
                    self.grammar(filter, findings)?;
                }
            }
        }
        Ok(())
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

    /// What binding decides of an aggregate from its declarations before it
    /// reads a field: a reduction of several relations, a derive binding's
    /// names, groups outside its relation, a context it cannot reduce in
    /// and an undeclared relation; then, at column level, each name outside
    /// a reduction that is no group key (FORMULA-22), a group that is no key
    /// (LOOKUP-19) and its operands' types (FORMULA-23). A row template's
    /// groups are fields of its driver, which binding reads first. `None`
    /// when binding binds nothing more of the aggregate; otherwise the
    /// relation it reduces, when it names one.
    fn aggregate(
        &self,
        e: ExprId,
        aggregate: &ast::Aggregate,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<Option<&str>>, BindError> {
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
            return Ok(Some(None));
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
            .filter_map(|name| name.split_once('.').map(|(relation, _)| relation))
            .chain(compiled.record_relations())
            .collect::<Vec<_>>();
        relations.sort_unstable();
        relations.dedup();
        let bare = compiled.identifiers().any(|name| !name.contains('.'));
        if relations.len() > 1 || (!relations.is_empty() && bare) {
            let mut relations = relations.into_iter().map(String::from).collect::<Vec<_>>();
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
            return Ok(None);
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
            relations.first().copied()
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
                return Ok(Some(None));
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
                return Ok(None);
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
                return Ok(None);
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
                return Ok(None);
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
            return Ok(None);
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
            return Ok(None);
        }
        if row {
            // A grouped row's aggregate reads, outside its reductions, only
            // its template's group keys (FORMULA-22).
            if let Phase::Row {
                group_by: Some(groups),
            } = phase
            {
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
            }
            return Ok(Some(relation));
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
        // each group is a key: a declared key's field, the field of its 1.0
        // input's declared key, or else an output key, which a group the
        // relation stores is then applicable on (LOOKUP-19).
        if let Some(relation) = relation {
            let applicable = match &aggregate.key {
                Some(JoinKey::Columns { columns, .. }) => {
                    columns.iter().map(|column| column.name.as_str()).collect()
                }
                Some(JoinKey::Pairs { pairs, .. }) => pairs
                    .iter()
                    .map(|(column, _)| column.name.as_str())
                    .collect(),
                Some(JoinKey::FromInput) => spec
                    .input_key(relation)
                    .unwrap_or_default()
                    .iter()
                    .map(|column| column.name.as_str())
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
                return Ok(None);
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
        Ok(Some(relation))
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
    /// filter's grammar and each name it reads outside its variable's input,
    /// and, outside the implicit template, each name of a variable's record
    /// that nothing declares, unless the variable reads a lookup binding does
    /// not bind, whose read fails first. True when binding binds nothing more
    /// of it: a declaration defect, or a filter on a group key.
    fn source(
        &self,
        e: ExprId,
        scope: Option<Scope<'_>>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<bool, BindError> {
        let spec = self.spec;
        let ExprKind::Source(source) = &spec.expr(e).kind else {
            return Ok(false);
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
                return Ok(true);
            }
        }
        if !defects.is_empty() {
            findings.extend(defects.into_iter().map(|defect| {
                BindFinding::Source(source_expressions::SourceFinding::from_diagnostic(defect))
            }));
            return Ok(true);
        }
        // Binding reads a lookup it does not bind before it binds a filter.
        let Some(filter) = source.filter.filter(|_| !self.refused_lookup(variable)) else {
            return Ok(false);
        };
        let qualifier = variable.split_once('.').map(|(owner, _)| owner);
        match (
            qualifier.filter(|owner| !self.prepared.intermediates.contains(owner)),
            scope,
        ) {
            // Only a stored variable's filter is compiled. It reads only the
            // records of its variable's input, so binding refuses each other
            // name before it binds a field. A variable qualified by nothing
            // declared stores no field: under row templates and over a donor's
            // records each name qualified as it is is unknown there too
            // (NAME-3, `Declaration::bind_row_filter`), and the implicit
            // template refuses the variable before it binds any filter.
            (Some(dataset), scope) => {
                let undeclared = self.undeclared(dataset)
                    && scope.is_none_or(|scope| !scope.drivers.contains(&dataset));
                if undeclared && scope.is_some_and(|scope| scope.phase.is_none()) {
                    return Ok(false);
                }
                let start = findings.len();
                self.grammar(filter, findings)?;
                let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
                    return Ok(false);
                };
                let (Some(_), true, Ok(plan)) = (
                    scope,
                    findings.len() == start
                        && (undeclared
                            || self
                                .prepared
                                .sources
                                .iter()
                                .any(|input| input.name == dataset)),
                    &formula.compiled,
                ) else {
                    return Ok(false);
                };
                let at = render(spec, formula.text.meta);
                let mut seen = BTreeSet::new();
                for identifier in plan.identifiers() {
                    if !seen.insert(identifier) {
                        continue;
                    }
                    if identifier
                        .split_once('.')
                        .is_none_or(|(owner, _)| owner != dataset)
                    {
                        findings.push(BindFinding::Source(source_expressions::SourceFinding::new(
                            C::SourceFilterReference,
                            &at,
                            &[("identifier", identifier), ("dataset", dataset)],
                        )));
                    } else if undeclared {
                        findings.push(BindFinding::UnknownReference {
                            path: at.clone(),
                            name: identifier.into(),
                        });
                    }
                }
            }
            // An output column, a row's value or a named intermediate's
            // field is one value already: there are no records to filter.
            // Without row templates a bare name that is no output column
            // fails as a reference first, unless the binding answers its
            // absence, which only the 0.5 surface does (D67).
            (None, Some(scope))
                if qualifier.is_some()
                    || scope.phase.is_some()
                    || (source.absent.is_some()
                        && spec.header.surface.absent_answers_unknown_variable())
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
        Ok(false)
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
        // driver must be a group key (ROW-5 in a row template, NAME-4 at
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
        // A partition names each variable once (WINDOW-3); report every repeat.
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
        // (NAME-3), and a row template's window reads only the columns the
        // template derives in the row phase (COLUMN-4): the operands, then
        // each filter name once, in sorted order.
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
                if name.contains('.') {
                    if !driver(name, scope) {
                        findings.push(BindFinding::UnknownReference {
                            path,
                            name: name.into(),
                        });
                    }
                } else if let (Some(row), Some(column)) = (
                    scope.row.filter(|_| scope.available.is_some()),
                    spec.columns
                        .iter()
                        .position(|column| column.name.name == name)
                        .filter(|column| {
                            scope
                                .available
                                .is_some_and(|available| !available.contains(column))
                        }),
                ) {
                    findings.push(BindFinding::OutputReference {
                        path,
                        name: name.into(),
                        row: Some(row.into()),
                        finding: crate::reference_binding::Diagnostic::PhaseBoundary { column },
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
        // A 1.0 operand's path is the field its surface writes (P-OP-2).
        let at = match spec.header.surface {
            ast::Surface::V0_5 => format!("{}.{operand}", path(spec, e)),
            ast::Surface::V1 { .. } if operand == "date" => spec.cite(date.meta),
            ast::Surface::V1 { .. } => spec.cite(spec.expr(reference).meta),
        };
        let mut finding = LookupFinding::new(&at, C::WindowBaselineInputType);
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

    /// Under row templates, the static types over the declared types of each
    /// `case` predicate at the positions below `root` (FORMULA-5,
    /// FORMULA-12), which binding finds once it evaluates the predicate of a
    /// derivation that bound. The walk types a flag where binding does, and
    /// without row templates a `case` predicate too ([`Self::selections`]).
    fn predicate_types(&self, root: ExprId, findings: &mut Vec<BindFinding>) {
        let spec = self.spec;
        if let ExprKind::Case(arms) = &spec.expr(root).kind {
            for arm in arms {
                if let Some(condition) = arm.condition {
                    condition_types(spec, condition, |name| self.declared_type(name), findings);
                }
                self.predicate_types(arm.result, findings);
            }
        }
    }

    /// The output columns a derivation reads, as binding records them for
    /// column order: each output column it reads, directly or among the
    /// output rows an unqualified aggregate reduces, and the columns each
    /// named intermediate it reads matches on, unless the intermediate drives
    /// the rows. A window reads output columns alone. A field another input
    /// supplies through an implicit join also depends on the columns that
    /// join matches on: in 1.0 the input's declared key (LOOKUP-1); in 0.5
    /// the output keys that input stores, which only its captured schema
    /// decides. `record` names the dataset whose record the rows read
    /// directly; with `None` every input joins.
    fn edges(&self, root: ExprId, record: Option<&str>) -> Vec<usize> {
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
                Some(owner) != record && self.prepared.intermediates.contains(owner)
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
                ast::Scope::Row => {
                    edges.extend(output(name));
                    let joined = read.name.owner().filter(|&owner| {
                        Some(owner) != record && !spec.header.surface.infers_join_keys()
                    });
                    if let Some(input) = joined {
                        edges.extend(
                            spec.input_key(input)
                                .unwrap_or_default()
                                .iter()
                                .filter_map(|column| output(&column.name)),
                        );
                    }
                }
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

    /// Column order over each derivation's reads (COLUMN-5, COLUMN-6,
    /// COLUMN-7), named with binding's columns and paths.
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
        for identifier in self.prepared.intermediates.names() {
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

    /// On a 1.0 tree, each reference of a written key match value of the
    /// lookup at `index` that reads no current-row value
    /// ([`unknown_match_value`]), at the path binding cites it
    /// (`intermediates::bind_one`): a name written as the value at `key`,
    /// any other value at its own entry, `key.<column>`. A lookup that
    /// another lookup's derivations read binds its key values at each read
    /// instead. Once a value's names bind, the verb it holds binds as in a
    /// derivation ([`Self::key_verb`]). True when one is refused: binding then
    /// binds nothing of the lookup.
    fn key_values(&self, index: usize, findings: &mut Vec<BindFinding>) -> Result<bool, BindError> {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        if !spec.header.surface.one_meaning_per_form() || donor_read(spec, index) {
            return Ok(false);
        }
        let fields = match &item.key {
            Some(JoinKey::Columns { columns, .. }) => columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            Some(JoinKey::Pairs { pairs, .. }) => {
                pairs.iter().map(|(field, _)| field.name.as_str()).collect()
            }
            _ => return Ok(false),
        };
        let at = render(spec, item.meta);
        let start = findings.len();
        for field in fields {
            let Some(value) = self
                .prepared
                .intermediates
                .match_declaration(&item.id.name, field)
            else {
                continue;
            };
            let (path, suggests) = match value {
                match_values::Declaration::Variable(_) => (format!("{at}.key"), true),
                match_values::Declaration::Expression { .. } => {
                    (format!("{at}.key.{field}"), false)
                }
                _ => (format!("{at}.key.{field}"), true),
            };
            let before = findings.len();
            for reference in value
                .references()
                .filter(|name| self.unknown_match_value(name))
            {
                // `SELF` names donor rows, so binding suggests the bare name
                // of a column the current row carries.
                let suggestion = reference
                    .strip_prefix("SELF.")
                    .filter(|bare| suggests && self.column(bare))
                    .map(String::from);
                findings.push(BindFinding::Lookup(LookupFinding::reference(
                    &path,
                    reference,
                    Some(&item.id.name),
                    ReferenceCause::OutputKey,
                    suggestion,
                )));
            }
            if let (match_values::Declaration::Expression { expr, .. }, true) =
                (value, findings.len() == before)
            {
                self.key_verb(*expr, findings)?;
            }
        }
        Ok(findings.len() != start)
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

    /// On a 1.0 tree, what binding refuses of a column aggregate's written
    /// `key` over `relation` that the declarations decide, for each pair in
    /// written order (`aggregates::bound_relation`), each where a named
    /// intermediate's would be (LOOKUP-5): a pair whose declared types cannot
    /// compare at `key` (LOOKUP-9), then each reference of its match value
    /// that reads no current-row value ([`unknown_match_value`]) at the
    /// pair, `key.<column>`, and once its names bind, the verb it holds
    /// ([`Self::key_verb`]). A key column the relation does not store is
    /// decided once its records are read.
    fn aggregate_keys(
        &self,
        e: ExprId,
        aggregate: &ast::Aggregate,
        relation: &str,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        use crate::diagnostic::AggregateFinding as Finding;
        let spec = self.spec;
        let calls = self.prepared.project_calls.is_some();
        let pairs = match &aggregate.key {
            Some(JoinKey::Columns { columns, .. }) => {
                let mut pairs = Vec::new();
                for column in columns {
                    let name = column.name.as_str();
                    if !pairs.iter().any(|(prior, _)| *prior == name) {
                        pairs.push((name, match_values::Declaration::Variable(name.into())));
                    }
                }
                pairs
            }
            Some(JoinKey::Pairs { pairs, .. }) => pairs
                .iter()
                .map(|(field, value)| {
                    match_values::Declaration::from_tree(spec, field.meta, *value, calls)
                        .map(|declaration| (field.name.as_str(), declaration))
                        .map_err(|_| BindError::Internal)
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Ok(()),
        };
        let path = path(spec, e);
        let key = format!("{path}.key");
        let names = |name: &str| self.declared.name(name);
        for (field, value) in pairs {
            let actual = self
                .declared
                .name(&format!("{relation}.{field}"))
                .column_type();
            let expected = match &value {
                match_values::Declaration::Variable(read)
                | match_values::Declaration::Source(read) => names(read),
                match_values::Declaration::Literal(_, expr)
                | match_values::Declaration::Read(_, expr)
                | match_values::Declaration::Expression { expr, .. } => {
                    self.result_type(*expr, ast::Scope::Row, &names)
                }
            };
            if let Some(actual) = actual.filter(|&actual| !expected.comparable(actual.into())) {
                findings.push(BindFinding::AggregateDiagnostic(
                    Finding::KeyType {
                        path: &key,
                        source: &value.display_name(field),
                        expected,
                        actual,
                    }
                    .diagnostic()
                    .map_err(|_| BindError::Internal)?,
                ));
            }
            let pair = format!("{key}.{field}");
            let before = findings.len();
            for reference in value
                .references()
                .filter(|name| self.unknown_match_value(name))
            {
                findings.push(BindFinding::AggregateDiagnostic(
                    Finding::MatchValueReference {
                        path: &pair,
                        identifier: reference,
                    }
                    .diagnostic()
                    .map_err(|_| BindError::Internal)?,
                ));
            }
            // A value whose names bind, for a key column the relation
            // stores, binds the verb it holds.
            if let (match_values::Declaration::Expression { expr, .. }, true) = (
                &value,
                findings.len() == before && stores(self.prepared, relation, field),
            ) {
                self.key_verb(*expr, findings)?;
            }
        }
        Ok(())
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

    /// Each intermediate in the order it is derived, as binding binds it
    /// (`intermediates::bind_one`): each derivation's defects, then its nodes
    /// over the donor's records ([`Self::donor_walk`]) and, once it binds,
    /// its conversion (VALUE-24); then its key types, its filter's grammar
    /// and operand types, and its range's types, over the declared types of
    /// its donor and of earlier intermediates. Returns the intermediates
    /// whose key types fail, which binding does not bind, and those it binds
    /// without their derivations: one whose derivation fails, and every one
    /// derived after one it does not bind (`intermediate_derivations::bind`).
    fn intermediates(
        &self,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(BTreeSet<usize>, BTreeSet<usize>), BindError> {
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
        let mut refused = BTreeSet::new();
        let mut underived = BTreeSet::new();
        for (position, donor) in donors.iter().enumerate() {
            let (index, item) = (donor.index, donor.item);
            let identifier = item.id.name.as_str();
            // The derivations read the donor's records in an ungrouped row
            // phase, typed as binding's captured types type them: a field of
            // the donor record, of another lookup's, or of an input's.
            let relation = item.dataset.name.as_str();
            let drivers = [relation];
            let scope = Scope {
                drivers: &drivers,
                row: None,
                phase: Some(Phase::Row { group_by: None }),
                available: None,
            };
            let derived = item
                .bindings
                .iter()
                .map(|binding| binding.name.name.as_str())
                .collect::<Vec<_>>();
            let typing = |name: &str| {
                let (owner, field) = name.split_once('.').unwrap_or((relation, name));
                if owner == relation {
                    donor.field(field, &stored)
                } else if let Some(other) = donors.iter().find(|other| other.item.id.name == owner)
                {
                    other.field(field, &stored)
                } else {
                    current(owner, field)
                }
                .and_then(ResultType::column_type)
            };
            let derivations = findings.len();
            for (at, (binding, &result)) in item.bindings.iter().zip(&donor.results).enumerate() {
                let start = findings.len();
                let reads = super::donors::DonorReads::lookup(
                    spec,
                    self.prepared,
                    relation,
                    identifier,
                    &derived,
                    at,
                    &refused,
                    &typing,
                );
                self.defects(binding.derivation.expr, findings)?;
                self.donor_walk(&reads, scope, binding.derivation.expr, findings)?;
                if let (Some(typed), None, true) = (
                    binding.declared,
                    &binding.derivation.unconvertible,
                    findings.len() == start,
                ) {
                    findings.extend(
                        types::conversion_finding(&render(spec, binding.meta), result, typed)
                            .map(BindFinding::Scalar),
                    );
                }
            }
            // Binding keeps none of the derivations when one of them fails,
            // or when a lookup derived before this one is not bound.
            if !item.bindings.is_empty() && (!refused.is_empty() || findings.len() != derivations) {
                underived.insert(index);
            }
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
                    refused.insert(index);
                    findings.push(BindFinding::Lookup(LookupFinding::key_type(
                        &format!("{}.key", spec.cite(item.meta)),
                        identifier,
                        &display,
                        expected,
                        actual,
                    )));
                }
            }
            if self.key_values(index, findings)? {
                refused.insert(index);
            }
            if let Some(filter) = item.filter {
                self.grammar(filter, findings)?;
            }
            findings.extend(
                self.filter_types(donor, &stored, &current)
                    .into_iter()
                    .map(BindFinding::Predicate),
            );
            // LOOKUP-17: a uniqueness check cannot depend on the record
            // reading the lookup. Direct reads of another input's current
            // record are already decided by the declarations (#3606).
            let foreign = |name: &str| {
                name.split_once('.').is_some_and(|(owner, _)| {
                    owner != relation && spec.inputs.iter().any(|input| input.name.name == owner)
                })
            };
            let correlated_filter = item.filter.is_some_and(|filter| {
                matches!(&spec.expr(filter).kind, ExprKind::Formula(ast::Formula {
                    compiled: Ok(plan), ..
                }) if plan.identifiers().any(foreign))
            });
            let correlated_derivation = item.bindings.iter().any(|binding| {
                reads::names(
                    spec,
                    binding.derivation.expr,
                    reads::Scan {
                        relation: true,
                        windows: true,
                    },
                    self.prepared.project_calls.is_some(),
                )
                .unwrap_or_default()
                .iter()
                .any(|name| foreign(name))
            });
            for check in &item.checks {
                for (correlated, code) in [
                    (correlated_filter, C::IntermediateUniqueCorrelatedFilter),
                    (
                        correlated_derivation,
                        C::IntermediateUniqueCorrelatedDerivation,
                    ),
                ] {
                    if correlated {
                        let mut finding = LookupFinding::new(&render(spec, check.meta), code);
                        finding.text("intermediate", identifier);
                        findings.push(BindFinding::Lookup(finding));
                    }
                }
            }
            // A range's value is a value of the reading row: an output
            // column, or on the 0.5 surface a field of the primary input's
            // record, or a field of an earlier intermediate or of an input.
            // Its bounds are donor fields; each stated one must be typed
            // (LOOKUP-9). On a 1.0 tree a bare value is an output column
            // alone, and binding refuses one that no column takes at
            // `between.value`, whatever its bounds bind, before it types the
            // range.
            if let Some(between) = &item.between {
                // On a 1.0 tree a bare value that no column takes names
                // nothing the row holds, unless another lookup's derivations
                // read this one: binding then keeps it for the donor record,
                // and refuses it where a row reads the lookup. A value read
                // from a lookup binding did not bind is none, and so is a
                // field qualified by nothing declared.
                let one_meaning = spec.header.surface.one_meaning_per_form();
                if let Some(value) =
                    name(spec, between.value).filter(|value| match value.split_once('.') {
                        None => one_meaning && !self.column(value) && !donor_read(spec, index),
                        Some((owner, _)) => {
                            self.undeclared(owner)
                                || spec
                                    .intermediates
                                    .iter()
                                    .position(|other| other.id.name == owner)
                                    .is_some_and(|other| refused.contains(&other))
                        }
                    })
                {
                    findings.push(unknown_range_value(
                        spec,
                        between,
                        Owner::Intermediate(identifier),
                        value,
                    ));
                    continue;
                }
                let primary = self.prepared.source().name.as_str();
                let earlier = &donors[..position];
                let value =
                    name(spec, between.value).and_then(|value| match value.split_once('.') {
                        None if self.column(value) => Some(declared.name(value)),
                        // A contextual 1.0 value has no type until a donor
                        // record gives it one.
                        None if one_meaning => None,
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
                            types.diagnostic(
                                &format!("{}.between", spec.cite(item.meta)),
                                identifier,
                            ),
                        )));
                    }
                }
            }
        }
        // ROW-2: a driver must expose a single column type for every
        // derived field it keeps. UNKNOWN still needs captured input types;
        // an authored union or missing-only result already has no type.
        for template in &spec.rows.templates {
            let Some(donor) = donors.iter().find(|donor| {
                template
                    .driver
                    .as_ref()
                    .is_some_and(|driver| driver.name == donor.item.id.name)
            }) else {
                continue;
            };
            let unknown = donor
                .item
                .bindings
                .iter()
                .filter_map(|binding| {
                    let name = binding.name.name.as_str();
                    let kept = donor
                        .item
                        .columns
                        .as_ref()
                        .is_none_or(|columns| columns.iter().any(|column| column.name == name));
                    let kind = donor.fields[name];
                    (kept && kind != ResultType::UNKNOWN && kind.column_type().is_none())
                        .then_some(name)
                })
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                let mut finding = LookupFinding::new(
                    &format!("{}.dataset", spec.cite(template.meta)),
                    C::IntermediateDriverType,
                );
                finding.text("intermediate", &donor.item.id.name);
                finding.texts("columns", &unknown);
                findings.push(BindFinding::Lookup(finding));
            }
        }
        Ok((refused, underived))
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
    /// reads other records, the derivation's defects and selections with its
    /// reads of other inputs' joins and its static types, its formula's
    /// qualified names (FORMULA-9), reads of a lookup binding does not bind
    /// (NAME-3) and operand types (FORMULA-12), and its conversion
    /// (VALUE-24); then column order over what every derivation reads
    /// (COLUMN-5, COLUMN-6, COLUMN-7). Returns whether each column's
    /// derivation binds: a conversion finding keeps it bound.
    fn base(&self, findings: &mut Vec<BindFinding>) -> Result<Vec<bool>, BindError> {
        let spec = self.spec;
        let prepared = self.prepared;
        let driver = [prepared.source().name.as_str()];
        let scope = Scope {
            drivers: &driver,
            row: None,
            phase: None,
            available: None,
        };
        let mut bound = Vec::new();
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
                bound.push(false);
                continue;
            }
            self.derivation(root, Some(scope), true, findings)?;
            if let ExprKind::Formula(ast::Formula {
                context: ExpressionContext::Compute | ExpressionContext::Derivation,
                compiled: Ok(compiled),
                text,
                ..
            }) = &spec.expr(root).kind
            {
                let at = render(spec, text.meta);
                qualified_numeric_findings(compiled, &at, &prepared.intermediates, findings);
                // Then each read of a lookup binding does not bind (NAME-3).
                for name in compiled
                    .identifiers()
                    .filter(|name| self.refused_lookup(name))
                {
                    findings.push(BindFinding::UnknownReference {
                        path: at.clone(),
                        name: name.into(),
                    });
                }
                // Then the operand types over the names that bound.
                operand_types(compiled, |name| self.column_type(name), findings);
            }
            dependencies.push(Some(self.edges(root, Some(driver[0]))));
            bound.push(findings.len() == start);
            if findings.len() == start && derivation.unconvertible.is_none() {
                let result = self.result_type(root, ast::Scope::Row, &self.declared);
                findings.extend(
                    prepared
                        .conversion_finding(column, result)
                        .map(BindFinding::Scalar),
                );
            }
        }
        self.order(&dependencies, paths, false, findings)?;
        Ok(bound)
    }

    /// The root filter of a spec without row templates, which binding binds
    /// over the base's records once every column and the column order are
    /// bound (`root_filter::bind`): its grammar, then each name it reads,
    /// once, in written order. A bare name fails, since no column has a value
    /// before the rows are filtered (ROW-9), and so does a field of any
    /// dataset but the base (NAME-3); without source schemas each field of
    /// the base it names is stored.
    fn root_filter(
        &self,
        filter: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let start = findings.len();
        self.grammar(filter, findings)?;
        let (
            true,
            ExprKind::Formula(ast::Formula {
                compiled: Ok(plan),
                text,
                ..
            }),
        ) = (findings.len() == start, &spec.expr(filter).kind)
        else {
            return Ok(());
        };
        let at = render(spec, text.meta);
        let base = self.prepared.source().name.as_str();
        let mut seen = BTreeSet::new();
        for identifier in plan.identifiers().filter(|&name| seen.insert(name)) {
            match identifier.split_once('.') {
                None => findings.push(BindFinding::Predicate(Diagnostic {
                    code: C::RootFilterPhaseBoundary,
                    spec_paths: vec![at.clone()],
                    context: [
                        ("identifier", identifier),
                        ("available_phase", "column_derivation"),
                        ("required_phase", "row_filter"),
                    ]
                    .into_iter()
                    .map(|(name, text)| (name.into(), crate::json::Value::String(text.into())))
                    .collect(),
                    source_span: None,
                    operand_route: None,
                })),
                Some((dataset, _)) if dataset != base => {
                    findings.push(BindFinding::UnknownReference {
                        path: at.clone(),
                        name: identifier.into(),
                    })
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// With row templates: each template's filter and row-phase derivations,
    /// then each column-phase derivation for the rows of each of its
    /// bindings in turn, as binding binds them (`rows::Templates::bind`):
    /// each derivation's defects and then its nodes in binding's order,
    /// scoped to the rows it evaluates on ([`Self::walk`]); a template's
    /// filter reads after its derivations. Then the static types of each
    /// flag and `case` predicate and the conversion of each derivation that
    /// bound, each template's derivations first; then each template's cycles
    /// and the column phase's order. Returns what bound.
    fn rows(
        &self,
        rows: &rows::Templates,
        findings: &mut Vec<BindFinding>,
    ) -> Result<TemplatesBound, BindError> {
        let spec = self.spec;
        let refused = &self.refused;
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
        let mut bound = TemplatesBound {
            columns: Vec::new(),
            templates: Vec::new(),
            derived: Vec::new(),
        };
        let mut graphs = Vec::new();
        // Whether each derivation, each template's and then the column
        // phase's, bound without a finding in some binding: only such a
        // derivation states its result type (`rows::bind_assignment`).
        let mut converts = Vec::new();
        for (index, template) in phases.iter().enumerate() {
            let declared = &spec.rows.templates[index];
            let reads = super::boundaries::Reads::row(
                spec,
                self.prepared,
                &self.declared,
                template,
                refused,
            );
            let filter = declared
                .filter
                .map(|filter| (filter, format!("{}.filter", render(spec, declared.meta))));
            if let Some((filter, at)) = &filter {
                self.grammar(*filter, findings)?;
                reads.grouped_filter(*filter, at, findings);
            }
            // The scope binding binds the template's derivations in.
            let bound_scope = template.scope();
            let scope = Scope {
                drivers: std::slice::from_ref(&bound_scope.driver),
                row: Some(template.id),
                phase: Some(bound_scope.phase),
                available: bound_scope.available,
            };
            let mut clean = Vec::new();
            for derivation in &template.derivations {
                let start = findings.len();
                self.defects(derivation.expr, findings)?;
                self.walk(&reads, scope, derivation.expr, findings)?;
                let read = reads.bound();
                clean.push(read && findings.len() == start);
            }
            converts.extend(clean.iter().copied());
            bound.derived.push(
                template
                    .derivations
                    .iter()
                    .zip(&clean)
                    .filter(|&(_, &clean)| clean)
                    .map(|(derivation, _)| derivation.expr)
                    .collect(),
            );
            // The derivations that bound, in the order they complete.
            let (graph, paths) = self.template_graph(template);
            let order = crate::dependency_analysis::analyze(&graph)
                .map_err(|_| BindError::Internal)?
                .order;
            let completed = order
                .into_iter()
                .filter_map(|column| {
                    template
                        .derivations
                        .iter()
                        .zip(&clean)
                        .find(|&(derivation, &clean)| derivation.column == column && clean)
                        .map(|(derivation, _)| (derivation.column, derivation.expr))
                })
                .collect::<Vec<_>>();
            let filtered = filter.as_ref().is_some_and(|(filter, at)| {
                reads.filter(*filter, at, reads.stages(&completed).as_deref(), findings)
            });
            bound.templates.push((
                completed.into_iter().map(|(_, root)| root).collect(),
                filtered,
            ));
            graphs.push((graph, paths));
        }
        // A column that holds an aggregate binds once over the whole dataset
        // through the primary input, whose record its rows share only when
        // every row template is driven by it (`rows::Templates::bind`).
        let shared = phases.iter().all(|template| template.driver == primary);
        let owner = |binding: &rows::ColumnPhaseBinding<'_>| {
            matches!(binding.scope, crate::dataset::ColumnScope::Template) || shared
        };
        for derivation in &columns {
            let root = derivation.expr;
            let bindings = rows
                .column_bindings(derivation.column, lookups::aggregates(spec, root), primary)
                .into_iter()
                .map(|binding| (binding.driver, owner(&binding)))
                .collect::<Vec<_>>();
            let start = findings.len();
            self.defects(root, findings)?;
            let declared = findings.len() == start;
            // Every driver binds the whole derivation for its own rows, one
            // binding after another, and a failure several bindings share is
            // reported once.
            let mut read = true;
            let mut clean = false;
            for (driver, owned) in &bindings {
                let drivers = std::slice::from_ref(driver);
                let scope = Scope {
                    drivers,
                    row: None,
                    phase: Some(Phase::Column {
                        groups: &column_groups,
                    }),
                    available: None,
                };
                let reads = super::boundaries::Reads::column(
                    spec,
                    self.prepared,
                    &self.declared,
                    drivers,
                    *owned,
                    &column_groups,
                    refused,
                );
                let before = findings.len();
                self.walk(&reads, scope, root, findings)?;
                let bound = reads.bound();
                read &= bound;
                clean |= declared && bound && findings.len() == before;
            }
            if bindings.len() > 1 {
                rows::distinct_findings(findings, start, self.prepared.source());
            }
            bound.columns.push(read && findings.len() == start);
            converts.push(clean);
        }
        for (derivation, converts) in phases
            .iter()
            .flat_map(|template| &template.derivations)
            .chain(&columns)
            .zip(converts)
        {
            if !converts {
                continue;
            }
            let start = findings.len();
            let root = derivation.expr;
            self.predicate_types(root, findings);
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
        for (graph, paths) in graphs {
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
        let width = spec.columns.len();
        let mut dependencies = vec![None; width];
        let mut paths = vec![String::new(); width];
        for derivation in &columns {
            let root = derivation.expr;
            let mut edges = Vec::new();
            for binding in
                rows.column_bindings(derivation.column, lookups::aggregates(spec, root), primary)
            {
                let record = owner(&binding).then_some(binding.driver);
                for edge in self.edges(root, record) {
                    if !edges.contains(&edge) {
                        edges.push(edge);
                    }
                }
            }
            dependencies[derivation.column] = Some(edges);
            paths[derivation.column] = path(spec, root);
        }
        self.order(&dependencies, paths, true, findings)?;
        Ok(bound)
    }

    /// A row template's graph of what its row-phase derivations read of its
    /// own row-phase columns, with each derivation's path.
    fn template_graph(&self, template: &rows::TemplatePhase<'_>) -> (Vec<Vec<usize>>, Vec<String>) {
        let width = self.spec.columns.len();
        let mut graph = vec![Vec::new(); width];
        let mut paths = vec![String::new(); width];
        for derivation in &template.derivations {
            graph[derivation.column] = self
                .edges(derivation.expr, Some(template.driver))
                .into_iter()
                .filter(|&column| template.derivations.iter().any(|d| d.column == column))
                .collect();
            paths[derivation.column] = path(self.spec, derivation.expr);
        }
        (graph, paths)
    }

    /// The input whose records build the rows of a template over `driver`:
    /// the input itself, or the input a driver intermediate reads.
    fn driver_input<'s>(&'s self, driver: &'s str) -> Option<&'s str> {
        let prepared = self.prepared;
        if prepared.sources.iter().any(|source| source.name == driver) {
            return Some(driver);
        }
        prepared
            .intermediates
            .input(driver)
            .and_then(|index| prepared.sources.get(index))
            .map(|source| source.name.as_str())
    }

    /// The scopes binding admits once the rows are bound
    /// ([`super::scopes`]): each matched aggregate's key values for the rows
    /// that evaluate it, then each lookup the rows read, over the reads that
    /// bound. A dataset verification reads its lookups for every row.
    fn scopes(&self, bound: &Bound, findings: &mut Vec<BindFinding>) {
        use super::scopes::{Reading, Scopes};
        let spec = self.spec;
        let prepared = self.prepared;
        let scopes = Scopes {
            spec,
            prepared,
            refused: &self.refused,
            underived: &self.underived,
        };
        let verified = relation_predicates::verification_predicates(&prepared.verifications.checks)
            .flat_map(crate::bound_expression::BoundPredicate::bindings)
            .filter(|binding| {
                matches!(
                    binding.read,
                    crate::bound_expression::Read::Intermediate { .. }
                )
            })
            .map(|binding| binding.name.as_str())
            .collect::<Vec<_>>();
        match (bound, &prepared.rows) {
            (Bound::Implicit(columns), _) => {
                let base = prepared.source().name.as_str();
                let derivations = spec
                    .columns
                    .iter()
                    .zip(columns)
                    .filter(|&(_, &bound)| bound)
                    .filter_map(|(column, _)| column.derivation.as_ref())
                    .map(|derivation| derivation.expr)
                    .collect::<Vec<_>>();
                let contexts = derivations
                    .iter()
                    .map(|&root| (root, vec![base]))
                    .collect::<Vec<_>>();
                scopes.record_keys(&contexts, findings);
                let reading = Reading {
                    row: None,
                    input: base,
                    groups: None,
                    derivations,
                    names: verified,
                };
                scopes.lookup_reads(&[reading], findings);
            }
            (Bound::Templates(bound), rows::Rows::Templates(rows)) => {
                let primary = prepared.source().name.as_str();
                let phases = rows.phases();
                let inputs = phases
                    .iter()
                    .map(|template| self.driver_input(template.driver))
                    .collect::<Vec<_>>();
                // Each column-phase derivation evaluates for the rows of the
                // templates that leave its column to this phase.
                let completing = rows
                    .column_phase()
                    .iter()
                    .map(|derivation| {
                        let templates = rows
                            .column_bindings(
                                derivation.column,
                                lookups::aggregates(spec, derivation.expr),
                                primary,
                            )
                            .into_iter()
                            .flat_map(|binding| binding.templates)
                            .collect::<Vec<_>>();
                        (derivation.expr, templates)
                    })
                    .zip(&bound.columns)
                    .filter(|&(_, &bound)| bound)
                    .map(|(completing, _)| completing)
                    .collect::<Vec<_>>();
                let contexts = completing
                    .iter()
                    .map(|(root, templates)| {
                        let drivers = templates
                            .iter()
                            .filter_map(|&template| inputs[template])
                            .collect::<Vec<_>>();
                        (*root, drivers)
                    })
                    .collect::<Vec<_>>();
                scopes.record_keys(&contexts, findings);
                let readings = phases
                    .iter()
                    .enumerate()
                    .map(|(index, template)| {
                        let (own, filtered) = &bound.templates[index];
                        let mut derivations = own.clone();
                        derivations.extend(
                            completing
                                .iter()
                                .filter(|(_, templates)| templates.contains(&index))
                                .map(|&(root, _)| root),
                        );
                        // An ungrouped filter that bound reads its lookups.
                        let mut names = spec.rows.templates[index]
                            .filter
                            .filter(|_| *filtered)
                            .and_then(|filter| match &spec.expr(filter).kind {
                                ExprKind::Formula(ast::Formula {
                                    compiled: Ok(plan), ..
                                }) => Some(plan),
                                _ => None,
                            })
                            .into_iter()
                            .flat_map(|plan| plan.identifiers())
                            .filter(|name| {
                                name.split_once('.').is_some_and(|(owner, _)| {
                                    owner != template.driver
                                        && prepared.intermediates.contains(owner)
                                })
                            })
                            .collect::<Vec<_>>();
                        names.extend(&verified);
                        Reading {
                            row: Some((template.id, template.driver)),
                            input: inputs[index].unwrap_or(template.driver),
                            groups: template.groups.as_deref(),
                            derivations,
                            names,
                        }
                    })
                    .collect::<Vec<_>>();
                scopes.lookup_reads(&readings, findings);
            }
            (Bound::Templates(_), rows::Rows::Implicit(_)) => {}
        }
    }

    /// LOOKUP-9 from the declarations alone: a range value qualified by an
    /// input that does not drive every row reading the lookup fails, whether
    /// or not the input stores the field. Each context pairs what its rows
    /// evaluate with the inputs driving them and the named intermediates that
    /// select their records, in binding's order, over the derivations and
    /// filters that bound (`lookup_range::admit`); each refused range is
    /// reported once, and a refusal already reported is not repeated. On a
    /// 1.0 tree a named intermediate's bare value that no column takes, which
    /// stays contextual when another lookup's derivations read it, names no
    /// value of a row either (LOOKUP-9); a value qualified by a named
    /// intermediate that builds one of the rows is unsupported; and one
    /// qualified by a row driver that builds none of them is a keyed lookup
    /// of a lookup that declares no key (LOOKUP-3, ROW-2).
    fn ranges(&self, bound: &Bound, findings: &mut Vec<BindFinding>) -> Result<(), BindError> {
        let spec = self.spec;
        let named = &self.prepared.intermediates;
        let inputs = &self.prepared.sources;
        let one_meaning = spec.header.surface.one_meaning_per_form();
        let refused = |value: &str, drivers: &[(&str, Option<&str>)]| {
            value.split_once('.').is_some_and(|(qualifier, _)| {
                inputs.iter().any(|input| input.name == qualifier)
                    && drivers.iter().any(|&(driver, _)| driver != qualifier)
            })
        };
        // A value qualified by a 1.0 row driver that builds none of the rows
        // reads it as a keyed lookup: its qualifier and the `key` it lacks.
        fn keyless<'v, 'n>(
            named: &'n intermediates::Declarations,
            value: &'v str,
        ) -> Option<(&'v str, &'n str)> {
            let (qualifier, _) = value.split_once('.')?;
            let (index, _) = named.reference(value)?;
            Some((qualifier, named.key_required(index)?))
        }
        let keyless = |value| keyless(named, value).filter(|_| one_meaning);
        let key_required = |key: &str| {
            BindFinding::Lookup(LookupFinding::from_diagnostic(lookup::key_required(key)))
        };
        let mut seen = BTreeSet::new();
        let mut push = |path: &str, finding: BindFinding, findings: &mut Vec<BindFinding>| {
            if seen.insert(path.to_owned())
                && !crate::specification::row_intermediates::reported(findings, &finding)
            {
                findings.push(finding);
            }
        };
        for (evaluated, drivers) in self.range_contexts(bound)? {
            for evaluated in evaluated {
                let (ranges, names, own) = match evaluated {
                    Evaluated::Expression(root, own) => {
                        let (ranges, names) = self.lookups(root, own);
                        (ranges, names, own)
                    }
                    Evaluated::Name(name) => (Vec::new(), vec![name], None),
                };
                for (value, path, owner) in ranges {
                    if refused(value, &drivers) {
                        let finding = BindFinding::Scalar(lookup::range_reference(
                            &format!("{path}.value"),
                            owner,
                            value,
                        ));
                        push(&path, finding, findings);
                    } else if let Some((_, key)) =
                        keyless(value).filter(|&(qualifier, _)| own != Some(qualifier))
                    {
                        push(&path, key_required(key), findings);
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
                    let value = range.value.as_str();
                    let contextual = one_meaning
                        && !value.contains('.')
                        && !self.column(value)
                        && spec
                            .intermediates
                            .iter()
                            .position(|item| item.id.name == identifier)
                            .is_some_and(|position| donor_read(spec, position));
                    let driving = value.split_once('.').is_some_and(|(qualifier, _)| {
                        one_meaning
                            && named.reference(value).is_some()
                            && drivers.iter().any(|&(_, driver)| driver == Some(qualifier))
                    });
                    let finding = if refused(value, &drivers) || contextual {
                        Some(BindFinding::Scalar(
                            range.unknown_value(Owner::Intermediate(identifier)),
                        ))
                    } else if driving {
                        Some(BindFinding::Scalar(crate::application_issue::unsupported(
                            "intermediate_driver_range_value",
                            Some(&format!("{}.value", range.path())),
                        )))
                    } else {
                        keyless(value).map(|(_, key)| key_required(key))
                    };
                    if let Some(finding) = finding {
                        push(range.path(), finding, findings);
                    }
                    if let Some((chained, _)) = named.reference(value) {
                        stack.push(chained);
                    }
                }
            }
        }
        Ok(())
    }

    /// The contexts whose rows read lookup ranges, over what `bound` says
    /// bound. Without row templates the base drives every row, and its
    /// qualifier reads the row's own record. A dataset verification reads its
    /// lookups for every row, as a column does. With row templates, the
    /// column phase, whose derivations and dataset verifications evaluate for
    /// the rows of every template that leaves them to it, then each template,
    /// whose derivations and filter evaluate for its rows only; a row's driver
    /// is the input behind its template, named directly or through a named
    /// intermediate.
    #[allow(clippy::type_complexity)]
    fn range_contexts(
        &self,
        bound: &Bound,
    ) -> Result<Vec<(Vec<Evaluated<'_>>, Vec<(&str, Option<&str>)>)>, BindError> {
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
        let (rows::Rows::Templates(rows), Bound::Templates(bound)) = (&prepared.rows, bound) else {
            let base = prepared.source().name.as_str();
            let columns = match bound {
                Bound::Implicit(columns) => columns.as_slice(),
                Bound::Templates(_) => &[],
            };
            let evaluated = spec
                .columns
                .iter()
                .zip(columns)
                .filter(|&(_, &bound)| bound)
                .filter_map(|(column, _)| column.derivation.as_ref())
                .map(|derivation| Evaluated::Expression(derivation.expr, Some(base)))
                .chain(verified)
                .collect();
            return Ok(vec![(evaluated, vec![(base, None)])]);
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
                    Some((template.driver, None))
                } else {
                    prepared
                        .intermediates
                        .input(template.driver)
                        .and_then(|index| prepared.sources.get(index))
                        .map(|source| (source.name.as_str(), Some(template.driver)))
                }
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(Vec::new());
        };
        let root = |expr: ExprId| Ok::<_, BindError>(expr);
        let mut contexts = Vec::new();
        // A column derivation evaluates only for the rows of the templates that
        // leave the column to this phase (LOOKUP-9), and every driver binds it
        // for its own rows. A column holding an aggregate binds once against
        // the whole dataset, where every input joins unless every row template
        // is driven by the input behind the first.
        for (derivation, _) in rows
            .column_phase()
            .into_iter()
            .zip(&bound.columns)
            .filter(|&(_, &bound)| bound)
        {
            let root = root(derivation.expr)?;
            let leaving = phases.iter().zip(&drivers).filter(|(template, _)| {
                !template
                    .derivations
                    .iter()
                    .any(|derived| derived.column == derivation.column)
            });
            let mut scopes: Vec<(Option<&str>, Vec<(&str, Option<&str>)>)> = Vec::new();
            if lookups::aggregates(spec, root) {
                let own = drivers
                    .first()
                    .map(|&(primary, _)| primary)
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
            for derivation in template
                .derivations
                .iter()
                .filter(|derivation| bound.derived[index].contains(&derivation.expr))
            {
                evaluated.push(Evaluated::Expression(
                    root(derivation.expr)?,
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
                .filter(|_| bound.templates[index].1)
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

/// On a 1.0 tree, the range value of the implicit join a structured source at
/// `e` reads, when the value is a bare name that no column takes: a bare name
/// is an output column alone (NAME-3), so it names nothing the row holds and
/// binding refuses it at `between.value` (LOOKUP-9), naming the variable.
pub(super) fn join_range(spec: &Spec, e: ExprId) -> Option<BindFinding> {
    let ExprKind::Source(source) = &spec.expr(e).kind else {
        return None;
    };
    let between = source
        .between
        .as_ref()
        .filter(|between| between.lower.is_some() || between.upper.is_some())
        .filter(|_| spec.header.surface.one_meaning_per_form())?;
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

/// On a 1.0 tree, the range at `between` of a lookup of `relation`, owned
/// by `owner`, whose value and stated bounds the declarations type and whose
/// types cannot compare (`incomparable_range_types`, LOOKUP-9), as binding
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
    if !spec.header.surface.one_meaning_per_form() {
        return None;
    }
    let declared = prepared.declared_types();
    let value = name(spec, between.value)?;
    let typed = match value.split_once('.') {
        None => spec.columns.iter().any(|column| column.name.name == value),
        Some((owner, _)) => prepared.sources.iter().any(|input| input.name == owner),
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

/// On a 1.0 tree, what binding refuses of the range of the implicit join a
/// structured source at `e` reads, once its read binds: a bare value that no
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
                &[("identifier", variable)],
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
