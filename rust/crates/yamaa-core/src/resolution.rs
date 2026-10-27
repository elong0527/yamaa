//! Pruning (LAYER-11) and ordering (LAYER-13) of a composed specification,
//! the step of [`crate::inheritance_preparation::prepare_versioned`] between
//! expanding its named windows and normalizing what survives. A spec that
//! declares no `parent` is a composition of one layer (LAYER-1) and keeps
//! its declaration order (COLUMN-5); only a layered spec is ordered.
//!
//! Pruning reads the AST of the expanded document; the live set is a
//! fixpoint over [`Spec::reads`] from the semantic roots; and the order runs
//! the one cycle detector, [`dependency_analysis`], over the columns each
//! derivation reads, so a layered cycle names its closed loop as a
//! single-file one does. No table is read, no function activated and
//! nothing published.

use crate::ast::{
    ColumnCheckKind, DatasetCheckKind, Derivation, ExprId, ExprKind, Intermediate, JoinKey,
    Literal, Named, Role, Scope, Segment, Spec, Step, Template,
};
use crate::dependency_analysis;
use crate::expression::{CompileError, Context};
use crate::schema::{
    Document, DocumentError, DocumentNode as N, NormalizedDocument, SchemaOrigin, SchemaSource,
};
use std::collections::BTreeSet;

/// How the surviving columns are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Declaration order, for a spec without `parent` (COLUMN-5).
    Declared,
    /// Dependency order, the earliest declaration first (LAYER-13).
    Dependencies,
}

/// A dependency finding of the order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    /// A derivation reads a name no column declares (COLUMN-5).
    Unknown { column: String, dependency: String },
    /// The first cycle as a closed loop of column names, its last entry
    /// repeating the first, and the derivation path of each member (COLUMN-6).
    Cycle {
        cycle: Vec<String>,
        paths: Vec<String>,
    },
}

/// Nothing partially pruned is published on failure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A live text exceeded a parser resource. A refusal never makes the
    /// declarations that text reads dead.
    #[error("pruning limit: context={context:?}, error={error:?}")]
    Limit {
        context: Context,
        error: CompileError,
    },
    #[error("pruning document: {0:?}")]
    Document(DocumentError),
    /// Unknown dependencies in column order, or else the first cycle.
    #[error("pruning invalid: {0:?}")]
    Invalid(Vec<Issue>),
}

/// Keep the declarations of `document`, which `spec` lowers
/// ([`crate::front_end::v1::spec`]), reachable from the semantic roots
/// (LAYER-11), with the surviving columns in `order`, and each node's origin
/// in `document`.
pub fn prune_lowered(
    document: &Document,
    spec: &Spec,
    order: Order,
) -> Result<NormalizedDocument, Error> {
    let mut live = Live::new(spec);
    live.roots()?;
    live.reach()?;
    let columns = match order {
        Order::Declared => live.selected(),
        Order::Dependencies => live.order()?,
    };
    Rebuild::new(document, &live, columns).run()
}

/// How a name is written where it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Outside every predicate.
    Variable,
    /// In a predicate, whose own validation reports an unknown name, so the
    /// name keeps declarations live and orders them but is never an unknown
    /// dependency.
    Predicate,
    /// An ODM item, which names its dataset before the first dot (NAME-7).
    Item,
    /// A lookup's match value or range value, which reads
    /// only a column of the row when bare and whose own binding reports a
    /// name it cannot read (LOOKUP-5, LOOKUP-9), so the name orders
    /// declarations but is never an unknown dependency.
    Match,
}

#[derive(Clone, Debug)]
struct Reading {
    name: String,
    kind: Kind,
}

impl Reading {
    fn variable(named: &Named) -> Self {
        Self {
            name: named.name.clone(),
            kind: Kind::Variable,
        }
    }
}

/// The dataset whose records build `template`'s rows: its driver, or `base`
/// when it names none. What the template derives, groups and filters on
/// reads that dataset's record as the row's own, as binding reads it.
fn template_driver<'a>(template: &'a Template, base: Option<&'a str>) -> Option<&'a str> {
    template
        .driver
        .as_ref()
        .map(|driver| driver.name.as_str())
        .or(base)
}

