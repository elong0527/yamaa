//! One binding pass (#2844 rewrite 2, #3139). Every derivation of every spec
//! binds through [`BindContext::bind`], with one arm per expression kind:
//! a row template's derivations, the column phase, a lookup's own records
//! (an intermediate's derivations, an aggregate's bindings, a match value)
//! and the one implicit template of a spec without `rows`. The context names
//! the rows a binding is for ([`Scope`]: template, phase, driver, available
//! columns), the lookups it may read ([`Donors`]), its catalog, and where the
//! binding writes its column edges and findings.
//!
//! The implicit template's rows are its keys' distinct combinations of the base
//! driver's records, or the records themselves when a window derives a key
//! ([`Implicit`]). Its reads keep the rules a spec without `rows` has always
//! bound by: a driver field read by a non-key column collects the distinct
//! readings of the row's records; another input's field joins on its implicit
//! join's key, the key that input declares; a lookup read takes the
//! selector's record without a row template's checks of its keys; a formula
//! reports its qualified names first; a `case` predicate is typed and may
//! suggest the input that stores an unknown name; and a formula or
//! `first_available` in a `case` result binds as those results did before.
use super::*;
use crate::{
    dataset::{Intermediate, SecondarySource},
    project_call_binding,
    reference_scope::{self, Phase, Reach},
};

/// The rows a binding is for, as the declarations alone decide them
/// ([`rows::TemplatePhase::scope`], [`rows::Templates::column_bindings`],
/// [`rows::Implicit::scope`]), so `check()` reads the scopes binding binds
/// in without a source schema.
#[derive(Clone, Copy, Debug)]
pub(super) struct Scope<'a> {
    pub template: Template<'a>,
    /// Row construction, with a template's `group_by`; or the column phase,
    /// with every grouped template's driver and `group_by` (NAME-4).
    pub phase: Phase<'a>,
    /// The dataset whose records build the rows.
    pub driver: &'a str,
    /// The output columns a row-phase derivation may read: those its
    /// template derives during row construction. `None` reads any column.
    pub available: Option<&'a [usize]>,
}

/// Whose rows a binding is for.
#[derive(Clone, Copy, Debug)]
pub(super) enum Template<'a> {
    /// The implicit template of a spec without `rows`.
    Implicit(Implicit),
    /// A row template's row phase, named by its id.
    Row(&'a str),
    /// No one template: the column phase completes the rows of every
    /// template that leaves the column to it, and a lookup binds over its
    /// own records.
    Any,
}

/// The implicit template of a spec without `rows`: one template over the
/// base driver, whose keys are computed from one base record each.
#[derive(Clone, Copy, Debug)]
pub(super) struct Implicit {
    /// Whether the bound column is a key.
    pub key: bool,
    /// Whether every row is one base record: a window derives a key.
    pub records: bool,
}

impl Implicit {
    /// Whether the bound column reads one base record per row: a key does,
    /// and every column does with record rows.
    pub(super) fn record(&self) -> bool {
        self.key || self.records
    }
}

/// The named intermediates a binding may read, with their bound relations,
/// and the inputs whose schemas type the fields those relations store.
#[derive(Clone, Copy)]
pub(super) struct Donors<'a> {
    pub named: &'a intermediates::Declarations,
    pub secondary: &'a [SecondarySource],
    pub items: &'a [Option<Intermediate>],
    pub inputs: Inputs<'a>,
}

impl Donors<'_> {
    /// The kind of the field `column` of the intermediate `item`'s records:
    /// a derived field's, or a stored field's as its input's schema decides
    /// it ([`Inputs::kind_of`]).
    pub(super) fn kind(&self, item: &Intermediate, column: usize) -> Option<ColumnType> {
        self.inputs.source_kind(item, self.secondary, column)
    }
    /// Whether the schemas decide if the relation `relation` stores the
    /// field `field` ([`InputSchema::stores`]): an input's schema says, a
    /// named intermediate leaves a field it does not derive to the input it
    /// reads, and any other relation's fields are what its binding laid out.
    pub(super) fn decides(&self, relation: &str, field: &str) -> bool {
        self.inputs.decides_through(self.named, relation, field)
    }
    /// `finding`, withheld from `check()` unless the schemas decide whether
    /// `relation` stores `field`, on which its wording depends.
    pub(super) fn withhold_unless(
        &self,
        relation: &str,
        field: &str,
        finding: BindFinding,
    ) -> BindFinding {
        if self.decides(relation, field) {
            finding
        } else {
            BindFinding::Withheld(std::boxed::Box::new(finding))
        }
    }
}

