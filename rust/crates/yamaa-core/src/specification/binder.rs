//! One binding pass (#2844 rewrite 2, #3139). Every derivation of every spec
//! binds through [`BindContext::bind`], with one arm per expression kind:
//! a row template's derivations, the column phase, a lookup's own records
//! (an intermediate's derivations, an aggregate's bindings, a match value)
//! and the one implicit template of a spec without `rows`. The context names
//! the rows a binding is for ([`Scope`]: template, phase, driver, available
//! columns), the lookups it may read ([`Donors`]), its catalog, and where the
//! binding writes its column edges and findings.
//!
//! The implicit template's rows are its keys' distinct combinations of the
//! base driver's records, or the records themselves when a window derives a
//! key ([`Implicit`]). Its reads keep the no-rows rules the scope and the
//! template decide: a driver field of a non-key column collects the distinct
//! readings of a row's records; another input's field joins on the output
//! keys its fields name; a lookup read takes the selector's own record; a
//! formula reports its qualified names first; and a `case` result's formula
//! and `first_available` bind as case branches always have there.
use super::*;
use crate::{
    dataset::{Intermediate, SecondarySource},
    project_call_binding,
    reference_scope::{self, Phase, Reach},
};

/// The rows a binding is for, as the declarations alone decide them. Binding
/// and `check()` read the same scopes; no source schema is needed.
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
    Implicit(Implicit<'a>),
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
pub(super) struct Implicit<'a> {
    /// The output keys, in declared order.
    pub keys: &'a [usize],
    /// Whether the bound column is a key.
    pub key: bool,
    /// Whether every row is one base record: a window derives a key.
    pub records: bool,
}

impl Implicit<'_> {
    /// Whether the bound column reads one base record per row: a key does,
    /// and every column does with record rows.
    pub(super) fn record(&self) -> bool {
        self.key || self.records
    }
}

