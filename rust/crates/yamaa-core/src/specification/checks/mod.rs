//! Preflight and `check()` over the typed tree (#2844, design section 3).
//!
//! The root checks read the tree's typed fields ([`root`]). Every other
//! check runs once per node kind: [`ast::Spec::sites`] names the roots, each
//! check keeps the site filter it has always had (rounding, for example,
//! skips a spec with row templates), a walk from each root finds the nodes
//! of its kind, and [`DeclaredTypes`] types what they read before any schema
//! is captured.
//!
//! `check()` and `run()` run the same checks. A check that refuses a spec
//! before binding ([`verbs`], [`odm`], [`lookups`]) refuses it in both. The
//! declaration findings ([`declared`]) are what `check()` reports; `run()`
//! reports each of them from binding, in binding's order, together with
//! what only the captured schemas decide. Among them are what binding
//! decides of each read of a row template or the column phase
//! ([`boundaries`]), of each read of a lookup's own derivations or an
//! aggregate's derive bindings ([`donors`]), and the scopes it admits once
//! the rows are bound ([`scopes`]).
//!
//! A check that needs an operation's executor type constructs it from the
//! tree, as binding does.

pub(super) mod boundaries;
pub(super) mod declared;
pub(super) mod donors;
pub(super) mod lookups;
pub(super) mod odm;
pub(super) mod root;
pub(super) mod scopes;
pub(super) mod verbs;

use super::*;
use crate::ast::{self, ExprId, ExprKind, JoinKey, Spec};

/// A node's path, as the spec's diagnostics cite it.
pub(in crate::specification) fn path(spec: &Spec, e: ExprId) -> String {
    spec.cite(spec.expr(e).meta)
}

/// A node's written position, as the spec's diagnostics cite it.
pub(in crate::specification) fn render(spec: &Spec, meta: ast::Meta) -> String {
    spec.cite(meta)
}

/// The operand of a verb in `slot`.
pub(in crate::specification) fn operand(
    operands: &[ast::Operand],
    slot: ast::Slot,
) -> Option<ExprId> {
    operands
        .iter()
        .find(|operand| operand.slot == slot)
        .map(|operand| operand.expr)
}

/// The name a verb's operand in `slot` reads.
pub(in crate::specification) fn operand_name<'s>(
    spec: &'s Spec,
    operands: &[ast::Operand],
    slot: ast::Slot,
) -> Option<&'s str> {
    operand(operands, slot).and_then(|e| name(spec, e))
}

/// A written handler's value, refused at its own path as preparation
/// refuses any written value it cannot hold.
pub(in crate::specification) fn handler(
    spec: &Spec,
    handler: Option<&ast::Constant>,
) -> Result<Option<crate::value::Value>, PrepareError> {
    handler
        .map(|handler| constant(&handler.value, &render(spec, handler.meta)))
        .transpose()
}

/// Whether the node at `meta` is written below the spec's field `field`,
/// such as `rows` for a row template's own derivation.
pub(in crate::specification) fn under(spec: &Spec, meta: ast::Meta, field: &str) -> bool {
    let mut at = meta.path;
    while let Some(parent) = spec.paths.parent(at) {
        if spec.paths.parent(parent).is_none() {
            return matches!(spec.paths.segment(at), ast::Segment::Field(name) if *name == field);
        }
        at = parent;
    }
    false
}

/// The derivation positions at and below `root`: the root, then each case
/// result in written order, depth first. These are the only places a
/// verb, a window or an aggregate nests (EXPR-3).
pub(super) fn positions(spec: &Spec, root: ExprId) -> Vec<ExprId> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        found.push(e);
        if let ExprKind::Case(arms) = &spec.expr(e).kind {
            stack.extend(arms.iter().rev().map(|arm| arm.result));
        }
    }
    found
}

