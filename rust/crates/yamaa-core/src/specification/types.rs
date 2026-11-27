//! What the one result-type table ([`crate::ast::result_type`], EXPR-14)
//! reads besides the tree: each project call's declared return type, and
//! the type of each name a site reads, from [`DeclaredTypes`] before any
//! schema is captured and from [`CapturedTypes`] after. VALUE-24 refuses a
//! result type its declared column type can never take.
use super::*;
use crate::{
    ast::{self, ExprId, Read, Scope, TypeEnv},
    diagnostic::{ConditionCode, Diagnostic},
    expression::TypeFinding,
    producer_contract::Contract,
    result_type::ResultType,
    value::ValueType,
};

/// The admitted spec's tree, which binding and result typing read, and
/// each project call's declared return type and admitted call.
#[derive(Debug)]
pub(super) struct Tree {
    spec: ast::Spec,
    calls: BTreeMap<ExprId, ColumnType>,
    indices: BTreeMap<ExprId, usize>,
}

impl Tree {
    pub(super) fn new(spec: ast::Spec, project: Option<&functions::Prepared>) -> Self {
        let (calls, indices) = project.map_or_else(Default::default, |project| {
            (project.returns.clone(), project.indices.clone())
        });
        Self {
            spec,
            calls,
            indices,
        }
    }
    pub(super) fn spec(&self) -> &ast::Spec {
        &self.spec
    }
    /// The admitted project call of the call expression `call`.
    pub(super) fn call(&self, call: ExprId) -> Result<usize, BindError> {
        self.indices.get(&call).copied().ok_or(BindError::Internal)
    }
    /// `names`, with each call typed by its definition.
    pub(super) fn env<'a>(&'a self, names: &'a dyn TypeEnv) -> Env<'a> {
        Env {
            names,
            calls: &self.calls,
        }
    }
    /// The result type of the derivation `root`, read in `scope`, over the
    /// names `names` types.
    pub(super) fn result_type(
        &self,
        root: ExprId,
        scope: Scope,
        names: &dyn TypeEnv,
    ) -> ResultType {
        ast::result_type(self.spec(), root, scope, &self.env(names))
    }
    /// The scope an intermediate's own derivations read in: its donor record.
    pub(super) fn donor(&self, intermediate: &str) -> Result<Scope, BindError> {
        self.spec()
            .intermediates
            .iter()
            .position(|item| item.id.name == intermediate)
            .map(Scope::Donor)
            .ok_or(BindError::Internal)
    }
}

/// A site's names with the tree's call types.
pub(super) struct Env<'a> {
    names: &'a dyn TypeEnv,
    calls: &'a BTreeMap<ExprId, ColumnType>,
}

impl TypeEnv for Env<'_> {
    fn read(&self, read: &Read) -> ResultType {
        self.names.read(read)
    }
    fn call(&self, call: ExprId) -> ResultType {
        self.calls
            .get(&call)
            .map_or(ResultType::UNKNOWN, |&returns| ResultType::column(returns))
    }
}

/// Names typed before capture, in preflight: an output column by its
/// declared type, and a stored field by its input's type authority. A
/// schema link's producer types every field of the artifact (INPUT-10),
/// whatever its storage profile. Otherwise `types` declares a field of a
/// delimited file, every field it leaves undeclared is `str`
/// (INPUT-5), and `types` narrows a field of a SAS or Transport file. Any
/// other name is not known until binding: an undeclared field of a
/// self-describing or SAS file has the type its captured schema gives
/// (EXPR-14).
#[derive(Default)]
pub(super) struct DeclaredTypes<'a> {
    columns: Vec<(&'a str, ColumnType)>,
    inputs: Vec<(&'a str, Stored<'a>)>,
}