/// The named intermediates a binding may read, with their bound relations.
#[derive(Clone, Copy)]
pub(super) struct Donors<'a> {
    pub named: &'a intermediates::Declarations,
    pub secondary: &'a [SecondarySource],
    pub items: &'a [Option<Intermediate>],
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
    pub(super) fn implicit(&self) -> Option<Implicit<'a>> {
        match self.scope.template {
            Template::Implicit(implicit) => Some(implicit),
            _ => None,
        }
    }
    /// The names this binding reads, typed from the captured schemas.
    pub(super) fn captured(&self) -> types::CapturedTypes<'a> {
        types::CapturedTypes::new(
            self.donors.named,
            self.catalog,
            self.donors.secondary,
            self.donors.items,
        )
    }
    /// Bind one derivation, the operation at `path` that derives `column`.
    pub(super) fn bind(
        &mut self,
        operation: &Operation,
        path: &str,
        column: usize,
    ) -> Result<Option<Expression>, BindError> {
        match operation {
            Operation::Literal(value) => Ok(Some(Expression::Literal(value.clone()))),
            Operation::Source(source) => source_expressions::bind(self, source, column, path),
            Operation::Compute(compiled) => match self.implicit() {
                Some(_) => self.implicit_formula(compiled, path),
                None => self.formula(compiled, path),
            },
            Operation::InvalidNumeric { expression, error } => {
                if !self.reported {
                    self.findings.push(BindFinding::Numeric {
                        path: format!("{path}.expr"),
                        expression: expression.clone(),
                        error: error.clone(),
                    });
                }
                Ok(None)
            }
            Operation::Case(branches) => cases::bind(self, branches, column),
            Operation::Flag(flag) => flags::bind(flag, self),
            Operation::FirstAvailable(selection) => match self.implicit() {
                Some(implicit) => self.implicit_first_available(implicit, selection, path, column),
                None => self.first_available(selection, path, column),
            },
            Operation::Round(_)
            | Operation::Temporal(_)
            | Operation::Imputation(_)
            | Operation::StrCase(_)
            | Operation::StrPad(_)
            | Operation::StrExtract(_)
            | Operation::StrReplace(_)
            | Operation::StrTemplate(_)
            | Operation::Scalar(_)
            | Operation::MappingCut(_) => self.scalar(operation, column),
            Operation::Window(window) => self.window(window),
            Operation::Aggregate(aggregate) => aggregates::bind(
                aggregate,
                self.catalog,
                self.scope.driver,
                self.row(),
                self.scope.phase,
                self.output,
                self.tree,
                self.donors.named,
                self.donors.secondary,
                self.donors.items,
                self.project,
                self.edges,
                self.findings,
            ),
            Operation::Odm(odm) => {
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
                    odm,
                    self.catalog,
                    self.scope.driver,
                    self.row(),
                    phase,
                    records,
                    self.findings,
                )
            }
            Operation::ProjectFunction { call } => self.call(*call),
        }
    }

    /// Every scalar verb: its operands bind through [`Self::operand`], and
    /// the verb's own module checks and wraps them.
    fn scalar(
        &mut self,
        operation: &Operation,
        column: usize,
    ) -> Result<Option<Expression>, BindError> {
        match operation {
            Operation::Round(rounding) => rounding::bind(rounding, column, self),
            Operation::Temporal(temporal) => temporal::bind(temporal, column, self),
            Operation::Imputation(imputation) => imputation::bind(imputation, column, self),
            Operation::StrCase(case) => str_case::bind(case, column, self),
            Operation::StrPad(pad) => str_pad::bind(pad, column, self),
            Operation::StrExtract(extract) => str_extract::bind(extract, column, self),
            Operation::StrReplace(replace) => str_extract::bind_replace(replace, column, self),
            Operation::StrTemplate(template) => str_template::bind(template, column, self),
            Operation::Scalar(extreme) => scalar::bind(extreme, column, self),
            Operation::MappingCut(mapping) => mapping_cut::bind(mapping, column, self),
            _ => Err(BindError::Internal),
        }
    }

    /// The binder still runs after a refused group read, so it reports the
    /// unknown and foreign fields that `group_findings` leaves to it.
    fn window(&mut self, window: &windows::Declaration) -> Result<Option<Expression>, BindError> {
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
                self.findings
                    .extend(functions::bind_findings(calls, call, errors, self.row())?);
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
            return row_intermediates::bind(
                name,
                path,
                self.donors.named,
                self.donors.secondary,
                self.donors.items,
                self.output,
                self.catalog,
                self.row(),
                self.scope.available,
                self.edges,
                self.findings,
            );
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
                self.findings
                    .extend(
                        scoped
                            .into_iter()
                            .map(|finding| BindFinding::QualifiedReference {
                                path: path.clone(),
                                name: name.into(),
                                row: row.map(String::from),
                                finding,
                            }),
                    );
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
    /// the output keys its fields name; a lookup read takes the selector's
    /// own record; a driver field of a non-key column collects the distinct
    /// readings of the row's records.
    fn implicit_operand(
        &mut self,
        implicit: Implicit<'_>,
        name: &str,
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let Donors {
            named,
            secondary,
            items,
        } = self.donors;
        if name
            .split_once('.')
            .is_some_and(|(dataset, _)| secondary.iter().any(|source| source.name == dataset))
        {
            return source_expressions::bind_implicit(
                name,
                path,
                implicit.keys,
                self.catalog,
                secondary,
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
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let text_path = format!("{path}.expr");
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
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let text_path = format!("{path}.expr");
        let before = self.findings.len();
        let Donors {
            named,
            secondary,
            items,
        } = self.donors;
        checks::declared::qualified_numeric_findings(compiled, &text_path, named, self.findings);
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
        path: &str,
    ) -> Result<Option<Expression>, BindError> {
        let text_path = format!("{path}.expr");
        let before = self.findings.len();
        let Donors {
            named,
            secondary,
            items,
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
            let read = source_expressions::bind_row(self, source, column, &path)?.map(
                |expression| match expression {
                    Expression::Source(column) if collect => {
                        crate::dataset::SelectionRead::Collect {
                            column,
                            identifier: source.variable.clone(),
                            filter: None,
                        }
                    }
                    Expression::Source(column) => crate::dataset::SelectionRead::Source(column),
                    Expression::Column(column) => crate::dataset::SelectionRead::Column(column),
                    expression => crate::dataset::SelectionRead::Expression(std::boxed::Box::new(
                        Assignment {
                            column,
                            expression,
                            path: path.clone(),
                        },
                    )),
                },
            );
            if let Some(read) = read {
                operands.push(crate::dataset::SelectionSource { path, read });
            }
        }
        Ok((self.findings.len() == before).then_some(Expression::FirstAvailable(
            std::boxed::Box::new(crate::dataset::FirstAvailable::new(
                path.into(),
                operands,
                selection.missing.clone(),
            )),
        )))
    }

    /// The implicit template's `first_available` binds each source as its
    /// structured sources bind. A collected read of a non-key column stays a
    /// collection; with record rows each source reads the row's record.
    fn implicit_first_available(
        &mut self,
        implicit: Implicit<'_>,
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