/// The inputs binding reads: each declared input in authored order, with
/// its schema ([`InputSchema`]), and the input whose records drive the
/// rows. Built once where binding starts and carried as this one value to
/// every layer that reads an input's schema, and to the catalogs and
/// secondary ports built from it: `run()` binds the captured schemas, and
/// `check()` the declared ones (#3805).
#[derive(Clone, Copy)]
pub(super) struct Inputs<'a> {
    pub sources: &'a [SourceDeclaration],
    pub schemas: &'a [InputSchema],
    pub driver: usize,
}

impl<'a> Inputs<'a> {
    /// The driver's schema, the primary port of the rows' plan.
    pub(super) fn driver_schema(&self) -> &'a InputSchema {
        &self.schemas[self.driver]
    }
    /// The schema of the input `name`.
    pub(super) fn schema(&self, name: &str) -> Option<&'a InputSchema> {
        self.sources
            .iter()
            .position(|source| source.name == name)
            .map(|index| &self.schemas[index])
    }
    /// Every input but the driver, in authored order: the secondary ports
    /// of the rows' plan, laid out as their schemas are.
    pub(super) fn secondary(&self) -> Vec<SecondarySource> {
        self.sources
            .iter()
            .zip(self.schemas)
            .enumerate()
            .filter(|(index, _)| *index != self.driver)
            .map(|(_, (declaration, schema))| SecondarySource {
                name: declaration.name.clone(),
                schema: schema.layout().clone(),
            })
            .collect()
    }
    /// Whether the schemas decide if the relation `relation` stores the
    /// field `field`: an input's schema says ([`InputSchema::stores`]), and
    /// any other relation's fields are what its binding laid out.
    pub(super) fn decides(&self, relation: &str, field: &str) -> bool {
        self.schema(relation)
            .is_none_or(|schema| schema.stores(field).is_some())
    }
    /// [`Self::decides`], where `relation` may be a named intermediate,
    /// which leaves a field it does not derive to the input it reads.
    pub(super) fn decides_through(
        &self,
        named: &intermediates::Declarations,
        relation: &str,
        field: &str,
    ) -> bool {
        if self.schema(relation).is_some() {
            return self.decides(relation, field);
        }
        match named.input(relation) {
            Some(index) if !named.derives(relation, field) => self
                .schemas
                .get(index)
                .is_none_or(|schema| schema.stores(field).is_some()),
            _ => true,
        }
    }
    /// The kind of the field `field` of the relation `relation`, laid out as
    /// `layout`: an input's stored field as its schema decides it, and any
    /// other field, of a relation no input is or one an intermediate
    /// derives, as its layout gives it, which its binding decided.
    pub(super) fn kind_of(
        &self,
        relation: &str,
        field: &str,
        layout: ColumnType,
    ) -> Option<ColumnType> {
        match self.schema(relation) {
            Some(schema) => match schema.position(field) {
                Some(column) => schema.kind(column),
                None => Some(layout),
            },
            None => Some(layout),
        }
    }
    /// The kinds of the fields of `relation`, laid out as `layout`.
    pub(super) fn kinds_of(&self, relation: &str, layout: &TableSchema) -> Vec<Option<ColumnType>> {
        layout
            .columns()
            .iter()
            .map(|column| self.kind_of(relation, &column.name, column.kind))
            .collect()
    }
    /// The catalog fields of `relation`, laid out as `layout`, each with its
    /// kind ([`Self::kind_of`]).
    pub(super) fn fields_of<'s>(
        &self,
        relation: &str,
        layout: &'s TableSchema,
    ) -> Vec<reference_binding::Field<'s>> {
        layout
            .columns()
            .iter()
            .map(|column| reference_binding::Field {
                name: &column.name,
                column_type: self.kind_of(relation, &column.name, column.kind),
            })
            .collect()
    }
    /// The kind of the field `column` of the intermediate `item`'s records,
    /// whose secondary ports are `secondary`: a derived field's, as its
    /// donor plan types it, or a stored field's ([`Self::kind_of`]).
    pub(super) fn source_kind(
        &self,
        item: &Intermediate,
        secondary: &[SecondarySource],
        column: usize,
    ) -> Option<ColumnType> {
        use crate::dataset::IntermediateSource;
        if item.derivations.is_some() {
            return item.source_kind(secondary, column);
        }
        let layout = item.source_kind(secondary, column)?;
        let relation = match &item.source {
            IntermediateSource::Input(index) => {
                secondary.get(*index).map(|source| source.name.as_str())
            }
            IntermediateSource::Primary { name, .. } => Some(name.as_str()),
            IntermediateSource::SelfRows { .. } | IntermediateSource::Pairs(_) => None,
        };
        let field = item.source_schema(secondary)?.columns().get(column)?;
        match relation {
            Some(relation) => self.kind_of(relation, &field.name, layout),
            None => Some(layout),
        }
    }
}