/// The current-row values each aggregate at a position below `root` pairs
/// with a donor column.
pub(super) fn aggregate_key_values(spec: &Spec, root: ExprId) -> Vec<ExprId> {
    positions(spec, root)
        .into_iter()
        .filter_map(|e| match &spec.expr(e).kind {
            ExprKind::Aggregate(aggregate) => match &aggregate.key {
                Some(JoinKey::Pairs { pairs, .. }) => {
                    Some(pairs.iter().map(|&(_, value)| value).collect::<Vec<_>>())
                }
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .collect()
}

/// Every key value a verb check visits besides the column and row
/// derivations: the aggregates' key values in column defaults, then in row
/// overrides, then each intermediate's own key values and those of the
/// aggregates in its derivations.
pub(super) fn key_sites(spec: &Spec) -> Vec<ExprId> {
    let mut sites = Vec::new();
    for column in &spec.columns {
        if let Some(derivation) = &column.derivation {
            sites.extend(aggregate_key_values(spec, derivation.expr));
        }
    }
    for template in &spec.rows.templates {
        for (_, derivation) in &template.overrides {
            sites.extend(aggregate_key_values(spec, derivation.expr));
        }
    }
    for item in &spec.intermediates {
        if let Some(JoinKey::Pairs { pairs, .. }) = &item.key {
            sites.extend(pairs.iter().map(|&(_, value)| value));
        }
        for binding in &item.bindings {
            sites.extend(aggregate_key_values(spec, binding.derivation.expr));
        }
    }
    sites
}

/// Whether the spec builds its rows with row templates.
pub(super) fn has_rows(spec: &Spec) -> bool {
    !spec.rows.templates.is_empty()
}

/// The driver without row templates: `base`, or the one declared input.
pub(super) fn default_driver(spec: &Spec) -> Option<&str> {
    if let Some(base) = &spec.rows.base {
        return Some(&base.name);
    }
    match spec.inputs.as_slice() {
        [input] => Some(&input.name.name),
        _ => None,
    }
}

/// A row template's driver: its `dataset`, or the one declared input.
pub(super) fn template_driver<'a>(spec: &'a Spec, template: &'a ast::Template) -> Option<&'a str> {
    match &template.driver {
        Some(driver) => Some(&driver.name),
        None => match spec.inputs.as_slice() {
            [input] => Some(&input.name.name),
            _ => None,
        },
    }
}

/// Whether the input `owner` declares no `key` on a 1.0 tree, where an
/// implicit join of it, and a qualified aggregate over it without `key` of
/// its own, match on that declared key (D23, D63): such a read has nothing to
/// match on, whatever the input stores (D111). A 0.5 join infers its keys
/// from the captured schemas.
pub(super) fn keyless(spec: &Spec, owner: &str) -> bool {
    !spec.header.surface.infers_join_keys() && spec.input_key(owner).is_none_or(<[_]>::is_empty)
}

/// The refusal at `path` of a read of the keyless 1.0 input `dataset` through
/// its implicit join (`no_applicable_keys`, LOOKUP-5, D111), as binding
/// reports it ([`crate::reference_binding::JoinedFinding::key_diagnostic`]).
pub(super) fn undeclared_key(path: &str, dataset: &str) -> BindFinding {
    BindFinding::Source(source_expressions::SourceFinding::from_diagnostic(
        crate::diagnostic::lookup::undeclared_input_key(path, dataset, false),
    ))
}

/// A name a flag or `case` predicate cannot read there (`unknown_field`,
/// FORMULA-9).
pub(super) fn predicate_unknown(path: &str, name: &str) -> BindFinding {
    BindFinding::Predicate(crate::diagnostic::Diagnostic {
        code: crate::diagnostic::ConditionCode::PredicateUnknownField,
        spec_paths: vec![path.into()],
        context: [("identifier".into(), crate::json::Value::String(name.into()))].into(),
        source_span: None,
        operand_route: None,
    })
}

/// The refusal of the value of the range at `between`, owned by `owner`,
/// which reads `value`, a field of a lookup binding does not bind
/// (`unknown_field`, LOOKUP-9), as `lookup_range::Declaration::unknown_value`
/// reports it.
pub(super) fn unknown_range_value(
    spec: &Spec,
    between: &ast::Between,
    owner: crate::diagnostic::lookup::Owner<'_>,
    value: &str,
) -> BindFinding {
    BindFinding::Scalar(crate::diagnostic::lookup::range_reference(
        &format!("{}.value", render(spec, between.meta)),
        owner,
        value,
    ))
}

/// What a 1.0 implicit join pairs each column of its input's declared `key`
/// with (LOOKUP-1, LOOKUP-2).
#[derive(Clone, Copy)]
pub(super) enum Pairing<'a> {
    /// During row construction, the same-named field of the driver's record,
    /// read in the row template's scope.
    Row {
        driver: &'a str,
        row: &'a str,
        group_by: Option<&'a [&'a str]>,
    },
    /// Otherwise, the current row's same-named column.
    Column,
}

/// Whether the input `owner` stores `field`, as far as the declarations
/// decide: a linked artifact stores exactly its producer's output columns
/// (INPUT-10, INPUT-11) and a generated ordinal (INPUT-3). No declaration
/// lists another input's fields, so each named one counts as stored.
pub(super) fn stores(prepared: &PreparedSpecification, owner: &str, field: &str) -> bool {
    let Some(index) = prepared
        .sources
        .iter()
        .position(|source| source.name == owner)
    else {
        return true;
    };
    prepared.sources[index].ordinal.as_deref() == Some(field)
        || prepared
            .source_contract(index)
            .is_none_or(|contract| contract.fields().iter().any(|stored| stored.name == field))
}

/// The declared type of the input `owner`'s field `field`: a generated
/// ordinal's `int` (INPUT-3), or as `declared` types any other.
pub(super) fn stored_type(
    prepared: &PreparedSpecification,
    declared: &types::DeclaredTypes<'_>,
    owner: &str,
    field: &str,
) -> crate::result_type::ResultType {
    if prepared
        .sources
        .iter()
        .any(|source| source.name == owner && source.ordinal.as_deref() == Some(field))
    {
        return crate::result_type::ResultType::column(ColumnType::Int);
    }
    declared.name(&format!("{owner}.{field}"))
}

/// What binding decides at `path` of the declared `key` of the 1.0 input
/// `owner` that an implicit join there matches on (D111 item 2), for each
/// column in declared order (`key_relation::declared`,
/// `Catalog::joined_read`): the input does not store it, as far as
/// [`stores`] decides (`unknown_field`, LOOKUP-5, identifier
/// `<input>.<column>`); the specification declares no column of it
/// (LOOKUP-5, identifier `<column>`); during row construction, the driver
/// does not store its same-named field, or a grouped template does not
/// group by it (ROW-5); and the pair's declared types cannot compare
/// (`incompatible_input_type`, LOOKUP-9). The first such finding, at which
/// binding stops.
pub(super) fn declared_key(
    spec: &Spec,
    prepared: &PreparedSpecification,
    declared: &types::DeclaredTypes<'_>,
    path: &str,
    owner: &str,
    pairing: Pairing<'_>,
) -> Option<BindFinding> {
    use crate::diagnostic::lookup::{self, Owner};
    use crate::result_type::ResultType;
    if spec.header.surface.infers_join_keys() {
        return None;
    }
    let source = |diagnostic| {
        Some(BindFinding::Source(
            source_expressions::SourceFinding::from_diagnostic(diagnostic),
        ))
    };
    for column in spec.input_key(owner)? {
        let field = column.name.as_str();
        if !stores(prepared, owner, field) {
            return source(lookup::donor_key_reference(
                path,
                Owner::Unnamed,
                &format!("{owner}.{field}"),
            ));
        }
        let Some(output) = spec.columns.iter().find(|column| column.name.name == field) else {
            return source(lookup::match_value_reference(path, Owner::Unnamed, field));
        };
        let expected = match pairing {
            Pairing::Row {
                driver,
                row,
                group_by,
            } => {
                let identifier = format!("{driver}.{field}");
                let finding = if !stores(prepared, driver, field) {
                    Some(crate::reference_scope::Finding::UnknownField)
                } else {
                    crate::reference_scope::group_finding(
                        &identifier,
                        crate::reference_scope::Phase::Row { group_by },
                    )
                };
                if let Some(finding) = finding {
                    return Some(BindFinding::QualifiedReference {
                        path: path.into(),
                        name: identifier,
                        row: Some(row.into()),
                        finding,
                    });
                }
                driver_type(spec, prepared, declared, driver, field)
            }
            Pairing::Column => ResultType::column(output.kind),
        };
        let (Some(expected), Some(actual)) = (
            expected.column_type(),
            stored_type(prepared, declared, owner, field).column_type(),
        ) else {
            continue;
        };
        if !ResultType::column(expected).comparable(actual.into()) {
            return source(lookup::key_type(
                path,
                Owner::Unnamed,
                field,
                ResultType::column(expected),
                ResultType::column(actual),
            ));
        }
    }
    None
}

/// The declared type of the field `field` of a row template's driver: an
/// input's stored field, or that of the input a driver lookup reads, unless
/// the lookup derives it or leaves it out.
fn driver_type(
    spec: &Spec,
    prepared: &PreparedSpecification,
    declared: &types::DeclaredTypes<'_>,
    driver: &str,
    field: &str,
) -> crate::result_type::ResultType {
    let Some(item) = spec
        .intermediates
        .iter()
        .find(|item| item.id.name == driver)
    else {
        return stored_type(prepared, declared, driver, field);
    };
    let derived = item
        .bindings
        .iter()
        .any(|binding| binding.name.name == field);
    let listed = item
        .columns
        .as_ref()
        .is_none_or(|columns| columns.iter().any(|column| column.name == field));
    if derived || !listed {
        return crate::result_type::ResultType::UNKNOWN;
    }
    stored_type(prepared, declared, &item.dataset.name, field)
}

/// The name a reference reads.
pub(super) fn name(spec: &Spec, e: ExprId) -> Option<&str> {
    match &spec.expr(e).kind {
        ExprKind::Ref(name) => Some(name.as_str()),
        _ => None,
    }
}

/// The variable a source operand reads: a reference, or a structured
/// source binding's variable.
pub(super) fn variable(spec: &Spec, e: ExprId) -> Option<&str> {
    match &spec.expr(e).kind {
        ExprKind::Ref(name) => Some(name.as_str()),
        ExprKind::Source(source) => name(spec, source.variable),
        _ => None,
    }
}

/// A written type's name.
pub(super) fn type_name(kind: ColumnType) -> &'static str {
    match kind {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}

/// Whether another lookup's derivations read the lookup at `index`: binding
/// then binds its match per reading context, for each row and for each donor
/// record that reads it (LOOKUP-16).
pub(super) fn donor_read(spec: &Spec, index: usize) -> bool {
    let item = &spec.intermediates[index];
    spec.intermediates
        .iter()
        .enumerate()
        .any(|(reader, other)| {
            other.bindings.iter().any(|binding| {
                spec.reads(binding.derivation.expr, ast::Scope::Donor(reader))
                    .iter()
                    .any(|read| read.name.owner() == Some(item.id.name.as_str()))
            })
        })
}

/// Whether a match value's reference `name` reads no value of the current
/// row whatever an input stores (LOOKUP-5): a bare name that no column takes
/// (NAME-3), or a name qualified by a lookup, by `SELF` or by nothing
/// declared. Only an input's field is left for its records to decide.
pub(super) fn unknown_match_value(
    spec: &Spec,
    prepared: &PreparedSpecification,
    name: &str,
) -> bool {
    match name.split_once('.') {
        None => !spec.columns.iter().any(|column| column.name.name == name),
        Some((owner, _)) => !prepared.sources.iter().any(|input| input.name == owner),
    }
}