/// The live declarations, by name, as reads find them.
struct Live<'a> {
    spec: &'a Spec,
    /// `base`, or the only input: the records a row template that names no
    /// driver builds its rows from.
    base: Option<&'a str>,
    /// The one dataset whose records build every row, if every row template
    /// shares it: a qualified read of it reads the row's own record, never a
    /// join.
    driver: Option<&'a str>,
    columns: BTreeSet<String>,
    inputs: BTreeSet<String>,
    intermediates: BTreeSet<String>,
    /// The intermediates nothing reads that survive only to reach their
    /// refusal for a missing `key` (LOOKUP-3, D219): they read nothing, so
    /// they keep no input live, and an input only they name is pruned
    /// instead of failing on a field it lacks.
    refused: BTreeSet<String>,
}

impl<'a> Live<'a> {
    fn new(spec: &'a Spec) -> Self {
        let base = spec.rows.base.as_ref().map(|base| base.name.as_str()).or(
            match spec.inputs.as_slice() {
                [only] => Some(only.name.name.as_str()),
                _ => None,
            },
        );
        let mut drivers = spec
            .rows
            .templates
            .iter()
            .map(|template| template_driver(template, base));
        let driver = match drivers.next() {
            None => base,
            Some(first) => first.filter(|&first| drivers.all(|other| other == Some(first))),
        };
        Self {
            spec,
            base,
            driver,
            columns: BTreeSet::new(),
            inputs: BTreeSet::new(),
            intermediates: BTreeSet::new(),
            refused: BTreeSet::new(),
        }
    }

    /// The columns of the input `owner`'s declared `key`, which an implicit
    /// join to it, or a qualified aggregate over it without `key`, matches
    /// the current row on (LOOKUP-1, LOOKUP-5): read, and ordered before the
    /// reader, but never an unknown dependency, since binding reports a
    /// column the specification does not declare at the read.
    fn key_reads(&self, owner: &str, out: &mut Vec<Reading>) {
        out.extend(
            self.spec
                .input_key(owner)
                .unwrap_or_default()
                .iter()
                .map(|column| Reading {
                    name: column.name.clone(),
                    kind: Kind::Predicate,
                }),
        );
    }

    /// The names `root` reads in `scope` ([`Spec::reads`]) on any row, whose
    /// own record is the shared driver's, if every row template shares one
    /// ([`Live::read_driven`]).
    fn read(&self, root: ExprId, scope: Scope, out: &mut Vec<Reading>) -> Result<(), Error> {
        self.read_driven(root, scope, self.driver, out)
    }

