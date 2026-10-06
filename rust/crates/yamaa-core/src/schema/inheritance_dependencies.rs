//! R017 reachability and stable ordering over normalized, composed contributions.

use super::{
    Document, DocumentNode as N, InheritanceReference, InheritanceReferenceError,
    InheritanceReferenceKind as Kind, NormalizationBudget, NormalizationError,
    NormalizationResource, NormalizedDocument, SchemaOrigin, SchemaSource, SchemaStructure,
    ValidationError,
};
use alloc::{string::String, vec, vec::Vec};

/// Ordered semantic findings; hosts render paths without discovering dependencies again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InheritanceDependencyIssue {
    Unknown {
        column: String,
        dependency: String,
    },
    /// Residual unscheduled columns in their initial collection order, including dependents.
    Cycle {
        columns: Vec<String>,
    },
}

/// No partially pruned document is published on semantic or resource failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InheritanceDependencyError {
    Normalization(NormalizationError),
    Reference(InheritanceReferenceError),
    Invalid(Vec<InheritanceDependencyIssue>),
}
impl From<NormalizationError> for InheritanceDependencyError {
    fn from(e: NormalizationError) -> Self {
        Self::Normalization(e)
    }
}
impl From<ValidationError> for InheritanceDependencyError {
    fn from(e: ValidationError) -> Self {
        Self::Normalization(e.into())
    }
}
impl From<InheritanceReferenceError> for InheritanceDependencyError {
    fn from(e: InheritanceReferenceError) -> Self {
        Self::Reference(e)
    }
}
type Error = InheritanceDependencyError;

#[derive(Clone, Copy)]
struct Named<'a> {
    name: &'a str,
    node: usize,
}

struct Run<'a, 'b> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    budget: &'b mut NormalizationBudget,
    inputs: Vec<Named<'a>>,
    columns: Vec<Named<'a>>,
    intermediates: Vec<Named<'a>>,
    rows: Vec<usize>,
    live_columns: Vec<String>,
    live_inputs: Vec<String>,
    live_intermediates: Vec<String>,
    column_node: Option<usize>,
    input_node: Option<usize>,
    intermediate_node: Option<usize>,
    row_derivations: Vec<usize>,
    ordered_columns: Vec<usize>,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
}

fn contains(names: &[String], name: &str, budget: &mut NormalizationBudget) -> Result<bool, Error> {
    budget.work(
        names
            .len()
            .saturating_add(1)
            .saturating_mul(name.len().saturating_add(1)),
    )?;
    Ok(names.iter().any(|n| n == name))
}
fn add(
    names: &mut Vec<String>,
    name: &str,
    budget: &mut NormalizationBudget,
) -> Result<bool, Error> {
    if contains(names, name, budget)? {
        return Ok(false);
    }
    budget.reserve(name.len(), 1)?;
    names.push(name.into());
    Ok(true)
}
fn lookup<'a>(
    items: &[Named<'a>],
    name: &str,
    budget: &mut NormalizationBudget,
) -> Result<Option<Named<'a>>, Error> {
    budget.work(
        items
            .len()
            .saturating_add(1)
            .saturating_mul(name.len().saturating_add(1)),
    )?;
    Ok(items.iter().rev().find(|n| n.name == name).copied())
}

