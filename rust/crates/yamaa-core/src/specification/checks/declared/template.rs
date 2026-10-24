//! The rows: each column of the implicit template and its root filter, or
//! each row template's derivations and filter and then the column phase;
//! the column order; and the lookup ranges and scopes the rows read.
use super::*;

/// One thing a row evaluates, as declared: a derivation, with the dataset
/// whose qualifier reads the row's own record there (`None`: every input
/// joins), or a name read outside a derivation.
enum Evaluated<'a> {
    Expression(ExprId, Option<&'a str>),
    Name(&'a str),
}

impl Checks<'_> {
    /// The output columns a derivation reads, as binding records them for
    /// column order: each output column it reads, directly or among the
    /// output rows an unqualified aggregate reduces, and the columns each
    /// named intermediate it reads matches on, unless the intermediate drives
    /// the rows. A window reads output columns alone. A field another input
    /// supplies through an implicit join also depends on the columns that
    /// join matches on: the input's declared key (LOOKUP-1). `record` names the dataset whose record the rows read
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
                    let joined = read.name.owner().filter(|&owner| Some(owner) != record);
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
    /// sorted: those its key, nearest reference and range value read, then
    /// those of each intermediate its range value or derivations read. An
    /// inferred key's columns depend on the fields its input stores, which
    /// only capture decides.
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
                _ => Vec::new(),
            };
            // The key pairs' match values and a nearest selection's
            // reference (LOOKUP-26).
            reads.extend(
                item.match_values()
                    .flat_map(|value| spec.reads(value, ast::Scope::Row))
                    .filter_map(|read| super::name(spec, read.at)),
            );
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

    /// Without row templates, each column in declaration order: a key that
    /// reads other records, the derivation's defects and selections with its
    /// reads of other inputs' joins and its static types, its formula's
    /// qualified names (FORMULA-9), reads of a lookup binding does not bind
    /// (NAME-3) and operand types (FORMULA-12), and its conversion
    /// (VALUE-24); then column order over what every derivation reads
    /// (COLUMN-5, COLUMN-6, COLUMN-7). Returns whether each column's
    /// derivation binds: a conversion finding keeps it bound.
    pub(super) fn base(&self, findings: &mut Vec<BindFinding>) -> Result<Vec<bool>, BindError> {
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
                context: ExpressionContext::Derivation,
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
    pub(super) fn root_filter(
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
    pub(super) fn rows(
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
    pub(super) fn scopes(&self, bound: &Bound, findings: &mut Vec<BindFinding>) {
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
    /// reported once, and a refusal already reported is not repeated. A named
    /// intermediate's bare value that no column takes, which stays
    /// contextual when another lookup's derivations read it, names no value
    /// of a row either (LOOKUP-9); a value qualified by a named
    /// intermediate that builds one of the rows is unsupported; and one
    /// qualified by a row driver that builds none of them is a keyed lookup
    /// of a lookup that declares no key (LOOKUP-3, ROW-2).
    pub(super) fn ranges(
        &self,
        bound: &Bound,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        let spec = self.spec;
        let named = &self.prepared.intermediates;
        let inputs = &self.prepared.sources;
        let refused = |value: &str, drivers: &[(&str, Option<&str>)]| {
            value.split_once('.').is_some_and(|(qualifier, _)| {
                inputs.iter().any(|input| input.name == qualifier)
                    && drivers.iter().any(|&(driver, _)| driver != qualifier)
            })
        };
        // A value qualified by a row driver that builds none of the rows
        // reads it as a keyed lookup: its qualifier and the `key` it lacks.
        fn keyless<'v, 'n>(
            named: &'n intermediates::Declarations,
            value: &'v str,
        ) -> Option<(&'v str, &'n str)> {
            let (qualifier, _) = value.split_once('.')?;
            let (index, _) = named.reference(value)?;
            Some((qualifier, named.key_required(index)?))
        }
        let keyless = |value| keyless(named, value);
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
                    let contextual = !value.contains('.')
                        && !self.column(value)
                        && spec
                            .intermediates
                            .iter()
                            .position(|item| item.id.name == identifier)
                            .is_some_and(|position| donor_read(spec, position));
                    let driving = value.split_once('.').is_some_and(|(qualifier, _)| {
                        named.reference(value).is_some()
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