    /// The names `root` reads in `scope` ([`Spec::reads`]) on a row whose
    /// own record is `driver`'s, except an aggregate's own bindings, a bare
    /// reducer name of an aggregate that declares bindings, which names one
    /// of them, and a range bound, which names a donor field; the key columns
    /// of an aggregate, each paired with the current row's same-named value;
    /// and the declared key of each input an implicit join or a keyless
    /// qualified aggregate reads ([`Live::key_reads`]).
    fn read_driven(
        &self,
        root: ExprId,
        scope: Scope,
        driver: Option<&str>,
        out: &mut Vec<Reading>,
    ) -> Result<(), Error> {
        let spec = self.spec;
        let mut bounds = BTreeSet::new();
        let mut predicates = BTreeSet::new();
        let mut items = BTreeSet::new();
        let mut keyless = BTreeSet::new();
        let mut matched = BTreeSet::new();
        // The reducer names of each aggregate that declares bindings: a bare
        // one never reads a column, and binding refuses one that no binding
        // names as `unknown_derive_variable` (LOOKUP-22), layered or not.
        let mut reducers = BTreeSet::new();
        let mut failure = None;
        spec.walk(root, scope, |at, _| {
            // A match value or range value, with every node inside it.
            let inside = matched.contains(&at);
            matched.extend(
                spec.children(at)
                    .filter(|&(role, _)| {
                        inside || matches!(role, Role::KeyValue | Role::RangeValue)
                    })
                    .map(|(_, child)| child),
            );
            match &spec.expr(at).kind {
                ExprKind::Formula(formula) => {
                    if let Err(error @ CompileError::Limit { .. }) = &formula.compiled {
                        failure = Some(Error::Limit {
                            context: formula.context,
                            error: error.clone(),
                        });
                        return Step::Stop;
                    }
                    if formula.context == Context::Predicate {
                        predicates.extend(formula.names.iter().map(|&(_, name)| name));
                    }
                }
                ExprKind::Aggregate(aggregate) => {
                    if !aggregate.bindings.is_empty() {
                        reducers.extend(
                            spec.children(aggregate.expr)
                                .filter(|&(role, _)| role == Role::Reducer)
                                .map(|(_, child)| child),
                        );
                    }
                    match &aggregate.key {
                        Some(JoinKey::Columns { columns, .. }) => {
                            out.extend(columns.iter().map(|column| Reading {
                                name: column.name.clone(),
                                kind: Kind::Match,
                            }));
                        }
                        Some(JoinKey::FromInput) => {
                            keyless.insert(at);
                        }
                        _ => {}
                    }
                }
                ExprKind::Odm(odm) => {
                    items.insert(odm.item);
                }
                _ => {}
            }
            bounds.extend(
                spec.children(at)
                    .filter(|&(role, _)| role == Role::RangeBound)
                    .map(|(_, child)| child),
            );
            Step::Continue
        });
        if let Some(failure) = failure {
            return Err(failure);
        }
        let mut joined = BTreeSet::new();
        for read in spec.reads(root, scope) {
            if matches!(read.scope, Scope::Local(_))
                || bounds.contains(&read.at)
                || (reducers.contains(&read.at) && read.name.owner().is_none())
            {
                continue;
            }
            let kind = if items.contains(&read.at) {
                Kind::Item
            } else if predicates.contains(&read.at) {
                Kind::Predicate
            } else if matched.contains(&read.at) {
                Kind::Match
            } else {
                Kind::Variable
            };
            // A qualified read of the current row is an implicit join unless
            // it reads the row's own driver record; a keyless qualified
            // aggregate matches its relation's records on the relation's key,
            // even when that relation drives the rows (LOOKUP-18).
            let joins = |owner: &str| match read.scope {
                Scope::Row => kind != Kind::Item && Some(owner) != driver,
                Scope::Relation(aggregate) => keyless.contains(&aggregate),
                Scope::Donor(_) | Scope::Local(_) => false,
            };
            if let Some(owner) = read.name.owner().filter(|&owner| joins(owner)) {
                joined.insert(owner.to_string());
            }
            out.push(Reading {
                name: read.name.0,
                kind,
            });
        }
        for owner in &joined {
            self.key_reads(owner, out);
        }
        Ok(())
    }

    /// The reads that match a lookup's records to the current row: its key
    /// and the reference its nearest selection measures to, its range value,
    /// its filter, its order and the source its nearest selection measures
    /// from (LOOKUP-25, LOOKUP-26). That source, like a range bound, names a
    /// donor field, which is a completed row's column only over `SELF`, as a
    /// bare order term there is (LOOKUP-7).
    fn matching(&self, index: usize, out: &mut Vec<Reading>) -> Result<(), Error> {
        let item = &self.spec.intermediates[index];
        // A pair relation matches its left record, not a current row: its key
        // values and its filter read its sides' records (LOOKUP-27).
        if item.pairs() {
            let donor = Scope::Donor(index);
            if let Some(JoinKey::Pairs { pairs, .. }) = &item.key {
                for &(_, value) in pairs {
                    self.read(value, donor, out)?;
                }
            }
            if let Some(filter) = item.filter {
                self.read(filter, donor, out)?;
            }
            return Ok(());
        }
        if let Some(JoinKey::Columns { columns, .. }) = &item.key {
            out.extend(columns.iter().map(Reading::variable));
        }
        for value in item.match_values() {
            self.read(value, Scope::Row, out)?;
        }
        if item.dataset.name == "SELF" {
            out.extend(
                item.nearest
                    .iter()
                    .map(|nearest| Reading::variable(&nearest.source)),
            );
        }
        if let Some(between) = &item.between {
            self.read(between.value, Scope::Row, out)?;
        }
        let donor = Scope::Donor(index);
        if let Some(filter) = item.filter {
            self.read(filter, donor, out)?;
        }
        for term in &item.order_by {
            self.read(term.variable, donor, out)?;
        }
        Ok(())
    }