impl<'a> Run<'a, '_> {
    fn field(&mut self, node: usize, name: &str) -> Result<Option<usize>, Error> {
        self.budget.work(
            self.input.nodes()[node]
                .length()
                .unwrap_or(0)
                .saturating_mul(name.len().saturating_add(1)),
        )?;
        Ok(self.input.field(node, name))
    }
    fn text(&self, node: usize) -> Option<&'a str> {
        match &self.input.nodes()[node] {
            N::Text(s) => Some(s),
            _ => None,
        }
    }
    fn named(
        &mut self,
        node: Option<usize>,
        identity: Option<&str>,
    ) -> Result<Vec<Named<'a>>, Error> {
        let Some(node) = node else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        match (&self.input.nodes()[node], identity) {
            (N::Sequence(items), Some(identity)) => {
                for &item in items {
                    if let Some(name) = self.field(item, identity)?.and_then(|id| self.text(id)) {
                        self.budget.reserve(0, 2)?;
                        result.push(Named { name, node: item });
                    }
                }
            }
            (N::Mapping(items), None) => {
                for &(key, value) in items {
                    if let Some(name) = self.text(key) {
                        self.budget.reserve(0, 2)?;
                        result.push(Named { name, node: value });
                    }
                }
            }
            _ => {}
        }
        Ok(result)
    }
    fn initialize(&mut self) -> Result<(), Error> {
        let root = self.input.root();
        self.column_node = self.field(root, "columns")?;
        self.input_node = self.field(root, "input")?;
        self.intermediate_node = self.field(root, "intermediates")?;
        self.columns = self.named(self.column_node, Some("name"))?;
        self.inputs = self.named(self.input_node, None)?;
        self.intermediates = self.named(self.intermediate_node, Some("id"))?;
        if let Some(rows) = self.field(root, "rows")? {
            if let N::Sequence(items) = &self.input.nodes()[rows] {
                self.budget.reserve(0, items.len())?;
                self.rows = items.clone();
                for &row in items {
                    if let Some(id) = self.field(row, "derivations")? {
                        if matches!(self.input.nodes()[id], N::Mapping(_)) {
                            self.budget.reserve(0, 1)?;
                            self.row_derivations.push(id);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn refs_field(
        &mut self,
        node: usize,
        class: &str,
        name: &str,
        scoped: bool,
    ) -> Result<Vec<InheritanceReference>, Error> {
        let Some(value) = self.field(node, name)? else {
            return Ok(Vec::new());
        };
        let Some(class_def) = self.schema.class_named(class) else {
            return Ok(Vec::new());
        };
        self.budget.work(
            class_def
                .fields
                .len()
                .saturating_mul(name.len().saturating_add(1)),
        )?;
        let Some(field) = class_def.fields.iter().find(|f| f.name == name) else {
            return Ok(Vec::new());
        };
        Ok(self.schema.inheritance_references(
            self.schema.descriptors()[field.descriptor]
                .descriptor
                .members(),
            self.input,
            value,
            scoped.then_some((class, name)),
            self.budget,
        )?)
    }
    fn refs_derivation(&mut self, node: usize) -> Result<Vec<InheritanceReference>, Error> {
        let types = self
            .schema
            .parse_query_types(&["derivation".into()], self.budget.validation_scope())?;
        Ok(self
            .schema
            .inheritance_references(&types, self.input, node, None, self.budget)?)
    }
    fn mark(&mut self, reference: &InheritanceReference) -> Result<bool, Error> {
        if reference.kind == Kind::Dataset {
            return add(&mut self.live_inputs, &reference.name, self.budget);
        }
        let Some((qualifier, _)) = reference.name.split_once('.') else {
            return add(&mut self.live_columns, &reference.name, self.budget);
        };
        if lookup(&self.inputs, qualifier, self.budget)?.is_some() {
            add(&mut self.live_inputs, qualifier, self.budget)
        } else if lookup(&self.intermediates, qualifier, self.budget)?.is_some() {
            add(&mut self.live_intermediates, qualifier, self.budget)
        } else {
            Ok(false)
        }
    }
    fn mark_all(&mut self, refs: Vec<InheritanceReference>) -> Result<bool, Error> {
        let mut changed = false;
        for reference in refs {
            changed |= self.mark(&reference)?;
        }
        Ok(changed)
    }
    fn seed_list(&mut self, node: Option<usize>, ordering: bool) -> Result<(), Error> {
        if let Some(node) = node {
            if let N::Sequence(items) = &self.input.nodes()[node] {
                for &item in items {
                    let candidate = if ordering && matches!(self.input.nodes()[item], N::Mapping(_))
                    {
                        self.field(item, "variable")?
                    } else {
                        Some(item)
                    };
                    if let Some(name) = candidate.and_then(|id| self.text(id)) {
                        add(&mut self.live_columns, name, self.budget)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn roots(&mut self) -> Result<(), Error> {
        let root = self.input.root();
        if let Some(output) = self.field(root, "output")? {
            let columns = self.field(output, "columns")?;
            self.seed_list(columns, false)?;
            let order = self.field(output, "order_by")?;
            self.seed_list(order, true)?;
        }
        let keys = self.field(root, "keys")?;
        self.seed_list(keys, false)?;
        for i in 0..self.columns.len() {
            let column = self.columns[i];
            if self.field(column.node, "verifications")?.is_some() {
                add(&mut self.live_columns, column.name, self.budget)?;
            }
        }
        let base = self.field(root, "base")?;
        if let Some(name) = base.and_then(|id| self.text(id)) {
            add(&mut self.live_inputs, name, self.budget)?;
        }
        for name in ["verifications", "filter"] {
            let refs = self.refs_field(root, "root_class", name, false)?;
            self.mark_all(refs)?;
        }
        for i in 0..self.rows.len() {
            let row = self.rows[i];
            let driver = self.field(row, "dataset")?.or(base);
            if let Some(name) = driver.and_then(|id| self.text(id)) {
                if lookup(&self.intermediates, name, self.budget)?.is_some() {
                    add(&mut self.live_intermediates, name, self.budget)?;
                } else {
                    add(&mut self.live_inputs, name, self.budget)?;
                }
            }
            for name in ["group_by", "filter"] {
                let refs = self.refs_field(row, "row_class", name, false)?;
                self.mark_all(refs)?;
            }
        }
        Ok(())
    }
    fn reachable(&mut self) -> Result<(), Error> {
        self.roots()?;
        self.budget.reserve(
            0,
            self.columns.len().saturating_add(self.intermediates.len()),
        )?;
        let mut done_columns = vec![false; self.columns.len()];
        let mut done_intermediates = vec![false; self.intermediates.len()];
        let mut done_rows: Vec<(usize, usize)> = Vec::new();
        loop {
            self.budget.work(1)?;
            let mut changed = false;
            for (i, done) in done_columns.iter_mut().enumerate() {
                let col = self.columns[i];
                if *done || !contains(&self.live_columns, col.name, self.budget)? {
                    continue;
                }
                *done = true;
                for name in ["derivation", "verifications"] {
                    let refs = self.refs_field(col.node, "column_class", name, true)?;
                    changed |= self.mark_all(refs)?;
                }
            }
            for i in 0..self.row_derivations.len() {
                let node = self.row_derivations[i];
                let N::Mapping(items) = &self.input.nodes()[node] else {
                    unreachable!()
                };
                for &(key, value) in items {
                    let Some(name) = self.text(key) else { continue };
                    self.budget.work(done_rows.len().saturating_add(1))?;
                    if done_rows.contains(&(node, key))
                        || !contains(&self.live_columns, name, self.budget)?
                    {
                        continue;
                    }
                    self.budget.reserve(0, 2)?;
                    done_rows.push((node, key));
                    let refs = self.refs_derivation(value)?;
                    changed |= self.mark_all(refs)?;
                }
            }
            for (i, done) in done_intermediates.iter_mut().enumerate() {
                let member = self.intermediates[i];
                if *done || !contains(&self.live_intermediates, member.name, self.budget)? {
                    continue;
                }
                *done = true;
                if let Some(name) = self
                    .field(member.node, "dataset")?
                    .and_then(|id| self.text(id))
                {
                    add(&mut self.live_inputs, name, self.budget)?;
                }
                if let N::Mapping(items) = &self.input.nodes()[member.node] {
                    for &(key, _) in items {
                        let Some(name) = self.text(key) else { continue };
                        if matches!(name, "id" | "dataset") {
                            continue;
                        }
                        let refs =
                            self.refs_field(member.node, "intermediate_class", name, false)?;
                        changed |= self.mark_all(refs)?;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Ok(())
    }
    fn column_dependencies(&mut self, column: Named<'a>) -> Result<Vec<String>, Error> {
        let mut refs = Vec::new();
        if let Some(node) = self.field(column.node, "derivation")? {
            refs = self.refs_derivation(node)?;
        }
        for i in 0..self.row_derivations.len() {
            if let Some(node) = self.field(self.row_derivations[i], column.name)? {
                let additional = self.refs_derivation(node)?;
                self.budget.reserve(0, additional.len())?;
                refs.extend(additional);
            }
        }
        let mut names = Vec::new();
        for reference in refs {
            if reference.kind != Kind::Variable {
                continue;
            }
            if let Some((qualifier, _)) = reference.name.split_once('.') {
                if let Some(intermediate) = lookup(&self.intermediates, qualifier, self.budget)? {
                    if !contains(&self.live_intermediates, qualifier, self.budget)? {
                        continue;
                    }
                    for field in ["key", "between", "filter", "order_by"] {
                        for r in
                            self.refs_field(intermediate.node, "intermediate_class", field, true)?
                        {
                            if r.kind == Kind::Variable && !r.name.contains('.') {
                                add(&mut names, &r.name, self.budget)?;
                            }
                        }
                    }
                }
            } else {
                add(&mut names, &reference.name, self.budget)?;
            }
        }
        // Reserve comparisons before the sort; diagnostics use lexical dependency order.
        let chars = names
            .iter()
            .fold(0usize, |n, s| n.saturating_add(s.len().saturating_add(1)));
        self.budget
            .work(chars.saturating_mul(names.len().saturating_add(1)))?;
        names.sort();
        Ok(names)
    }
    fn order(&mut self) -> Result<(), Error> {
        let mut selected = Vec::new();
        for i in 0..self.columns.len() {
            if contains(&self.live_columns, self.columns[i].name, self.budget)? {
                self.budget.reserve(0, 1)?;
                selected.push(self.columns[i]);
            }
        }
        let mut dependencies: Vec<Vec<usize>> = Vec::new();
        let mut issues = Vec::new();
        for &column in &selected {
            let names = self.column_dependencies(column)?;
            self.budget.reserve(0, names.len())?;
            let mut indices = Vec::new();
            for dependency in names {
                self.budget.work(
                    selected
                        .len()
                        .saturating_mul(dependency.len().saturating_add(1)),
                )?;
                if let Some(index) = selected.iter().position(|c| c.name == dependency) {
                    indices.push(index);
                } else {
                    self.budget
                        .reserve(column.name.len().saturating_add(dependency.len()), 2)?;
                    // Account for the rendered path and both owned context values.
                    self.budget.validation.reserve_diagnostic(
                        column
                            .name
                            .len()
                            .saturating_mul(2)
                            .saturating_add(dependency.len())
                            .saturating_add(40),
                    )?;
                    issues.push(InheritanceDependencyIssue::Unknown {
                        column: column.name.into(),
                        dependency,
                    });
                }
            }
            dependencies.push(indices);
        }
        if !issues.is_empty() {
            return Err(Error::Invalid(issues));
        }
        self.budget.reserve(0, selected.len())?;
        let mut emitted = vec![false; selected.len()];
        loop {
            let mut next = None;
            for (i, edges) in dependencies.iter().enumerate() {
                self.budget.work(edges.len().saturating_add(1))?;
                if !emitted[i] && edges.iter().all(|&j| emitted[j]) {
                    next = Some(i);
                    break;
                }
            }
            let Some(i) = next else { break };
            emitted[i] = true;
            self.budget.reserve(0, 1)?;
            self.ordered_columns.push(selected[i].node);
        }
        if self.ordered_columns.len() != selected.len() {
            let mut columns = Vec::new();
            for (i, column) in selected.iter().enumerate() {
                if !emitted[i] {
                    self.budget.reserve(column.name.len(), 1)?;
                    columns.push(column.name.into());
                }
            }
            let text_bytes = columns.iter().fold(12usize, |total, name: &String| {
                total.saturating_add(name.len())
            });
            self.budget.validation.reserve_diagnostic(text_bytes)?;
            return Err(Error::Invalid(vec![InheritanceDependencyIssue::Cycle {
                columns,
            }]));
        }
        Ok(())
    }
    fn copy(&mut self, node: usize, depth: usize) -> Result<usize, Error> {
        let limit = self.budget.limits.depth.min(128);
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            }
            .into());
        }
        self.budget.work(1)?;
        let source = &self.input.nodes()[node];
        let edges = match source {
            N::Sequence(v) => v.len(),
            N::Mapping(v) => v.len().saturating_mul(2),
            _ => 0,
        };
        let bytes = match source {
            N::Text(s) | N::Integer(s) => s.len(),
            _ => 0,
        };
        self.budget.reserve(bytes, edges)?;
        let result = match source {
            N::Sequence(items) => {
                let mut output = Vec::new();
                if Some(node) == self.column_node {
                    for i in 0..self.ordered_columns.len() {
                        output.push(self.copy(self.ordered_columns[i], depth + 1)?);
                    }
                } else {
                    for &item in items {
                        if Some(node) == self.intermediate_node {
                            let name = self.field(item, "id")?.and_then(|id| self.text(id));
                            if !match name {
                                Some(n) => contains(&self.live_intermediates, n, self.budget)?,
                                None => false,
                            } {
                                continue;
                            }
                        }
                        output.push(self.copy(item, depth + 1)?);
                    }
                }
                N::Sequence(output)
            }
            N::Mapping(items) => {
                self.budget.work(self.row_derivations.len())?;
                let row_derivation = self.row_derivations.contains(&node);
                let mut output = Vec::new();
                for &(key, value) in items {
                    if node == self.input.root()
                        && self.text(key) == Some("intermediates")
                        && self
                            .intermediate_node
                            .is_some_and(|i| matches!(self.input.nodes()[i], N::Sequence(_)))
                        && self.live_intermediates.is_empty()
                    {
                        continue;
                    }
                    if Some(node) == self.input_node || row_derivation {
                        let name = self.text(key);
                        let live = if row_derivation {
                            &self.live_columns
                        } else {
                            &self.live_inputs
                        };
                        if !match name {
                            Some(n) => contains(live, n, self.budget)?,
                            None => false,
                        } {
                            continue;
                        }
                    }
                    output.push((self.copy(key, depth + 1)?, self.copy(value, depth + 1)?));
                }
                N::Mapping(output)
            }
            _ => source.clone(),
        };
        let result_node = self.nodes.len();
        self.nodes.push(result);
        self.origins.push(SchemaOrigin {
            source: SchemaSource::Input,
            node,
            generated: false,
        });
        Ok(result_node)
    }
}

impl SchemaStructure {
    /// Prune a composed contribution graph, then order its surviving columns (REQ-0638–0644).
    ///
    /// Call after non-strict named-window expansion and before strict expansion and complete
    /// validation. This operation does not discover files, read tables or execute expressions.
    pub fn resolve_inheritance_dependencies(
        &self,
        input: &Document,
        budget: &mut NormalizationBudget,
    ) -> Result<NormalizedDocument, InheritanceDependencyError> {
        if !matches!(input.nodes()[input.root()], N::Mapping(_)) {
            return Err(ValidationError::InvalidDescriptor.into());
        }
        let mut run = Run {
            schema: self,
            input,
            budget,
            inputs: Vec::new(),
            columns: Vec::new(),
            intermediates: Vec::new(),
            rows: Vec::new(),
            live_columns: Vec::new(),
            live_inputs: Vec::new(),
            live_intermediates: Vec::new(),
            column_node: None,
            input_node: None,
            intermediate_node: None,
            row_derivations: Vec::new(),
            ordered_columns: Vec::new(),
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        run.initialize()?;
        run.reachable()?;
        run.order()?;
        let root = run.copy(input.root(), 0)?;
        run.budget.work(run.nodes.len())?;
        let document = Document::new(run.nodes, root, run.budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        Ok(NormalizedDocument {
            document,
            origins: run.origins,
        })
    }
}