/// Where an input's field types come from before capture.
enum Stored<'a> {
    Linked(&'a Contract),
    Typeless(&'a [(ast::Named, ColumnType)]),
    Narrowed(&'a [(ast::Named, ColumnType)]),
    Captured,
    /// A pair relation, read as an input is: each field is a derivation,
    /// typed when it declares its type (LOOKUP-27).
    Pairs(&'a [ast::Binding]),
}

impl<'a> DeclaredTypes<'a> {
    /// `contracts` holds each linked input's producer contract by input name.
    pub(super) fn new(
        spec: &'a ast::Spec,
        contracts: impl IntoIterator<Item = (&'a str, &'a Contract)>,
    ) -> Self {
        let contracts = contracts.into_iter().collect::<Vec<_>>();
        let inputs = spec
            .inputs
            .iter()
            .map(|input| {
                let name = input.name.name.as_str();
                let stored = match contracts.iter().find(|&&(linked, _)| linked == name) {
                    Some(&(_, contract)) => Stored::Linked(contract),
                    None => match input.source.profile {
                        Some(ast::Profile::Csv) => Stored::Typeless(&input.source.types),
                        // A SAS or Dataset-JSON container supplies each
                        // type, which `types` may narrow (SAS-3, DSJSON-2).
                        Some(
                            ast::Profile::Xport
                            | ast::Profile::Sas7bdat
                            | ast::Profile::DatasetJson
                            | ast::Profile::DatasetNdjson,
                        ) => Stored::Narrowed(&input.source.types),
                        _ => Stored::Captured,
                    },
                };
                (name, stored)
            })
            .chain(
                spec.intermediates
                    .iter()
                    .filter(|item| item.pairs())
                    .map(|item| (item.id.name.as_str(), Stored::Pairs(&item.bindings))),
            )
            .collect();
        let columns = spec
            .columns
            .iter()
            .map(|column| (column.name.name.as_str(), column.kind))
            .collect();
        Self { columns, inputs }
    }
    /// The type of `name`, a column or a qualified stored field.
    pub(super) fn name(&self, name: &str) -> ResultType {
        let Some((owner, field)) = name.split_once('.') else {
            return ResultType::declared(
                self.columns
                    .iter()
                    .find(|&&(column, _)| column == name)
                    .map(|&(_, kind)| kind),
            );
        };
        let declared = |types: &[(ast::Named, ColumnType)]| {
            types
                .iter()
                .find(|(declared, _)| declared.name == field)
                .map(|&(_, kind)| ResultType::column(kind))
        };
        match self.inputs.iter().find(|&&(input, _)| input == owner) {
            Some((_, Stored::Linked(contract))) => ResultType::declared(
                contract
                    .fields()
                    .iter()
                    .find(|stored| stored.name == field)
                    .map(|stored| stored.kind),
            ),
            Some((_, Stored::Typeless(types))) => declared(types).unwrap_or(ResultType::STR),
            Some((_, Stored::Narrowed(types))) => declared(types).unwrap_or(ResultType::UNKNOWN),
            Some((_, Stored::Pairs(bindings))) => bindings
                .iter()
                .find(|binding| binding.name.name == field)
                .and_then(|binding| binding.declared)
                .map_or(ResultType::UNKNOWN, ResultType::column),
            Some((_, Stored::Captured)) | None => ResultType::UNKNOWN,
        }
    }
}

impl TypeEnv for DeclaredTypes<'_> {
    fn read(&self, read: &Read) -> ResultType {
        self.name(read.name.as_str())
    }
}

impl PreparedSpecification {
    /// The declared types of this spec's names, its links' producers
    /// typing the linked inputs.
    pub(super) fn declared_types(&self) -> DeclaredTypes<'_> {
        DeclaredTypes::new(
            self.tree.spec(),
            self.sources
                .iter()
                .zip(&self.source_contracts)
                .filter_map(|(source, contract)| Some((source.name.as_str(), contract.as_ref()?))),
        )
    }
}

/// Names typed from the input schemas at a bound site: a field of a named
/// intermediate by its donor's type, anything else by the binding catalog.
/// No cells are read and no resolver is called.
#[derive(Clone, Copy)]
pub(super) struct CapturedTypes<'a> {
    donors: binder::Donors<'a>,
    catalog: &'a Catalog,
    spec: &'a ast::Spec,
}

