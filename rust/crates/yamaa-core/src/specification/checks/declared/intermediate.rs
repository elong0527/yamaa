//! The named intermediates, in the order they are derived: each one's
//! derivations over its donor's records, its key values, its nearest
//! selection, its filter's qualifiers and types.
use super::*;

impl Checks<'_> {
    /// One derivation of a lookup, or one derive binding of an aggregate,
    /// node by node in binding's order over the donor's records, which
    /// `scope` names ([`super::donors`]): a structured source's declaration
    /// and filter before its read, and its join options once the read binds;
    /// each other read where binding reads it; a mapping's source type and a
    /// placeholder's type once their read binds; a window's defects before
    /// its reads; and an aggregate as in any ungrouped row. The authored
    /// defects precede it.
    pub(super) fn donor_walk(
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

    /// Each reference of a written key match value of the lookup at `index`
    /// that reads no current-row value ([`unknown_match_value`]), at the path
    /// binding cites it
    /// (`intermediates::bind_one`): a name written as the value at `key`,
    /// any other value at its own entry, `key.<column>`. A lookup that
    /// another lookup's derivations read binds its key values at each read
    /// instead. Once a value's names bind, the verb it holds binds as in a
    /// derivation ([`Self::key_verb`]). True when one is refused: binding then
    /// binds nothing of the lookup.
    fn key_values(&self, index: usize, findings: &mut Vec<BindFinding>) -> Result<bool, BindError> {
        let spec = self.spec;
        let item = &spec.intermediates[index];
        if donor_read(spec, index) {
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

    /// What binding refuses of the nearest selection of `donor`'s lookup that
    /// the declarations decide (`intermediates::bind_nearest`), in binding's
    /// order (LOOKUP-26): a `source` the declarations type as neither `date`
    /// nor `datetime`, a `unit` that type does not count in (TIME-4), a `to`
    /// whose type is not the source's, then each reference of `to` that reads
    /// no current-row value and the verb it holds, as for a key's match value
    /// ([`Self::key_values`]). A source the declarations do not type is left
    /// to binding. True when one is refused: binding then binds nothing of
    /// the lookup.
    fn nearest(
        &self,
        donor: &Donor<'_>,
        stored: &Typing<'_>,
        current_names: &dyn Fn(&str) -> ResultType,
        findings: &mut Vec<BindFinding>,
    ) -> Result<bool, BindError> {
        let spec = self.spec;
        let item = donor.item;
        let (Some(nearest), Some(unit)) = (
            &item.nearest,
            item.nearest.as_ref().and_then(|nearest| nearest.unit),
        ) else {
            return Ok(false);
        };
        let identifier = item.id.name.as_str();
        let owner = Owner::Intermediate(identifier);
        let at = render(spec, nearest.meta);
        let refuse = |diagnostic: Diagnostic, findings: &mut Vec<BindFinding>| {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                diagnostic,
            )));
            Ok(true)
        };
        let Some(kind) = donor.field(&nearest.source.name, stored) else {
            return Ok(false);
        };
        let (source, temporal) = match kind.column_type() {
            Some(source @ ColumnType::Date) => (source, ast::TemporalType::Date),
            Some(source @ ColumnType::DateTime) => (source, ast::TemporalType::DateTime),
            other => {
                return refuse(
                    lookup::nearest_source_type(
                        &format!("{at}.source"),
                        owner,
                        &nearest.source.name,
                        ResultType::declared(other),
                    ),
                    findings,
                )
            }
        };
        if let Err(diagnostic) = crate::scalar_verb::difference(temporal, unit, &at) {
            return refuse(diagnostic, findings);
        }
        let Some(to) = self
            .prepared
            .intermediates
            .reference_declaration(identifier)
        else {
            return Ok(false);
        };
        let path = render(spec, nearest.to_field);
        let expected = ResultType::column(source);
        let actual = match to {
            match_values::Declaration::Variable(read) | match_values::Declaration::Source(read) => {
                current_names(read)
            }
            match_values::Declaration::Literal(_, expr)
            | match_values::Declaration::Read(_, expr)
            | match_values::Declaration::Expression { expr, .. } => {
                self.result_type(*expr, ast::Scope::Row, &current_names)
            }
        };
        if !actual.comparable(expected) {
            return refuse(
                lookup::nearest_reference_type(&path, owner, expected, actual),
                findings,
            );
        }
        // A lookup that another lookup's derivations read binds its
        // reference at each read instead.
        if donor_read(spec, donor.index) {
            return Ok(false);
        }
        let suggests = !matches!(to, match_values::Declaration::Expression { .. });
        let start = findings.len();
        for reference in to
            .references()
            .filter(|name| self.unknown_match_value(name))
        {
            let suggestion = reference
                .strip_prefix("SELF.")
                .filter(|bare| suggests && self.column(bare))
                .map(String::from);
            findings.push(BindFinding::Lookup(LookupFinding::reference(
                &path,
                reference,
                Some(identifier),
                ReferenceCause::OutputKey,
                suggestion,
            )));
        }
        if let (match_values::Declaration::Expression { expr, .. }, true) =
            (to, findings.len() == start)
        {
            self.key_verb(*expr, findings)?;
        }
        Ok(findings.len() != start)
    }

    /// Each name an intermediate's filter qualifies by nothing declared
    /// ([`unknown_filter_qualifiers`]) binds nothing whatever any input
    /// stores: binding refuses it as unknown (LOOKUP-7) at the filter, with a
    /// `suggestion` naming the donor's same-named field when the donor record
    /// has one, and binds nothing of the intermediate. Each is reported only
    /// when the declarations decide that suggestion ([`Self::donor_field`]);
    /// otherwise it is left to `run()`, so the issues stay a subsequence of
    /// `run()`'s (D220). Whether the filter reads such a name, and whether
    /// one was reported.
    fn filter_qualifiers(
        &self,
        donor: &Donor<'_>,
        findings: &mut Vec<BindFinding>,
    ) -> (bool, bool) {
        let (names, path) = unknown_filter_qualifiers(self.spec, self.prepared, donor.item);
        let dataset = donor.item.dataset.name.as_str();
        let mut reported = false;
        for &(name, field) in &names {
            let Some(stored) = self.donor_field(donor, field) else {
                continue;
            };
            reported = true;
            findings.push(BindFinding::Lookup(LookupFinding::reference(
                path,
                name,
                Some(&donor.item.id.name),
                ReferenceCause::Selection,
                stored.then(|| format!("{dataset}.{field}")),
            )));
        }
        (!names.is_empty(), reported)
    }

    /// Whether an intermediate's donor record has the field `field`, as far
    /// as the declarations decide: a field the intermediate derives, whether
    /// or not its derivation binds; over SELF, a column every row template
    /// derives in the row phase; over an input, its generated ordinal
    /// (INPUT-3), an output column of the producer it links (INPUT-10), or a
    /// field its `types` names. Only the records decide any other field of
    /// an input.
    fn donor_field(&self, donor: &Donor<'_>, field: &str) -> Option<bool> {
        let item = donor.item;
        if item
            .bindings
            .iter()
            .any(|binding| binding.name.name == field)
        {
            return Some(true);
        }
        let dataset = item.dataset.name.as_str();
        if dataset == "SELF" {
            let rows::Rows::Templates(templates) = &self.prepared.rows else {
                return Some(false);
            };
            let completed = templates.self_columns();
            return Some(
                self.spec
                    .columns
                    .iter()
                    .position(|column| column.name.name == field)
                    .is_some_and(|column| completed.contains(&column)),
            );
        }
        let index = self
            .prepared
            .sources
            .iter()
            .position(|source| source.name == dataset)?;
        if self.prepared.sources[index].ordinal.as_deref() == Some(field) {
            return Some(true);
        }
        if let Some(contract) = self.prepared.source_contract(index) {
            return Some(contract.fields().iter().any(|stored| stored.name == field));
        }
        self.spec
            .inputs
            .iter()
            .find(|input| input.name.name == dataset)
            .is_some_and(|input| {
                input
                    .source
                    .types
                    .iter()
                    .any(|(declared, _)| declared.name == field)
            })
            .then_some(true)
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
    pub(super) fn intermediates(
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
        // Whether a filter has failed so far, which leaves binding binding
        // nothing of its lookup whether or not this reports it (LOOKUP-7).
        let mut unfiltered = false;
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
            if !item.bindings.is_empty()
                && (!refused.is_empty() || unfiltered || findings.len() != derivations)
            {
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
            // The nearest selection binds after the keys (LOOKUP-26).
            if self.nearest(donor, &stored, &current_names, findings)? {
                refused.insert(index);
            }
            // A name qualified by nothing declared fails the filter, and
            // binding then binds nothing of the lookup (LOOKUP-7).
            let mut filtered = true;
            if let Some(filter) = item.filter {
                self.grammar(filter, findings)?;
                let (fails, reported) = self.filter_qualifiers(donor, findings);
                filtered = !fails;
                unfiltered |= fails;
                if reported {
                    refused.insert(index);
                }
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
            // Only a filter binding binds is correlated (`finish_item`).
            let correlated_filter = filtered
                && item.filter.is_some_and(|filter| {
                    matches!(&spec.expr(filter).kind, ExprKind::Formula(ast::Formula {
                        compiled: Ok(plan), ..
                    }) if plan.identifiers().any(foreign))
                });
            // A lookup bound without its derivations reads no current record
            // through them (`finish_item`).
            let correlated_derivation = !underived.contains(&index)
                && item.bindings.iter().any(|binding| {
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
            // column, or a field of an earlier intermediate or of an input.
            // Its bounds are donor fields; each stated one must be typed
            // (LOOKUP-9). A bare value is an output column alone, and
            // binding refuses one that no column takes at `between.value`,
            // whatever its bounds bind, before it types the range.
            if let Some(between) = &item.between {
                // A bare value that no column takes names nothing the row
                // holds, unless another lookup's derivations
                // read this one: binding then keeps it for the donor record,
                // and refuses it where a row reads the lookup. A value read
                // from a lookup binding did not bind is none, and so is a
                // field qualified by nothing declared.
                if let Some(value) =
                    name(spec, between.value).filter(|value| match value.split_once('.') {
                        None => !self.column(value) && !donor_read(spec, index),
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
                let earlier = &donors[..position];
                let value =
                    name(spec, between.value).and_then(|value| match value.split_once('.') {
                        None if self.column(value) => Some(declared.name(value)),
                        // A contextual value has no type until a donor
                        // record gives it one.
                        None => None,
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
    pub(super) fn written_filter_types(&self) -> Result<Vec<Diagnostic>, BindError> {
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
}