    fn declares_input(&self, name: &str) -> bool {
        self.spec.inputs.iter().any(|input| input.name.name == name)
    }

    /// The last intermediate declared as `name`.
    fn intermediate(&self, name: &str) -> Option<usize> {
        self.spec
            .intermediates
            .iter()
            .rposition(|item| item.id.name == name)
    }

    /// Make what one reading names live: a column, or the input and the
    /// lookup a qualifier names (both, so that naming validation reports a
    /// collision), or an ODM item's input.
    fn mark(&mut self, reading: &Reading) -> bool {
        let qualifier = reading.name.split_once('.').map(|(owner, _)| owner);
        match (reading.kind, qualifier) {
            (Kind::Item, Some(dataset)) => self.inputs.insert(dataset.into()),
            (Kind::Item, None) => false,
            (_, None) => self.columns.insert(reading.name.clone()),
            (_, Some(owner)) => {
                let mut changed = false;
                if self.declares_input(owner) {
                    changed |= self.inputs.insert(owner.into());
                }
                if self.intermediate(owner).is_some() {
                    changed |= self.intermediates.insert(owner.into());
                }
                changed
            }
        }
    }

    fn mark_all(&mut self, readings: &[Reading]) -> bool {
        let mut changed = false;
        for reading in readings {
            changed |= self.mark(reading);
        }
        changed
    }

    fn follow(&mut self, root: ExprId, scope: Scope) -> Result<bool, Error> {
        self.follow_driven(root, scope, self.driver)
    }

    fn follow_driven(
        &mut self,
        root: ExprId,
        scope: Scope,
        driver: Option<&str>,
    ) -> Result<bool, Error> {
        let mut readings = Vec::new();
        self.read_driven(root, scope, driver, &mut readings)?;
        Ok(self.mark_all(&readings))
    }

    /// The roots of LAYER-11.
    fn roots(&mut self) -> Result<(), Error> {
        let spec = self.spec;
        // The columns `output.columns`, `keys` and `output.order_by` name.
        for named in spec.output.columns.iter().chain(&spec.keys) {
            self.columns.insert(named.name.clone());
        }
        for term in &spec.output.order_by {
            if let ExprKind::Ref(name) = &spec.expr(term.variable).kind {
                self.columns.insert(name.0.clone());
            }
        }
        // Every column and intermediate that carries its own verification. A
        // complete rename-only intermediate also reaches its refusal
        // (LOOKUP-4) when nothing reads it, and so does a 1.x intermediate
        // that drives no row template and declares no `key` (LOOKUP-3, D219),
        // which reads nothing until something reads it.
        for column in &spec.columns {
            if !column.checks.is_empty() {
                self.columns.insert(column.name.name.clone());
            }
        }
        for item in &spec.intermediates {
            if item.checks.is_empty() && spec.lacks_key(item) && !item.pairs() {
                self.refused.insert(item.id.name.clone());
            } else if !item.checks.is_empty()
                || spec.lacks_key(item)
                || (rename_only(spec, item) && !item.dataset.name.is_empty())
            {
                self.intermediates.insert(item.id.name.clone());
            }
        }
        if let Some(base) = &spec.rows.base {
            self.inputs.insert(base.name.clone());
        }
        // The columns dataset verifications read, those a column check's
        // `when` reads (CHECK-5), and the root filter's.
        let mut readings = Vec::new();
        for check in spec.columns.iter().flat_map(|column| &column.checks) {
            if let ColumnCheckKind::NotMissing {
                when: Some(when), ..
            } = &check.kind
            {
                self.read(*when, Scope::Row, &mut readings)?;
            }
        }
        for check in &spec.checks {
            match &check.kind {
                DatasetCheckKind::Unique { columns } | DatasetCheckKind::AllOrNone { columns } => {
                    readings.extend(columns.iter().map(Reading::variable));
                }
                DatasetCheckKind::Assert { when, require } => {
                    for &root in when.iter().chain([require]) {
                        self.read(root, Scope::Row, &mut readings)?;
                    }
                }
                DatasetCheckKind::RowCount(count) => {
                    readings.extend(count.group_by.iter().flatten().map(Reading::variable));
                    for root in [count.filter, count.when].into_iter().flatten() {
                        self.read(root, Scope::Row, &mut readings)?;
                    }
                }
                DatasetCheckKind::Unsupported(_) => {}
            }
        }
        if let Some(filter) = spec.rows.filter {
            self.read(filter, Scope::Row, &mut readings)?;
        }
        self.mark_all(&readings);
        // Every row template: its driver, or `base` when it names none, and
        // what its grouping and filter read.
        for template in &spec.rows.templates {
            if let Some(driver) = template.driver.as_ref().or(spec.rows.base.as_ref()) {
                if self.intermediate(&driver.name).is_some() {
                    self.intermediates.insert(driver.name.clone());
                } else {
                    self.inputs.insert(driver.name.clone());
                }
            }
            let driver = template_driver(template, self.base);
            for &group in template.group_by.iter().flatten() {
                self.follow_driven(group, Scope::Row, driver)?;
            }
            if let Some(filter) = template.filter {
                self.follow_driven(filter, Scope::Row, driver)?;
            }
        }
        Ok(())
    }

