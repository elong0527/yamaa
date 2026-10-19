//! The one resolution lifecycle after composition: expand named windows,
//! prune (LAYER-11), order (LAYER-13), normalize, expand strictly and admit
//! the model. A spec that declares no `parent` is a composition of one layer
//! (REQ-0614) and takes the same lifecycle, keeping its declaration order
//! (REQ-0060); only a layered spec is ordered.
//!
//! Pruning reads the AST. The expanded document is lowered once
//! ([`lower_document`]); the live set is a fixpoint over [`Spec::reads`]
//! from the semantic roots; and the order runs the one cycle detector,
//! [`dependency_analysis`], over the columns each derivation reads, so a
//! layered cycle names its closed loop as a single-file one does. No table
//! is read, no function activated and nothing published.

use crate::ast::{
    DatasetCheckKind, Derivation, ExprId, ExprKind, Intermediate, JoinKey, Literal, Named, Role,
    Scope, Segment, Spec, Step,
};
use crate::dependency_analysis;
use crate::expression::{CompileError, Context};
use crate::front_end::v0_5::lower_document;
use crate::schema::{
    Document, DocumentError, DocumentNode as N, LayerNodeOrigin, NormalizationError,
    NormalizedDocument, SchemaDiagnostic, SchemaOrigin, SchemaSource, SchemaStructure,
    SpecificationDocument, ValidationError, WindowReference,
};
use std::collections::BTreeSet;

/// How the surviving columns are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Declaration order, for a spec without `parent` (REQ-0060).
    Declared,
    /// Dependency order, the earliest declaration first (LAYER-13).
    Dependencies,
}

/// A dependency finding of the order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    /// A derivation reads a name no column declares (REQ-0070).
    Unknown { column: String, dependency: String },
    /// The first cycle as a closed loop of column names, its last entry
    /// repeating the first, and the derivation path of each member (REQ-0072).
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

/// Keep the declarations reachable from the semantic roots (LAYER-11), with
/// the surviving columns in `order`, and each node's origin in `document`.
pub fn prune(document: &Document, order: Order) -> Result<NormalizedDocument, Error> {
    let lowered = lower_document(document, None);
    let mut live = Live::new(lowered.spec());
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
    /// An ODM item, which names its dataset before the first dot (REQ-1265).
    Item,
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

/// The live declarations, by name, as reads find them.
struct Live<'a> {
    spec: &'a Spec,
    columns: BTreeSet<String>,
    inputs: BTreeSet<String>,
    intermediates: BTreeSet<String>,
}

impl<'a> Live<'a> {
    fn new(spec: &'a Spec) -> Self {
        Self {
            spec,
            columns: BTreeSet::new(),
            inputs: BTreeSet::new(),
            intermediates: BTreeSet::new(),
        }
    }

