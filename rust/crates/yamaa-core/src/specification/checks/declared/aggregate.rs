//! An aggregate: its expression and header, its filter's reads, the key a
//! column aggregate matches on, and its derive bindings.
use super::*;

impl Checks<'_> {
    /// An aggregate's expression grammar and header, then its filter's
    /// grammar and reads ([`Self::aggregate_filter`]), the key a column
    /// aggregate matches on, and its derive bindings, each followed by its
    /// conversion once it binds (VALUE-24).
    /// Binding binds nothing more of a column or row aggregate whose
    /// expression or header fails, and binds its derive steps only when
    /// nothing else of it failed.
    pub(super) fn reduction(
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
            if scope.is_some() {
                self.aggregate_filter(filter, relation, findings);
            }
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
        if let (true, Some(relation)) = (column, relation) {
            self.aggregate_keys(e, aggregate, relation, findings)?;
        }
        if scope.is_some() && findings.len() != start {
            return Ok(());
        }
        // Once nothing else of it fails, binding binds its range: a bare value
        // is an output column alone, and one that no column takes names
        // nothing the row holds, nor does a field of a lookup binding does not
        // bind or one qualified by nothing declared (LOOKUP-9).
        if let Some((between, value)) = aggregate
            .between
            .as_ref()
            .filter(|between| {
                column && relation.is_some() && (between.lower.is_some() || between.upper.is_some())
            })
            .and_then(|between| Some((between, name(spec, between.value)?)))
            .filter(|&(_, value)| match value.split_once('.') {
                None => !self.column(value),
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

    /// Each name an aggregate's filter reads outside what the aggregate
    /// reduces, which binding refuses as unknown (NAME-3) at the filter, once
    /// in written order (`aggregates::bind_filter`): the filter of a qualified
    /// aggregate reads only fields of its relation, and the filter of one
    /// that reduces the output rows only output columns. Whether the relation
    /// stores a field of it is left to its records.
    fn aggregate_filter(
        &self,
        filter: ExprId,
        relation: Option<&str>,
        findings: &mut Vec<BindFinding>,
    ) {
        let ExprKind::Formula(ast::Formula {
            compiled: Ok(plan), ..
        }) = &self.spec.expr(filter).kind
        else {
            return;
        };
        let mut seen = BTreeSet::new();
        for name in plan.identifiers().filter(|&name| seen.insert(name)) {
            let outside = match (relation, name.split_once('.')) {
                (Some(relation), Some((owner, _))) => owner != relation,
                (Some(_), None) => true,
                (None, _) => !self.column(name),
            };
            if outside {
                findings.push(BindFinding::UnknownReference {
                    path: plan.spec_path().into(),
                    name: name.into(),
                });
            }
        }
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
                // The aggregate's own expression reads it, so no binding does.
                let mut finding = LookupFinding::new(&at, C::AggregateDeriveVariable);
                finding.text("variable", name);
                finding.text("expr", expression);
                finding.null("binding");
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

    /// What binding refuses of a column aggregate's written `key` over
    /// `relation` that the declarations decide, for each pair in
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
}