impl<'a> CapturedTypes<'a> {
    pub(super) fn new(
        donors: binder::Donors<'a>,
        catalog: &'a Catalog,
        spec: &'a ast::Spec,
    ) -> Self {
        Self {
            donors,
            catalog,
            spec,
        }
    }
    fn origins(&self) -> FieldOrigins<'a> {
        FieldOrigins {
            spec: self.spec,
            inputs: self.donors.inputs,
            derived: None,
        }
    }
    pub(super) fn kind(&self, name: &str) -> Option<ValueType> {
        if let Some((index, field)) = self.donors.named.reference(name) {
            return self
                .donors
                .items
                .get(index)
                .and_then(Option::as_ref)
                .and_then(|item| {
                    let column = item.source_column(self.donors.secondary, field)?;
                    self.donors.kind(item, column).map(Into::into)
                });
        }
        self.catalog.type_of(name).map(Into::into)
    }
    /// The plan as this site types its names, so each `NULLIF` returns its
    /// result type (EXPR-14).
    pub(super) fn typed(&self, plan: &CompiledExpression) -> CompiledExpression {
        plan.typed(|name| ResultType::known(self.kind(name)))
    }
    /// Every static type fault of a predicate over this site's names
    /// (FORMULA-5, FORMULA-12), before any value is read
    /// ([`predicate_faults`]); true when there was one.
    pub(super) fn predicate(
        &self,
        plan: &CompiledExpression,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        self.predicate_over(plan, |_| true, findings)
    }
    /// [`Self::predicate`] of a predicate a row template evaluates over its
    /// records, where a field of an input that no declaration types is
    /// typed by its present values at run time: only the names whose type
    /// a declaration states are typed here.
    pub(super) fn declared_predicate(
        &self,
        plan: &CompiledExpression,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let inputs = self.donors.inputs;
        self.predicate_over(
            plan,
            |name| {
                name.split_once('.').is_none_or(|(relation, field)| {
                    inputs.schema(relation).is_none() || inputs.declares_type(relation, field)
                })
            },
            findings,
        )
    }
    /// [`Self::predicate`] over the names `typed` holds; any other name has
    /// no type here.
    fn predicate_over(
        &self,
        plan: &CompiledExpression,
        typed: impl Fn(&str) -> bool,
        findings: &mut Vec<BindFinding>,
    ) -> bool {
        let kinds = plan
            .identifiers()
            .map(|name| (name, self.kind(name).filter(|_| typed(name))))
            .collect::<BTreeMap<_, _>>();
        predicate_faults(
            plan,
            |name| kinds.get(name).copied().flatten(),
            self.origins(),
            findings,
        )
    }
    /// Every static type fault of a formula over the names `bindings` binds.
    pub(super) fn numeric(
        &self,
        plan: &CompiledExpression,
        bindings: &[Binding],
        findings: &mut Vec<BindFinding>,
    ) {
        let kinds = plan
            .identifiers()
            .filter(|name| bindings.iter().any(|binding| binding.name == *name))
            .map(|name| (name, self.kind(name)))
            .collect::<BTreeMap<_, _>>();
        findings.extend(
            numeric_kinds(
                plan,
                |name| kinds.get(name).copied().flatten(),
                self.origins(),
            )
            .into_iter()
            .map(BindFinding::Scalar),
        );
    }
}

impl TypeEnv for CapturedTypes<'_> {
    fn read(&self, read: &Read) -> ResultType {
        ResultType::known(self.kind(read.name.as_str()))
    }
}

/// Where an input field's type comes from, as a comparison fault's hint
/// reads it: the specification's input declarations, and the schemas
/// binding reads, which say whether an input stores a field.
#[derive(Clone, Copy)]
pub(super) struct FieldOrigins<'a> {
    pub spec: &'a ast::Spec,
    pub inputs: binder::Inputs<'a>,
    /// At a lookup filter rebased onto the donor record of the lookup that
    /// reads it: the donor's name, that lookup, and the declarations, so a
    /// field the lookup derives has its derivation's type.
    pub derived: Option<(&'a str, &'a str, &'a intermediates::Declarations)>,
}

impl FieldOrigins<'_> {
    /// Whether `name` is a field of an input that may or may not store it
    /// ([`binder::Inputs::decides`]), as `check()` reads a field only the
    /// specification names, and that no lookup [`Self::derived`] names
    /// derives.
    fn undecided(&self, name: &str) -> bool {
        name.split_once('.').is_some_and(|(owner, field)| {
            !self.inputs.decides(owner, field)
                && !self.derived.is_some_and(|(donor, lookup, named)| {
                    owner == donor && named.derives(lookup, field)
                })
        })
    }
    /// The hint for `name`, a bare name of a field that a delimited input,
    /// which no schema link types, stores and its `types` leave undeclared,
    /// so that the field reads as `str` (INPUT-5): the field, why it is
    /// text, and where its type is declared. A field the input is not
    /// known to store, as `check()` reads one no declaration states, has
    /// none.
    pub(super) fn untyped(&self, name: &str) -> Option<String> {
        let (owner, field) = name.split_once('.')?;
        let source = &self
            .spec
            .inputs
            .iter()
            .find(|input| input.name.name == owner)?
            .source;
        let untyped = matches!(source.profile, Some(ast::Profile::Csv))
            && source.schema.is_none()
            && !source
                .types
                .iter()
                .any(|(declared, _)| declared.name == field)
            && self.inputs.schema(owner)?.stores(field) == Some(true);
        untyped.then(|| {
            format!(
                "{name} is str because input {owner} declares no type for {field} in input.{owner}.types"
            )
        })
    }
}

/// Every static type fault of a numeric expression or predicate, with
/// `kind` giving each name's type. A comparison whose `str` operand is an
/// input field no declaration types names it in a `hint` (INPUT-5,
/// FORMULA-5).
fn type_findings(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    collected_sources: bool,
    origins: Option<FieldOrigins<'_>>,
) -> Result<Vec<Diagnostic>, BindError> {
    plan.type_findings(kind, collected_sources)
        .iter()
        .map(|finding| project(plan, finding, origins))
        .collect()
}