/// One derivation's binding: its scope, the lookups and fields it may read,
/// and the column edges and findings it records.
pub(super) struct BindContext<'a, 'f> {
    pub scope: Scope<'a>,
    pub donors: Donors<'a>,
    pub catalog: &'a Catalog,
    pub output: &'a TableSchema,
    pub project: Option<&'a crate::project_calls::ProjectCalls>,
    pub tree: &'a types::Tree,
    pub edges: &'f mut Vec<usize>,
    pub findings: &'f mut Vec<BindFinding>,
    /// Whether the derivation's declaration defects were already reported
    /// before any reference bound ([`cases::padding_findings`]).
    pub reported: bool,
}

impl<'a> BindContext<'a, '_> {
    /// The row template whose row phase reads, which row-phase findings name.
    pub(super) fn row(&self) -> Option<&'a str> {
        match self.scope.template {
            Template::Row(id) => Some(id),
            _ => None,
        }
    }
    pub(super) fn implicit(&self) -> Option<Implicit> {
        match self.scope.template {
            Template::Implicit(implicit) => Some(implicit),
            _ => None,
        }
    }
    /// The names this binding reads, typed from the captured schemas.
    pub(super) fn captured(&self) -> types::CapturedTypes<'a> {
        types::CapturedTypes::new(self.donors, self.catalog)
    }
    /// The tree binding reads.
    pub(super) fn spec(&self) -> &'a ast::Spec {
        self.tree.spec()
    }
    /// `finding`, a bare name's finding whose cause the catalog's own
    /// dataset decides (`unknown_field` against `unresolvable_name`,
    /// NAME-3, FORMULA-9): withheld unless the schemas decide whether that
    /// dataset stores `name` ([`Donors::withhold_unless`]).
    pub(super) fn bare_name(&self, name: &str, finding: BindFinding) -> BindFinding {
        match self.catalog.dataset_name(0) {
            Some(relation) => self.donors.withhold_unless(relation, name, finding),
            None => finding,
        }
    }
    /// Each bare-name finding from `start` on that the catalog's own dataset
    /// decides ([`Self::bare_name`]).
    pub(super) fn bare_names_from(&mut self, start: usize) {
        for index in start..self.findings.len() {
            if let BindFinding::OutputReference {
                name,
                finding:
                    reference_binding::Diagnostic::UnknownField
                    | reference_binding::Diagnostic::UnresolvableName { .. },
                ..
            } = &self.findings[index]
            {
                let name = name.clone();
                let finding = self.findings[index].take();
                self.findings[index] = self.bare_name(&name, finding);
            }
        }
    }

    /// Bind one derivation: the expression `e`, which derives `column`.
    /// Preparation admitted every expression it binds, so a value the tree
    /// cannot give binding is an internal failure.
    pub(super) fn bind(
        &mut self,
        e: ExprId,
        column: usize,
    ) -> Result<Option<Expression>, BindError> {
        let spec = self.spec();
        let path = checks::path(spec, e);
        let internal = |_| BindError::Internal;
        match &spec.expr(e).kind {
            ExprKind::Literal(value) => Ok(Some(Expression::Literal(bound_constant(value)?))),
            ExprKind::Ref(_) | ExprKind::Source(_) => {
                let source =
                    source_expressions::Declaration::from_tree(spec, e).map_err(internal)?;
                source_expressions::bind(self, &source, column, &path)
            }
            ExprKind::Formula(formula) => self.compute(formula),
            ExprKind::Case(arms) => cases::bind(self, arms, column),
            ExprKind::Flag { .. } => flags::bind(
                &flags::Declaration::from_tree(spec, e).map_err(internal)?,
                self,
            ),
            ExprKind::FirstAvailable { sources, missing } => {
                let selection =
                    source_expressions::FirstAvailable::from_tree(spec, sources, missing.as_ref())
                        .map_err(internal)?;
                match self.implicit() {
                    Some(implicit) => {
                        self.implicit_first_available(implicit, &selection, &path, column)
                    }
                    None => self.first_available(&selection, &path, column),
                }
            }
            ExprKind::Scalar { .. } => self.scalar(e, column),
            ExprKind::Window(_) => {
                self.window(&windows::Declaration::from_tree(spec, e).map_err(internal)?)
            }
            ExprKind::Aggregate(_) => {
                let aggregate = aggregates::Declaration::from_tree(
                    spec,
                    e,
                    self.donors.named,
                    self.project.is_some(),
                )
                .map_err(internal)?;
                aggregates::bind(&aggregate, self)
            }
            ExprKind::Odm(_) => {
                // With record rows a key reads its row's one record, as an
                // ungrouped row template's derivation does.
                let (phase, records) = match self.implicit() {
                    Some(implicit) if implicit.records && implicit.key => {
                        (Phase::Row { group_by: None }, true)
                    }
                    Some(implicit) => (self.scope.phase, implicit.records),
                    None => (self.scope.phase, false),
                };
                odm::bind(
                    &odm::Declaration::from_tree(spec, e).map_err(internal)?,
                    self.catalog,
                    self.scope.driver,
                    self.row(),
                    phase,
                    records,
                    self.findings,
                )
            }
            ExprKind::Call { .. } => self.call(self.tree.call(e)?),
            ExprKind::Unsupported(_) => Err(BindError::Internal),
        }
    }

    /// A formula in a derivation: bound, or its grammar finding reported
    /// unless the derivation's declaration findings came first.
    fn compute(&mut self, formula: &ast::Formula) -> Result<Option<Expression>, BindError> {
        let text_path = checks::render(self.spec(), formula.text.meta);
        match &formula.compiled {
            Ok(compiled) => match self.implicit() {
                Some(_) => self.implicit_formula(compiled, &text_path),
                None => self.formula(compiled, &text_path),
            },
            Err(error @ CompileError::Invalid { .. }) => {
                if !self.reported {
                    self.findings.push(BindFinding::Numeric {
                        path: text_path,
                        expression: formula.text.text.clone(),
                        error: error.clone(),
                    });
                }
                Ok(None)
            }
            Err(_) => Err(BindError::Internal),
        }
    }

    /// Every scalar verb: each operand binds as the scope decides
    /// ([`Self::operand`], or as a source binding binds), binding types it as
    /// the operation asks and refuses a type it does not accept, and the
    /// operation gives the plan expression.
    fn scalar(&mut self, e: ExprId, column: usize) -> Result<Option<Expression>, BindError> {
        use crate::scalar_verb::{Completion, Read, Typing};
        let spec = self.spec();
        let operation =
            crate::scalar_verb::Operation::from_tree(spec, e).map_err(|_| BindError::Internal)?;
        let start = self.findings.len();
        if let Some(refusal) = operation.refusal() {
            self.findings.push(BindFinding::Scalar(refusal.clone()));
            return Ok(None);
        }
        let typing = operation.typing();
        let mut operands = Vec::new();
        let mut complete = true;
        for (index, operand) in operation.operands().iter().enumerate() {
            let mark = self.findings.len();
            let (name, bound) = match &operand.read {
                Read::Literal(value) => {
                    operands.push(Assignment {
                        column,
                        expression: Expression::Literal(value.clone()),
                        path: operand.path.clone(),
                    });
                    continue;
                }
                Read::Name(name) => (name.as_str(), self.operand(name, &operand.path)?),
                Read::Source(node) => {
                    let source = source_expressions::Declaration::from_tree(spec, *node)
                        .map_err(|_| BindError::Internal)?;
                    let bound = source_expressions::bind(self, &source, column, &operand.path)?;
                    (checks::variable(spec, *node).unwrap_or_default(), bound)
                }
            };
            let Some(expression) = bound else {
                complete = false;
                if let Some(unknown) = operation.unknown(index) {
                    self.unknown(mark, name, unknown);
                }
                continue;
            };
            let kind = match typing {
                Typing::None | Typing::Together => None,
                Typing::Catalog => self.catalog.type_of(name),
                Typing::RowTemplates => self
                    .implicit()
                    .is_none()
                    .then(|| self.catalog.type_of(name))
                    .flatten(),
                Typing::Bound => match &expression {
                    Expression::Intermediate { index, column } => self.donors.items[*index]
                        .as_ref()
                        .and_then(|item| self.donors.kind(item, *column)),
                    _ => self.catalog.type_of(name),
                },
                Typing::Captured => self.captured_type(name),
            };
            if let Some(refusal) = kind.and_then(|kind| operation.check(index, kind)) {
                self.findings.push(BindFinding::Scalar(refusal));
                complete = false;
                continue;
            }
            operands.push(Assignment {
                column,
                expression,
                path: operand.path.clone(),
            });
        }
        if complete {
            if let Some(refusal) = operation.late_refusal() {
                self.findings.push(BindFinding::Scalar(refusal.clone()));
                complete = false;
            }
        }
        if typing == Typing::Together {
            let types = operation
                .operands()
                .iter()
                .map(|operand| match &operand.read {
                    Read::Name(name) => self.captured_type(name),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if let Some(refusal) = operation.incomparable(&types) {
                self.findings.push(BindFinding::Scalar(refusal));
            }
        }
        let done = match operation.completion() {
            Completion::Operands => complete,
            Completion::Findings => self.findings.len() == start,
        };
        Ok(if done { operation.plan(operands) } else { None })
    }

    /// A read of `name`, a field a named intermediate selects by its donor's
    /// captured type and anything else by the catalog's.
    fn captured_type(&self, name: &str) -> Option<ColumnType> {
        match self.donors.named.reference(name) {
            Some((index, field)) => self
                .donors
                .items
                .get(index)
                .and_then(Option::as_ref)
                .and_then(|item| {
                    let column = item.source_column(self.donors.secondary, field)?;
                    self.donors.kind(item, column)
                }),
            None => self.catalog.type_of(name),
        }
    }

    /// A verb's own refusal of an operand that did not bind because `name`
    /// is unknown, in place of each reference refusal since `mark`.
    fn unknown(&mut self, mark: usize, name: &str, unknown: crate::scalar_verb::Unknown) {
        if !unknown.qualified && name.contains('.') {
            return;
        }
        for finding in &mut self.findings[mark..] {
            let replaced = match finding {
                BindFinding::UnknownReference { .. }
                | BindFinding::OutputReference {
                    finding: reference_binding::Diagnostic::UnknownField,
                    ..
                } => true,
                BindFinding::QualifiedReference {
                    finding: crate::reference_scope::Finding::UnknownField,
                    ..
                } => unknown.qualified,
                _ => false,
            };
            if replaced {
                *finding = BindFinding::Scalar(unknown.diagnostic.clone());
            }
        }
    }

    /// The binder still runs after a refused group read, so it reports the
    /// unknown and foreign fields that `group_findings` leaves to it.
    fn window(&mut self, window: &windows::Declaration) -> Result<Option<Expression>, BindError> {
        let mark = self.findings.len();
        let refused = window.group_findings(
            self.catalog,
            self.scope.driver,
            self.row(),
            self.scope.phase,
            self.findings,
        )?;
        let bound = window.bind_available(
            self.catalog,
            self.row(),
            self.scope.available,
            self.edges,
            self.findings,
        )?;
        self.bare_names_from(mark);
        Ok(if refused { None } else { bound })
    }

    /// A project function call. A row template's call reads its row's
    /// record; the implicit template's reads one base record for a key or
    /// with record rows, and otherwise collects the distinct readings of the
    /// records that feed the key combination.
    pub(super) fn call(&mut self, call: usize) -> Result<Option<Expression>, BindError> {
        let calls = self.project.ok_or(BindError::Internal)?;
        let (source_mode, reach) = match self.implicit() {
            Some(implicit) if !implicit.record() => {
                (project_call_binding::SourceMode::Collect, Reach::Relation)
            }
            Some(_) => (project_call_binding::SourceMode::Record, Reach::Record),
            None => (project_call_binding::SourceMode::Record, Reach::Scalar),
        };
        let context = project_call_binding::Context {
            source_dataset: 0,
            source_mode,
            available_outputs: self.scope.available,
            scope: reference_scope::Scope {
                drivers: &[self.scope.driver],
                current_driver: true,
                reach,
                joined: false,
                phase: self.scope.phase,
            },
        };
        match project_call_binding::bind(calls, call, self.catalog, context) {
            Ok(bound) => {
                self.edges.extend(bound.dependencies);
                Ok(Some(Expression::ProjectFunction(bound.function)))
            }
            Err(project_call_binding::Error::Findings(errors)) => {
                let mark = self.findings.len();
                self.findings
                    .extend(functions::bind_findings(calls, call, errors, self.row())?);
                self.bare_names_from(mark);
                Ok(None)
            }
            Err(project_call_binding::Error::Reference(error)) => Err(BindError::Catalog(error)),
            Err(_) => Err(BindError::Internal),
        }
    }

    /// One named read: a column, a field of an input's record, or a field a
    /// named intermediate selects. Every operand, predicate name and formula
    /// name binds here, as the scope decides.
    pub(super) fn operand(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        if let Some(implicit) = self.implicit() {
            return self.implicit_operand(implicit, name, path);
        }
        let driver = self.scope.driver;
        if self.donors.named.reference(name).is_some()
            && name
                .split_once('.')
                .is_none_or(|(owner, _)| owner != driver)
        {
            return row_intermediates::bind(self, name, path);
        }
        self.direct_read(name, path)
    }

    /// A read the catalog answers: another input's field through its
    /// implicit join, a driver field, or a column, each in this scope.
    pub(super) fn direct_read(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let catalog = self.catalog;
        if name
            .split_once('.')
            .is_some_and(|(dataset, _)| catalog.joined(dataset).is_some())
        {
            return source_expressions::bind_catalog_join(
                name,
                path,
                catalog,
                self.row(),
                self.scope.available,
                self.edges,
                self.findings,
            );
        }
        let path = String::from(path);
        if name.contains('.') {
            let scoped = reference_scope::validate(
                catalog,
                name,
                None,
                reference_scope::Scope {
                    drivers: &[self.scope.driver],
                    current_driver: false,
                    reach: Reach::Scalar,
                    joined: false,
                    phase: self.scope.phase,
                },
            )
            .map_err(BindError::Catalog)?;
            if !scoped.is_empty() {
                let row = self.row();
                self.findings.extend(scoped.into_iter().map(|finding| {
                    BindFinding::QualifiedReference {
                        path: path.clone(),
                        name: name.into(),
                        row: row.map(String::from),
                        finding,
                    }
                }));
                return Ok(None);
            }
        } else if let Some(finding) = catalog
            .validate_output(name, None, self.scope.available, &[0])
            .map_err(BindError::Catalog)?
        {
            self.findings.push(BindFinding::OutputReference {
                path,
                name: name.into(),
                row: self.row().map(String::from),
                finding,
            });
            self.bare_names_from(self.findings.len() - 1);
            return Ok(None);
        }
        Ok(Some(match catalog.bind(name) {
            Some(reference_binding::Binding::Output { column, .. }) => {
                self.edges.push(column);
                Expression::Column(column)
            }
            Some(reference_binding::Binding::Dataset { field, .. }) => Expression::Source(field),
            None => {
                self.findings.push(BindFinding::UnknownReference {
                    path,
                    name: name.into(),
                });
                return Ok(None);
            }
        }))
    }

    /// The implicit template's named read: another input's field joins on
    /// its implicit join's key, the key that input declares; a lookup read
    /// takes the selector's own record; a driver field of a non-key column
    /// collects the distinct readings of the row's records.
    fn implicit_operand(
        &mut self,
        implicit: Implicit,
        name: &str,
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let Donors {
            named,
            secondary,
            items,
            ..
        } = self.donors;
        if let Some((dataset, _)) = name
            .split_once('.')
            .filter(|(dataset, _)| secondary.iter().any(|source| source.name == *dataset))
        {
            return source_expressions::bind_implicit(
                name,
                path,
                &row_drivers::declared_key(self.spec(), dataset),
                self.catalog,
                (self.donors.inputs, secondary),
                self.edges,
                self.findings,
            );
        }
        let path = String::from(path);
        if !name.contains('.') {
            if let Some(finding) = self
                .catalog
                .validate_output(name, None, None, &[0])
                .map_err(BindError::Catalog)?
            {
                self.findings.push(BindFinding::OutputReference {
                    path,
                    name: name.into(),
                    row: None,
                    finding,
                });
                self.bare_names_from(self.findings.len() - 1);
                return Ok(None);
            }
        }
        let source = if let Some((index, field)) = named.reference(name) {
            let Some(item) = &items[index] else {
                self.findings.push(BindFinding::UnknownReference {
                    path,
                    name: name.into(),
                });
                return Ok(None);
            };
            self.edges
                .extend(row_intermediates::dependencies(index, items)?);
            let Some(column) = item.source_column(secondary, field) else {
                self.findings
                    .push(BindFinding::Lookup(LookupFinding::reference(
                        &path,
                        name,
                        None,
                        ReferenceCause::Value,
                        None,
                    )));
                return Ok(None);
            };
            Expression::Intermediate { index, column }
        } else {
            match self.catalog.bind(name) {
                Some(reference_binding::Binding::Output { column, .. }) => {
                    self.edges.push(column);
                    Expression::Column(column)
                }
                Some(reference_binding::Binding::Dataset { field, .. }) if implicit.key => {
                    Expression::Source(field)
                }
                Some(reference_binding::Binding::Dataset { field, .. }) => Expression::Collect {
                    column: field,
                    identifier: name.into(),
                    filter: None,
                    selection: None,
                },
                None => {
                    self.findings.push(BindFinding::UnknownReference {
                        path,
                        name: name.into(),
                    });
                    return Ok(None);
                }
            }
        };
        Ok(Some(source))
    }

    /// A formula: each name it reads binds in written order; a qualified
    /// name must name a lookup, except over a lookup's own records.
    fn formula(
        &mut self,
        compiled: &CompiledExpression,
        text_path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let text_path = String::from(text_path);
        let before = self.findings.len();
        let mut bindings = Vec::new();
        for identifier in compiled.identifiers() {
            if !self.catalog.record_derivation()
                && identifier.split_once('.').is_some_and(|(dataset, _)| {
                    self.donors.named.reference(identifier).is_none()
                        && (matches!(self.scope.phase, Phase::Column { .. })
                            || self.catalog.joined(dataset).is_some())
                })
            {
                match self.scope.phase {
                    Phase::Column { .. } => {
                        self.findings.push(BindFinding::QualifiedNumericReference {
                            path: text_path.clone(),
                            expression: compiled.expression().into(),
                            identifier: identifier.into(),
                        })
                    }
                    Phase::Row { group_by } => {
                        let mut finding = LookupFinding::new(
                            &text_path,
                            if group_by.is_some() {
                                crate::diagnostic::ConditionCode::GroupedRowNumericSourceScope
                            } else {
                                crate::diagnostic::ConditionCode::RowNumericSourceScope
                            },
                        );
                        finding.text("identifier", identifier);
                        finding.texts("drivers", &[self.scope.driver]);
                        self.findings.push(BindFinding::Lookup(finding));
                    }
                }
                continue;
            }
            if let Some(expression) = self.operand(identifier, &text_path)? {
                let read = match expression {
                    Expression::Source(column) => Read::Source(column),
                    Expression::Column(column) => Read::Column(column),
                    Expression::Intermediate { index, column } => {
                        Read::Intermediate { index, column }
                    }
                    _ => return Err(BindError::Internal),
                };
                bindings.push(Binding {
                    name: identifier.into(),
                    read,
                });
            }
        }
        self.numeric(compiled, bindings, before)
    }

    /// The implicit template's formula reports every qualified name that
    /// names no lookup first (FORMULA-9), then binds its lookup reads, then
    /// its columns.
    fn implicit_formula(
        &mut self,
        compiled: &CompiledExpression,
        text_path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let text_path = String::from(text_path);
        let before = self.findings.len();
        let Donors {
            named,
            secondary,
            items,
            ..
        } = self.donors;
        qualified_numeric_findings(compiled, &text_path, named, self.findings);
        let mut bindings = Vec::new();
        for name in compiled
            .identifiers()
            .filter(|name| named.reference(name).is_some())
        {
            let (index, field) = named.reference(name).ok_or(BindError::Internal)?;
            if let Some(item) = &items[index] {
                self.edges
                    .extend(row_intermediates::dependencies(index, items)?);
                if let Some(column) = item.source_column(secondary, field) {
                    bindings.push(Binding {
                        name: name.into(),
                        read: Read::Intermediate { index, column },
                    });
                    continue;
                }
            }
            self.findings.push(BindFinding::UnknownReference {
                path: text_path.clone(),
                name: name.into(),
            });
        }
        for name in compiled.identifiers().filter(|name| !name.contains('.')) {
            if let Some(finding) = self
                .catalog
                .validate_output(name, None, None, &[0])
                .map_err(BindError::Catalog)?
            {
                self.findings.push(BindFinding::OutputReference {
                    path: text_path.clone(),
                    name: name.into(),
                    row: None,
                    finding,
                });
                self.bare_names_from(self.findings.len() - 1);
                continue;
            }
            match self.catalog.bind(name) {
                Some(reference_binding::Binding::Output { column, .. }) => {
                    self.edges.push(column);
                    bindings.push(Binding {
                        name: name.into(),
                        read: Read::Column(column),
                    });
                }
                Some(reference_binding::Binding::Dataset { field, .. }) => bindings.push(Binding {
                    name: name.into(),
                    read: Read::Source(field),
                }),
                None => self.findings.push(BindFinding::UnknownReference {
                    path: text_path.clone(),
                    name: name.into(),
                }),
            }
        }
        self.numeric(compiled, bindings, before)
    }

    /// A formula in a `case` result of the implicit template reads a lookup
    /// or a column; any other name is unknown.
    pub(super) fn branch_formula(
        &mut self,
        compiled: &CompiledExpression,
        text_path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let text_path = String::from(text_path);
        let before = self.findings.len();
        let Donors {
            named,
            secondary,
            items,
            ..
        } = self.donors;
        let mut bindings = Vec::new();
        for name in compiled.identifiers() {
            if let Some((index, field)) = named.reference(name) {
                if let Some(item) = &items[index] {
                    self.edges
                        .extend(row_intermediates::dependencies(index, items)?);
                    if let Some(column) = item.source_column(secondary, field) {
                        bindings.push(Binding {
                            name: name.into(),
                            read: Read::Intermediate { index, column },
                        });
                        continue;
                    }
                }
                self.findings.push(BindFinding::UnknownReference {
                    path: text_path.clone(),
                    name: name.into(),
                });
                continue;
            }
            match self.catalog.bind(name) {
                Some(reference_binding::Binding::Output { column, .. }) => {
                    self.edges.push(column);
                    bindings.push(Binding {
                        name: name.into(),
                        read: Read::Column(column),
                    });
                }
                _ => self.findings.push(BindFinding::UnknownReference {
                    path: text_path.clone(),
                    name: name.into(),
                }),
            }
        }
        self.numeric(compiled, bindings, before)
    }

    /// A formula's static types over the names it bound, then its plan.
    fn numeric(
        &mut self,
        compiled: &CompiledExpression,
        bindings: Vec<Binding>,
        before: usize,
    ) -> Result<Option<Expression>, BindError> {
        let captured = self.captured();
        captured.numeric(compiled, &bindings, self.findings);
        (self.findings.len() == before)
            .then(|| BoundNumeric::new(captured.typed(compiled), bindings).map(Expression::Compute))
            .transpose()
            .map_err(BindError::InvalidNumericBinding)
    }

    /// `first_available`: each source binds as a structured source of the
    /// row's own scope. In a `case` result of the implicit template a source
    /// read of a non-key column collects the distinct readings of the row's
    /// records, unless record rows make the row one record.
    pub(super) fn first_available(
        &mut self,
        selection: &source_expressions::FirstAvailable,
        path: &str,
        column: usize,
    ) -> Result<Option<Expression>, BindError> {
        let before = self.findings.len();
        let collect = self.implicit().is_some_and(|implicit| !implicit.records);
        let mut operands = Vec::new();
        for (index, source) in selection.sources.iter().enumerate() {
            let path = format!("{path}.sources[{index}]");
            let read =
                source_expressions::bind_template(self, source, column, &path)?.map(|expression| {
                    match expression {
                        Expression::Source(column) if collect => {
                            crate::dataset::SelectionRead::Collect {
                                column,
                                identifier: source.variable.clone(),
                                filter: None,
                            }
                        }
                        Expression::Source(column) => crate::dataset::SelectionRead::Source(column),
                        Expression::Column(column) => crate::dataset::SelectionRead::Column(column),
                        expression => crate::dataset::SelectionRead::Expression(
                            std::boxed::Box::new(Assignment {
                                column,
                                expression,
                                path: path.clone(),
                            }),
                        ),
                    }
                });
            if let Some(read) = read {
                operands.push(crate::dataset::SelectionSource { path, read });
            }
        }
        Ok(
            (self.findings.len() == before).then_some(Expression::FirstAvailable(
                std::boxed::Box::new(crate::dataset::FirstAvailable::new(
                    path.into(),
                    operands,
                    selection.missing.clone(),
                )),
            )),
        )
    }

    /// The implicit template's `first_available` binds each source as its
    /// structured sources bind. A collected read of a non-key column stays a
    /// collection; with record rows each source reads the row's record.
    fn implicit_first_available(
        &mut self,
        implicit: Implicit,
        selection: &source_expressions::FirstAvailable,
        path: &str,
        column: usize,
    ) -> Result<Option<Expression>, BindError> {
        let mut operands = Vec::new();
        for (index, operand) in selection.sources.iter().enumerate() {
            let operand_path = format!("{path}.sources[{index}]");
            if let Some(expression) =
                source_expressions::bind(self, operand, column, &operand_path)?
            {
                // Distinct-value collection failures belong to first_available.
                // Recursive operands retain their own expression and handler paths.
                let read = match expression {
                    Expression::Collect {
                        column,
                        identifier,
                        filter,
                        selection: None,
                    } if !implicit.records => crate::dataset::SelectionRead::Collect {
                        column,
                        identifier,
                        filter: filter.map(std::boxed::Box::new),
                    },
                    expression => crate::dataset::SelectionRead::Expression(std::boxed::Box::new(
                        Assignment {
                            column,
                            expression,
                            path: operand_path.clone(),
                        },
                    )),
                };
                operands.push(crate::dataset::SelectionSource {
                    path: operand_path,
                    read,
                });
            }
        }
        Ok(Some(Expression::FirstAvailable(std::boxed::Box::new(
            crate::dataset::FirstAvailable::new(path.into(), operands, selection.missing.clone()),
        ))))
    }
}

/// FORMULA-9: a qualified formula name must name a declared record lookup.
/// The test reads declarations alone.
fn qualified_numeric_findings(
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