    /// Follow every live declaration's reads until nothing more is live.
    fn reach(&mut self) -> Result<(), Error> {
        let spec = self.spec;
        let mut columns = vec![false; spec.columns.len()];
        let mut overrides = spec
            .rows
            .templates
            .iter()
            .map(|template| vec![false; template.overrides.len()])
            .collect::<Vec<_>>();
        let mut intermediates = vec![false; spec.intermediates.len()];
        loop {
            let mut changed = false;
            for (index, column) in spec.columns.iter().enumerate() {
                if columns[index] || !self.columns.contains(&column.name.name) {
                    continue;
                }
                columns[index] = true;
                if let Some(derivation) = &column.derivation {
                    changed |= self.follow(derivation.expr, Scope::Row)?;
                }
            }
            for (template, row) in spec.rows.templates.iter().enumerate() {
                let driver = template_driver(row, self.base);
                for (entry, (name, derivation)) in row.overrides.iter().enumerate() {
                    if overrides[template][entry] || !self.columns.contains(&name.name) {
                        continue;
                    }
                    overrides[template][entry] = true;
                    changed |= self.follow_driven(derivation.expr, Scope::Row, driver)?;
                }
            }
            for (index, item) in spec.intermediates.iter().enumerate() {
                if intermediates[index] || !self.intermediates.contains(&item.id.name) {
                    continue;
                }
                intermediates[index] = true;
                if !item.dataset.name.is_empty() {
                    self.inputs.insert(item.dataset.name.clone());
                }
                // A live intermediate keeps the intermediates whose records it
                // reads: a pair relation its sides, and an intermediate over a
                // pair relation that relation (LOOKUP-27).
                for relation in std::iter::once(&item.dataset).chain(&item.inner_join) {
                    if self.intermediate(&relation.name).is_some() {
                        changed |= self.intermediates.insert(relation.name.clone());
                    } else if !relation.name.is_empty() {
                        self.inputs.insert(relation.name.clone());
                    }
                }
                let mut readings = Vec::new();
                self.matching(index, &mut readings)?;
                for binding in &item.bindings {
                    self.read(binding.derivation.expr, Scope::Donor(index), &mut readings)?;
                }
                for check in &item.checks {
                    readings.extend(check.columns.iter().map(Reading::variable));
                }
                changed |= self.mark_all(&readings);
            }
            if !changed {
                return Ok(());
            }
        }
    }

    /// Whether the intermediate `id` survives pruning: it is live, or it
    /// survives only to reach its refusal.
    fn survives(&self, id: &str) -> bool {
        self.intermediates.contains(id) || self.refused.contains(id)
    }

    /// The live columns in declaration order.
    fn selected(&self) -> Vec<usize> {
        (0..self.spec.columns.len())
            .filter(|&column| self.columns.contains(&self.spec.columns[column].name.name))
            .collect()
    }