    /// The names `root` reads in `scope` ([`Spec::reads`]), except an
    /// aggregate's own bindings and a range bound, which names a donor field;
    /// and the key columns of an aggregate, each paired with the current
    /// row's same-named value.
    fn read(&self, root: ExprId, scope: Scope, out: &mut Vec<Reading>) -> Result<(), Error> {
        let spec = self.spec;
        let mut bounds = BTreeSet::new();
        let mut predicates = BTreeSet::new();
        let mut items = BTreeSet::new();
        let mut failure = None;
        spec.walk(root, scope, |at, _| {
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
                    if let Some(JoinKey::Columns { columns, .. }) = &aggregate.key {
                        out.extend(columns.iter().map(Reading::variable));
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
        for read in spec.reads(root, scope) {
            if matches!(read.scope, Scope::Local(_)) || bounds.contains(&read.at) {
                continue;
            }
            let kind = if items.contains(&read.at) {
                Kind::Item
            } else if predicates.contains(&read.at) {
                Kind::Predicate
            } else {
                Kind::Variable
            };
            out.push(Reading {
                name: read.name.0,
                kind,
            });
        }
        Ok(())
    }

    /// The reads that match a lookup's records to the current row: its key,
    /// its range value, its filter and its order (LOOKUP-25).
    fn matching(&self, index: usize, out: &mut Vec<Reading>) -> Result<(), Error> {
        let item = &self.spec.intermediates[index];
        match &item.key {
            Some(JoinKey::Columns { columns, .. }) => {
                out.extend(columns.iter().map(Reading::variable));
            }
            Some(JoinKey::Pairs { pairs, .. }) => {
                for &(_, value) in pairs {
                    self.read(value, Scope::Row, out)?;
                }
            }
            _ => {}
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
        let mut readings = Vec::new();
        self.read(root, scope, &mut readings)?;
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
        // (REQ-1248) when nothing reads it.
        for column in &spec.columns {
            if !column.checks.is_empty() {
                self.columns.insert(column.name.name.clone());
            }
        }
        for item in &spec.intermediates {
            if !item.checks.is_empty() || (rename_only(item) && !item.dataset.name.is_empty()) {
                self.intermediates.insert(item.id.name.clone());
            }
        }
        if let Some(base) = &spec.rows.base {
            self.inputs.insert(base.name.clone());
        }
        // The columns dataset verifications read, and the root filter's.
        let mut readings = Vec::new();
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
                    readings.extend(count.group_by.iter().map(Reading::variable));
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
            for &group in template.group_by.iter().flatten() {
                self.follow(group, Scope::Row)?;
            }
            if let Some(filter) = template.filter {
                self.follow(filter, Scope::Row)?;
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
                for (entry, (name, derivation)) in row.overrides.iter().enumerate() {
                    if overrides[template][entry] || !self.columns.contains(&name.name) {
                        continue;
                    }
                    overrides[template][entry] = true;
                    changed |= self.follow(derivation.expr, Scope::Row)?;
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

    /// The live columns in declaration order.
    fn selected(&self) -> Vec<usize> {
        (0..self.spec.columns.len())
            .filter(|&column| self.columns.contains(&self.spec.columns[column].name.name))
            .collect()
    }

    /// A column's default and each row template's own derivation of it.
    fn derivations(&self, column: usize) -> impl Iterator<Item = &'a Derivation> + 'a {
        let declared = &self.spec.columns[column];
        declared
            .derivation
            .iter()
            .chain(self.spec.rows.templates.iter().filter_map(move |template| {
                template
                    .overrides
                    .iter()
                    .find(|(name, _)| name.name == declared.name.name)
                    .map(|(_, derivation)| derivation)
            }))
    }

    /// The columns a column's derivations read, through a live lookup's
    /// matching reads too, and those of them its own derivations name
    /// outside a predicate, which an unknown column fails (REQ-0627).
    fn dependencies(&self, column: usize) -> Result<(BTreeSet<String>, BTreeSet<String>), Error> {
        let mut readings = Vec::new();
        for derivation in self.derivations(column) {
            self.read(derivation.expr, Scope::Row, &mut readings)?;
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
        let operation = self.derivations(column).next().and_then(|derivation| {
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
/// absence answer is missing reads what the implicit join reads (LOOKUP-4).
fn rename_only(item: &Intermediate) -> bool {
    !matches!(
        item.key,
        Some(JoinKey::Columns { .. } | JoinKey::Pairs { .. })
    ) && item.between.is_none()
        && item.filter.is_none()
        && item.order_by.is_empty()
        && item.keep.is_none()
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
                        if !id.is_some_and(|id| self.live.intermediates.contains(id)) {
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

/// An origin each lifecycle traces through every pass: a standalone spec's
/// raw node, or a layered spec's contributing layer.
pub trait Origin: Copy {
    /// Where a pass output came from, through the pass input's origins.
    fn through(origin: SchemaOrigin, prior: &[Self]) -> Self;
}

impl Origin for SchemaOrigin {
    fn through(origin: SchemaOrigin, prior: &[Self]) -> Self {
        match origin.source {
            SchemaSource::Input => {
                let mut mapped = prior[origin.node];
                mapped.generated |= origin.generated;
                mapped
            }
            SchemaSource::Default { .. } => origin,
        }
    }
}

impl Origin for LayerNodeOrigin {
    fn through(origin: SchemaOrigin, prior: &[Self]) -> Self {
        LayerNodeOrigin::through(origin, prior)
    }
}

/// The admitted model with what each lifecycle retains from its passes.
#[derive(Debug)]
pub struct Resolved<O> {
    pub model: SpecificationDocument,
    /// The pruned document, which final normalization reads.
    pub pruned: Document,
    /// Each model node's origin in `pruned`, or its schema default.
    pub origins: Vec<SchemaOrigin>,
    /// Each model node's origin, traced to the caller's arena.
    pub traced: Vec<O>,
    /// The named-window references whose declaring column or intermediate
    /// survived pruning; the strict expansion finds none left to record.
    pub windows: Vec<WindowReference>,
}

/// A failed pass, with the document it read: its findings address it.
#[derive(Debug)]
pub enum Failure {
    Normalize {
        error: NormalizationError,
        input: Document,
    },
    Prune(Error),
    Validation(ValidationError),
    /// Final model findings have no document-value references.
    Model(Vec<SchemaDiagnostic>),
}

/// Resolve a composed document whose nodes `origins` trace: expand named
/// windows, prune, normalize the resolved document, expand strictly and
/// admit the model. Every null field is already absent (LAYER-9).
pub fn resolve<O: Origin>(
    schema: &SchemaStructure,
    composed: Document,
    origins: &[O],
    order: Order,
) -> Result<Resolved<O>, Failure> {
    let expanded = schema
        .expand_named_windows(&composed, false)
        .map_err(|error| Failure::Normalize {
            error,
            input: composed,
        })?;
    let expanded_origins = expanded
        .origins
        .iter()
        .map(|&node| origins[node])
        .collect::<Vec<_>>();
    let pruned = prune(&expanded.document, order).map_err(Failure::Prune)?;
    let pruned_origins = pruned
        .origins
        .iter()
        .map(|&origin| O::through(origin, &expanded_origins))
        .collect::<Vec<_>>();
    let normalized = match schema.normalize_resolved_document(&pruned.document) {
        Ok(normalized) => normalized,
        Err(error) => {
            return Err(Failure::Normalize {
                error,
                input: pruned.document,
            })
        }
    };
    let normalized_origins = normalized
        .origins
        .iter()
        .map(|&origin| O::through(origin, &pruned_origins))
        .collect::<Vec<_>>();
    let strict = schema
        .expand_named_windows(&normalized.document, true)
        .map_err(|error| Failure::Normalize {
            error,
            input: normalized.document,
        })?;
    let traced = strict
        .origins
        .iter()
        .map(|&node| normalized_origins[node])
        .collect();
    let final_origins = strict
        .origins
        .iter()
        .map(|&node| normalized.origins[node])
        .collect();
    let model = SpecificationDocument::admit(strict.document)
        .map_err(Failure::Validation)?
        .map_err(Failure::Model)?;
    let windows = surviving(&pruned.document, expanded.references);
    Ok(Resolved {
        model,
        pruned: pruned.document,
        origins: final_origins,
        traced,
        windows,
    })
}

/// Keep the window references whose declaring column or intermediate is
/// still in the pruned document; other references are kept as recorded.
fn surviving(pruned: &Document, references: Vec<WindowReference>) -> Vec<WindowReference> {
    let declared = |field: &str, identity: &str| -> Vec<&str> {
        let Some(list) = pruned.field(pruned.root(), field) else {
            return Vec::new();
        };
        let N::Sequence(items) = &pruned.nodes()[list] else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(
                |&item| match &pruned.nodes()[pruned.field(item, identity)?] {
                    N::Text(name) => Some(name.as_str()),
                    _ => None,
                },
            )
            .collect()
    };
    let columns = declared("columns", "name");
    let intermediates = declared("intermediates", "id");
    references
        .into_iter()
        .filter(|reference| {
            for (prefix, names) in [("columns.", &columns), ("intermediates.", &intermediates)] {
                if let Some(rest) = reference.path.strip_prefix(prefix) {
                    let name = rest.split('.').next().unwrap_or(rest);
                    return names.contains(&name);
                }
            }
            true
        })
        .collect()
}