/// `finding`'s diagnostic, with its hint where `origins` gives one.
fn project(
    plan: &CompiledExpression,
    finding: &TypeFinding,
    origins: Option<FieldOrigins<'_>>,
) -> Result<Diagnostic, BindError> {
    let mut diagnostic = finding
        .diagnostic(plan.spec_path(), plan.expression())
        .map_err(|_| BindError::InvalidDiagnostic)?;
    if let Some(hint) = origins.and_then(|origins| untyped_field_hint(origins, finding)) {
        diagnostic
            .context
            .insert("hint".into(), crate::json::Value::String(hint));
    }
    Ok(diagnostic)
}

/// The hint of a comparison fault whose `str` operand is an input field no
/// declaration types ([`FieldOrigins::untyped`]).
fn untyped_field_hint(origins: FieldOrigins<'_>, finding: &TypeFinding) -> Option<String> {
    let TypeFinding::IncompatiblePair {
        left, right, names, ..
    } = finding
    else {
        return None;
    };
    [(*left, &names[0]), (*right, &names[1])]
        .into_iter()
        .filter(|&(kind, _)| kind == ValueType::Str)
        .find_map(|(_, name)| origins.untyped(name.as_deref()?))
}

pub(super) fn numeric_kinds(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    origins: FieldOrigins<'_>,
) -> Vec<Diagnostic> {
    type_findings(plan, kind, false, Some(origins)).expect("numeric findings project")
}

pub(super) fn aggregate_kinds(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    source_arguments: bool,
    origins: FieldOrigins<'_>,
) -> Result<Vec<Diagnostic>, BindError> {
    type_findings(plan, kind, source_arguments, Some(origins))
}

/// Every static type fault of a predicate (FORMULA-5, FORMULA-12), with
/// `kind` giving each name's type as its site's catalogue reads it: the one
/// judgement of every predicate field, so `check()` reports before any
/// record what a run reports (P9). `origins` is None where the predicate
/// reads no input field, as a verification reads the output's columns.
pub(super) fn predicate_kinds(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    origins: Option<FieldOrigins<'_>>,
) -> Vec<Diagnostic> {
    type_findings(plan, kind, false, origins).expect("predicate findings project")
}

/// [`predicate_kinds`] as binding findings, each withheld from `check()`
/// when an operand of its comparison is a field its input may or may not store:
/// only the records decide between the fault and an unknown field (D195,
/// D199).
/// A run's schemas decide every field, so it withholds none.
pub(super) fn predicate_findings(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    origins: FieldOrigins<'_>,
) -> Vec<BindFinding> {
    plan.type_findings(kind, false)
        .iter()
        .map(|fault| {
            let finding = BindFinding::Predicate(
                project(plan, fault, Some(origins)).expect("predicate findings project"),
            );
            if operands(fault).any(|name| origins.undecided(name)) {
                BindFinding::Withheld(Box::new(finding))
            } else {
                finding
            }
        })
        .collect()
}

/// The names of a comparison fault's operands. A numeric operand's fault
/// is reported by `check()` over the type its declarations give, as the
/// `check_unknown_reads` family pins (FORMULA-12).
fn operands(fault: &TypeFinding) -> impl Iterator<Item = &str> {
    let names: &[Option<String>] = match fault {
        TypeFinding::IncompatiblePair { names, .. } => names,
        _ => &[],
    };
    names.iter().filter_map(Option::as_deref)
}

/// [`predicate_findings`] appended to `findings`; true when the predicate
/// has a fault, so its site binds no predicate.
pub(super) fn predicate_faults(
    plan: &CompiledExpression,
    kind: impl Fn(&str) -> Option<ValueType>,
    origins: FieldOrigins<'_>,
    findings: &mut Vec<BindFinding>,
) -> bool {
    let start = findings.len();
    findings.extend(predicate_findings(plan, kind, origins));
    findings.len() != start
}

impl PreparedSpecification {
    /// VALUE-24 for an output column, named as a conversion failure names the
    /// column whose lifecycle cannot complete.
    pub(super) fn conversion_finding(
        &self,
        column: usize,
        result: ResultType,
    ) -> Option<Diagnostic> {
        let column = &self.output.columns()[column];
        conversion_finding(&format!("columns.{}", column.name), result, column.kind)
    }
}

/// VALUE-24: the derivation at `path` declares `target`, which no
/// non-missing result of type `result` can convert to. The caller exempts a
/// derivation that declares `unconvertible`, whose literal answers each one.
pub(super) fn conversion_finding(
    path: &str,
    result: ResultType,
    target: ColumnType,
) -> Option<Diagnostic> {
    (!result.converts_to(target)).then(|| {
        Diagnostic::new(
            ConditionCode::ConversionResultType,
            vec![path.into()],
            [
                ("from".into(), crate::json::Value::String(result.name())),
                (
                    "to".into(),
                    crate::json::Value::String(target.name().into()),
                ),
            ]
            .into(),
        )
    })
}

#[cfg(test)]
mod tests;