    /// A column's default and each row template's own derivation of it, each
    /// with the dataset whose record it reads as the row's own: the shared
    /// driver for the default, and the template's driver for its own.
    fn derivations(
        &self,
        column: usize,
    ) -> impl Iterator<Item = (&'a Derivation, Option<&'a str>)> + 'a {
        let declared = &self.spec.columns[column];
        let (driver, base) = (self.driver, self.base);
        declared
            .derivation
            .iter()
            .map(move |derivation| (derivation, driver))
            .chain(self.spec.rows.templates.iter().filter_map(move |template| {
                template
                    .overrides
                    .iter()
                    .find(|(name, _)| name.name == declared.name.name)
                    .map(|(_, derivation)| (derivation, template_driver(template, base)))
            }))
    }

    /// The columns a column's derivations read, through a live lookup's
    /// matching reads too, and those of them its own derivations name
    /// outside a predicate, which an unknown column fails (LAYER-5).
    fn dependencies(&self, column: usize) -> Result<(BTreeSet<String>, BTreeSet<String>), Error> {
        let mut readings = Vec::new();
        for (derivation, driver) in self.derivations(column) {
            self.read_driven(derivation.expr, Scope::Row, driver, &mut readings)?;
        }
        let mut names = BTreeSet::new();
        let mut diagnosable = BTreeSet::new();
        for reading in readings {
            match (reading.kind, reading.name.split_once('.')) {
                (Kind::Item, _) => {}
                (_, Some((owner, _))) => {
                    let Some(index) = self.intermediate(owner) else {
                        continue;
                    };
                    if !self.intermediates.contains(owner) {
                        continue;
                    }
                    let mut matching = Vec::new();
                    self.matching(index, &mut matching)?;
                    names.extend(
                        matching
                            .into_iter()
                            .filter(|read| read.kind != Kind::Item && !read.name.contains('.'))
                            .map(|read| read.name),
                    );
                }
                (kind, None) => {
                    if kind == Kind::Variable {
                        diagnosable.insert(reading.name.clone());
                    }
                    names.insert(reading.name);
                }
            }
        }
        Ok((names, diagnosable))
    }

    /// LAYER-13: each live column after every column it reads, with the
    /// earliest declaration first among the ready ones.
    fn order(&self) -> Result<Vec<usize>, Error> {
        let selected = self.selected();
        let name = |index: usize| &self.spec.columns[selected[index]].name.name;
        let mut graph = Vec::with_capacity(selected.len());
        let mut issues = Vec::new();
        for (index, &column) in selected.iter().enumerate() {
            let (names, diagnosable) = self.dependencies(column)?;
            let mut edges = Vec::new();
            for dependency in names {
                if let Some(read) = (0..selected.len()).find(|&other| *name(other) == dependency) {
                    edges.push(read);
                } else if diagnosable.contains(&dependency) {
                    issues.push(Issue::Unknown {
                        column: name(index).clone(),
                        dependency,
                    });
                }
            }
            graph.push(edges);
        }
        if !issues.is_empty() {
            return Err(Error::Invalid(issues));
        }
        let analysis = dependency_analysis::analyze_admitted(graph);
        if let Some(cycle) = analysis.cycle {
            let members = &cycle[..cycle.len() - 1];
            return Err(Error::Invalid(vec![Issue::Cycle {
                paths: members
                    .iter()
                    .map(|&index| self.derivation_path(selected[index]))
                    .collect(),
                cycle: cycle.into_iter().map(|index| name(index).clone()).collect(),
            }]));
        }
        Ok(analysis
            .order
            .into_iter()
            .map(|index| selected[index])
            .collect())
    }

    /// A cycle member's path names its operation, as the single-file
    /// compiler cites it, even when only a row template derives it.
    fn derivation_path(&self, column: usize) -> String {
        let name = &self.spec.columns[column].name.name;
        let operation = self.derivations(column).next().and_then(|(derivation, _)| {
            match self
                .spec
                .paths
                .segment(self.spec.expr(derivation.expr).meta.path)
            {
                Segment::Key(operation) => Some(operation),
                _ => None,
            }
        });
        match operation {
            Some(operation) => format!("columns.{name}.derivation.{operation}"),
            None => format!("columns.{name}.derivation"),
        }
    }
}

/// Without a relation change or `require_match`, an intermediate whose
/// absence answer is missing reads what the implicit join reads (LOOKUP-4):
/// one whose `key` lists the columns of its input's declared key (D23, D71).
fn rename_only(spec: &Spec, item: &Intermediate) -> bool {
    let joins_as_input = match &item.key {
        Some(JoinKey::Columns { .. }) => spec.keys_as_input(item),
        Some(JoinKey::Pairs { .. }) => false,
        Some(JoinKey::FromInput) | None => true,
    };
    joins_as_input
        && item.between.is_none()
        && item.filter.is_none()
        && item.order_by.is_empty()
        && item.keep.is_none()
        && item.nearest.is_none()
        && item.columns.is_none()
        && item.bindings.is_empty()
        && item.checks.is_empty()
        && !item.require_match
        && item
            .absent
            .as_ref()
            .is_none_or(|absent| absent.value == Literal::Missing)
}

/// Copy `document` with only its live inputs, intermediates, columns and
/// row derivations, the columns in their resolved order. Every copied node
/// records the node it copies.
struct Rebuild<'a> {
    input: &'a Document,
    live: &'a Live<'a>,
    columns: Option<usize>,
    inputs: Option<usize>,
    intermediates: Option<usize>,
    derivations: Vec<usize>,
    order: Vec<usize>,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
}

impl<'a> Rebuild<'a> {
    fn new(input: &'a Document, live: &'a Live<'a>, order: Vec<usize>) -> Self {
        let root = input.root();
        let columns = input.field(root, "columns");
        let items = |node: Option<usize>| match node.map(|node| &input.nodes()[node]) {
            Some(N::Sequence(items)) => items.as_slice(),
            _ => &[],
        };
        let derivations = items(input.field(root, "rows"))
            .iter()
            .filter_map(|&row| input.field(row, "derivations"))
            .filter(|&node| matches!(input.nodes()[node], N::Mapping(_)))
            .collect();
        let order = order
            .into_iter()
            .filter_map(|column| items(columns).get(column).copied())
            .collect();
        Self {
            input,
            live,
            columns,
            inputs: input.field(root, "input"),
            intermediates: input.field(root, "intermediates"),
            derivations,
            order,
            nodes: Vec::new(),
            origins: Vec::new(),
        }
    }

    fn run(mut self) -> Result<NormalizedDocument, Error> {
        let root = self.copy(self.input.root());
        let document = Document::new(self.nodes, root).map_err(Error::Document)?;
        Ok(NormalizedDocument {
            document,
            origins: self.origins,
        })
    }

    fn text(&self, node: usize) -> Option<&'a str> {
        match &self.input.nodes()[node] {
            N::Text(text) => Some(text),
            _ => None,
        }
    }

    fn copy(&mut self, node: usize) -> usize {
        let input = self.input;
        let copied = match &input.nodes()[node] {
            N::Sequence(_) if Some(node) == self.columns => {
                let order = self.order.clone();
                N::Sequence(order.into_iter().map(|column| self.copy(column)).collect())
            }
            N::Sequence(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for &item in items {
                    if Some(node) == self.intermediates {
                        let id = input.field(item, "id").and_then(|id| self.text(id));
                        if !id.is_some_and(|id| self.live.survives(id)) {
                            continue;
                        }
                    }
                    copied.push(self.copy(item));
                }
                N::Sequence(copied)
            }
            N::Mapping(entries) => {
                let derivations = self.derivations.contains(&node);
                let mut copied = Vec::with_capacity(entries.len());
                for &(key, value) in entries {
                    // A list of intermediates of which none survives goes.
                    if node == input.root()
                        && self.text(key) == Some("intermediates")
                        && self
                            .intermediates
                            .is_some_and(|list| matches!(input.nodes()[list], N::Sequence(_)))
                        && self.live.intermediates.is_empty()
                        && self.live.refused.is_empty()
                    {
                        continue;
                    }
                    if derivations || Some(node) == self.inputs {
                        let live = if derivations {
                            &self.live.columns
                        } else {
                            &self.live.inputs
                        };
                        if !self.text(key).is_some_and(|name| live.contains(name)) {
                            continue;
                        }
                    }
                    copied.push((self.copy(key), self.copy(value)));
                }
                N::Mapping(copied)
            }
            other => other.clone(),
        };
        self.nodes.push(copied);
        self.origins.push(SchemaOrigin {
            source: SchemaSource::Input,
            node,
            generated: false,
        });
        self.nodes.len() - 1
    }
}
