//! Indexed width expressions. Choices are shared by consumption and captures;
//! range queries split only sums whose terms share a choice. The interpreter,
//! dependency walk and destruction all use flat arenas, including long chains
//! of capture references which do not increase grammar nesting.
use super::{CompileError, Extent};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

pub(super) type Id = usize;
const CAP: u128 = usize::MAX as u128 + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Span {
    min: u128,
    max: Option<u128>,
}
impl Span {
    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.zip(other.max).map(|(a, b)| a.max(b)),
        }
    }
}
fn add(a: u128, b: u128) -> u128 {
    a.saturating_add(b).min(CAP)
}
fn times(a: u128, b: usize) -> u128 {
    a.saturating_mul(b as u128).min(CAP)
}

/// Besides the union interval, keep the greatest path minimum and finite
/// maximum. An unbounded alternative must not hide another path's overflow.
#[derive(Clone, Copy, Debug)]
struct Summary {
    span: Span,
    greatest_min: u128,
    finite_max: Option<u128>,
}
impl Summary {
    fn one(span: Span) -> Self {
        Self {
            span,
            greatest_min: span.min,
            finite_max: span.max,
        }
    }
    fn union(self, other: Self) -> Self {
        Self {
            span: self.span.union(other.span),
            greatest_min: self.greatest_min.max(other.greatest_min),
            finite_max: match (self.finite_max, other.finite_max) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
        }
    }
    fn add(self, other: Self) -> Self {
        Self {
            span: Span {
                min: add(self.span.min, other.span.min),
                max: self.span.max.zip(other.span.max).map(|(a, b)| add(a, b)),
            },
            greatest_min: add(self.greatest_min, other.greatest_min),
            finite_max: self
                .finite_max
                .zip(other.finite_max)
                .map(|(a, b)| add(a, b)),
        }
    }
    fn times(self, count: usize) -> Self {
        if count == 0 {
            return Self::one(Span {
                min: 0,
                max: Some(0),
            });
        }
        Self {
            span: Span {
                min: times(self.span.min, count),
                max: self.span.max.map(|width| times(width, count)),
            },
            greatest_min: times(self.greatest_min, count),
            finite_max: self.finite_max.map(|width| times(width, count)),
        }
    }
    fn optional(self, maximum: Option<usize>) -> Self {
        if maximum == Some(0) {
            return self.times(0);
        }
        Self {
            span: Span {
                min: 0,
                max: self
                    .span
                    .max
                    .zip(maximum)
                    .map(|(width, count)| times(width, count)),
            },
            greatest_min: 0,
            finite_max: self
                .finite_max
                .zip(maximum)
                .map(|(width, count)| times(width, count)),
        }
    }
    fn checked(self, byte: usize) -> Result<Extent, CompileError> {
        if self.greatest_min >= CAP || self.finite_max.is_some_and(|max| max >= CAP) {
            return Err(CompileError::Unsupported {
                byte,
                feature: "pattern width exceeds addressable range",
            });
        }
        Ok(Extent {
            min: self.span.min as usize,
            max: self.span.max.map(|max| max as usize),
        })
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Expression {
    Value(Span),
    Add(Id, Id),
    Choice(usize, Vec<Id>),
    /// Pointwise interval union, rather than a new capture-visible choice.
    Union(Vec<Id>),
    Times(Id, usize),
    Optional(Id, Option<usize>),
    Positive(Id),
    Valid(Id),
    /// Union body-local choices while retaining incoming capture dependencies.
    Quantify(Id, usize, usize),
}
struct Entry {
    expression: Expression,
    bounds: Option<Summary>,
    constant: bool,
    total: bool,
    free: Option<(usize, usize)>,
}
fn join_span(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> Option<(usize, usize)> {
    match (a, b) {
        (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
        (a, b) => a.or(b),
    }
}
fn remove_span(span: Option<(usize, usize)>, bound: &Range<usize>) -> Option<(usize, usize)> {
    span.and_then(|(mut first, mut last)| {
        if bound.contains(&first) {
            first = bound.end;
        }
        if bound.contains(&last) {
            last = bound.start.saturating_sub(1);
            if bound.start == 0 {
                return None;
            }
        }
        (first <= last).then_some((first, last))
    })
}

fn component_root(parent: &[usize], mut id: usize) -> usize {
    while parent[id] != id {
        id = parent[id];
    }
    id
}

pub(super) struct Arena {
    entries: Vec<Entry>,
    identities: BTreeMap<Expression, Id>,
    choices: Vec<usize>,
    global: Vec<[Option<Option<Summary>>; 2]>,
    fixed: Vec<Option<Fixed>>,
}
impl Arena {
    pub fn new() -> Self {
        let mut result = Self {
            entries: Vec::new(),
            identities: BTreeMap::new(),
            choices: Vec::new(),
            global: Vec::new(),
            fixed: Vec::new(),
        };
        result.value(Extent::ZERO);
        result
    }
    fn push(&mut self, entry: Entry) -> Id {
        if let Some(&id) = self.identities.get(&entry.expression) {
            return id;
        }
        let id = self.entries.len();
        self.identities.insert(entry.expression.clone(), id);
        self.entries.push(entry);
        self.global.push([None; 2]);
        self.fixed.push(None);
        id
    }
    pub fn value(&mut self, extent: Extent) -> Id {
        self.constant(Some(Summary::one(Span {
            min: extent.min as u128,
            max: extent.max.map(|max| max as u128),
        })))
    }
    fn constant(&mut self, value: Option<Summary>) -> Id {
        if value.is_some_and(|value| value.span.min == 0 && value.span.max == Some(0))
            && !self.entries.is_empty()
        {
            return 0;
        }
        // Bottom has no successful valuation. Valid/Guard preserve it through
        // Group and Look resets without allocating a parallel capture state.
        let expression = value.map_or(Expression::Positive(0), |value| {
            Expression::Value(value.span)
        });
        self.push(Entry {
            expression,
            bounds: value,
            constant: true,
            total: value.is_some(),
            free: None,
        })
    }
    pub fn choices(&self) -> usize {
        self.choices.len()
    }
    pub fn new_choice(&mut self, arms: usize) -> usize {
        let choice = self.choices.len();
        self.choices.push(arms);
        choice
    }
    pub fn choice(&mut self, choice: usize, arms: Vec<Id>) -> Id {
        if arms.iter().all(|&arm| arm == arms[0]) {
            return arms[0];
        }
        let bounds = arms
            .iter()
            .filter_map(|&id| self.entries[id].bounds)
            .reduce(Summary::union);
        let free = arms.iter().fold(Some((choice, choice)), |free, &id| {
            join_span(free, self.entries[id].free)
        });
        let total = arms.iter().all(|&id| self.entries[id].total);
        self.push(Entry {
            expression: Expression::Choice(choice, arms),
            bounds,
            constant: false,
            total,
            free,
        })
    }
    pub fn union(&mut self, arms: Vec<Id>) -> Id {
        if arms.iter().all(|&arm| arm == arms[0]) {
            return arms[0];
        }
        let bounds = arms
            .iter()
            .filter_map(|&id| self.entries[id].bounds)
            .reduce(Summary::union);
        let constant = arms.iter().all(|&id| self.entries[id].constant);
        if constant {
            return self.constant(bounds.map(|value| Summary::one(value.span)));
        }
        let free = arms
            .iter()
            .fold(None, |free, &id| join_span(free, self.entries[id].free));
        let total = arms.iter().any(|&id| self.entries[id].total);
        self.push(Entry {
            expression: Expression::Union(arms),
            bounds,
            constant: false,
            total,
            free,
        })
    }
    pub fn add(&mut self, a: Id, b: Id) -> Id {
        if a == 0 {
            return b;
        }
        if b == 0 {
            return a;
        }
        let a_entry = &self.entries[a];
        let b_entry = &self.entries[b];
        let bounds = a_entry.bounds.zip(b_entry.bounds).map(|(a, b)| a.add(b));
        if a_entry.constant && b_entry.constant {
            return self.constant(bounds);
        }
        let total = a_entry.total && b_entry.total;
        let free = join_span(a_entry.free, b_entry.free);
        self.push(Entry {
            expression: Expression::Add(a.min(b), a.max(b)),
            bounds,
            constant: false,
            total,
            free,
        })
    }
    pub fn valid(&mut self, child: Id) -> Id {
        if self.entries[child].total {
            return 0;
        }
        let entry = &self.entries[child];
        let bounds = entry.bounds.map(|_| {
            Summary::one(Span {
                min: 0,
                max: Some(0),
            })
        });
        if entry.constant {
            return self.constant(bounds);
        }
        let free = entry.free;
        self.push(Entry {
            expression: Expression::Valid(child),
            bounds,
            constant: false,
            total: false,
            free,
        })
    }
    pub fn times(&mut self, child: Id, count: usize) -> Id {
        if count <= 1 {
            return self.scale(child, count);
        }
        // A scalar multiple must not hide independent summands behind an
        // opaque atom. Rebuild this Add DAG once, preserving shared aliases.
        let mut pending = vec![child];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Expression::Add(a, b) = self.entries[id].expression {
                pending.push(a);
                pending.push(b);
            }
        }
        let mut scaled = BTreeMap::new();
        for id in seen {
            let value = match self.entries[id].expression {
                Expression::Add(a, b) => self.add(scaled[&a], scaled[&b]),
                _ => self.scale(id, count),
            };
            scaled.insert(id, value);
        }
        scaled[&child]
    }
    fn scale(&mut self, child: Id, count: usize) -> Id {
        if count == 0 {
            return self.valid(child);
        }
        if count == 1 {
            return child;
        }
        let entry = &self.entries[child];
        let bounds = entry.bounds.map(|value| value.times(count));
        if entry.constant {
            return self.constant(bounds);
        }
        let total = entry.total;
        let free = entry.free;
        self.push(Entry {
            expression: Expression::Times(child, count),
            bounds,
            constant: false,
            total,
            free,
        })
    }
    pub fn optional(&mut self, child: Id, maximum: Option<usize>) -> Id {
        if maximum == Some(0) {
            return self.valid(child);
        }
        let entry = &self.entries[child];
        let bounds = entry.bounds.map(|value| value.optional(maximum));
        if entry.constant {
            return self.constant(bounds);
        }
        let total = entry.total;
        let free = entry.free;
        self.push(Entry {
            expression: Expression::Optional(child, maximum),
            bounds,
            constant: false,
            total,
            free,
        })
    }
    pub fn positive(&mut self, child: Id) -> Id {
        let entry = &self.entries[child];
        if entry.bounds.is_some_and(|value| value.span.min > 0) {
            return child;
        }
        let bounds = entry.bounds.and_then(|value| {
            (value.span.max != Some(0)).then(|| Summary {
                span: Span {
                    min: value.span.min.max(1),
                    max: value.span.max,
                },
                greatest_min: value.greatest_min.max(1),
                finite_max: value.finite_max,
            })
        });
        if entry.constant || bounds.is_none() {
            return self.constant(bounds);
        }
        let free = entry.free;
        self.push(Entry {
            expression: Expression::Positive(child),
            bounds,
            constant: false,
            total: false,
            free,
        })
    }
    pub fn quantify(&mut self, child: Id, choices: Range<usize>) -> Id {
        if choices.is_empty() {
            return child;
        }
        // Interval union distributes over sums with disjoint body choices.
        // Separate their components before binding, so incoming independent
        // captures do not become one artificial correlated dependency.
        let mut seen = BTreeSet::new();
        let mut pending = vec![child];
        let mut leaves = Vec::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            match self.entries[id].expression {
                Expression::Add(a, b) => {
                    pending.push(a);
                    pending.push(b);
                }
                _ => leaves.push(id),
            }
        }
        let assigned = vec![None; self.choices.len()];
        let mut parent = (0..leaves.len()).collect::<Vec<_>>();
        let mut owners = BTreeMap::new();
        let mut independent = None;
        let mut local_leaves = BTreeSet::new();
        for (position, &leaf) in leaves.iter().enumerate() {
            let local = dependencies(&self.entries, &assigned, leaf)
                .into_iter()
                .filter(|choice| choices.contains(choice))
                .collect::<Vec<_>>();
            if local.is_empty() {
                if let Some(previous) = independent {
                    parent[position] = component_root(&parent, previous);
                } else {
                    independent = Some(position);
                }
            }
            if !local.is_empty() {
                local_leaves.insert(leaf);
            }
            for choice in local {
                if let Some(&previous) = owners.get(&choice) {
                    let a = component_root(&parent, previous);
                    let b = component_root(&parent, position);
                    parent[b] = a;
                } else {
                    owners.insert(choice, position);
                }
            }
        }
        if owners.is_empty() {
            return child;
        }
        let mut groups: BTreeMap<usize, BTreeSet<Id>> = BTreeMap::new();
        for (position, leaf) in leaves.into_iter().enumerate() {
            groups
                .entry(component_root(&parent, position))
                .or_default()
                .insert(leaf);
        }
        if groups.len() == 1 {
            return self.bind(child, choices);
        }
        let mut result = 0;
        for keep in groups.into_values() {
            let mut projected = BTreeMap::new();
            // Arena children precede parents. Rebuild only Add nodes; references
            // and choices stay shared, and zero terms disappear immediately.
            for &id in &seen {
                let value = match self.entries[id].expression {
                    Expression::Add(a, b) => self.add(projected[&a], projected[&b]),
                    _ if keep.contains(&id) => id,
                    _ => 0,
                };
                projected.insert(id, value);
            }
            let part = if keep.is_disjoint(&local_leaves) {
                projected[&child]
            } else {
                self.bind(projected[&child], choices.clone())
            };
            result = self.add(result, part);
        }
        result
    }
    fn bind(&mut self, child: Id, choices: Range<usize>) -> Id {
        let entry = &self.entries[child];
        let free = remove_span(entry.free, &choices);
        if entry
            .free
            .is_none_or(|(first, last)| last < choices.start || first >= choices.end)
        {
            return child;
        }
        let total = entry.total;
        let bounds = entry.bounds;
        self.push(Entry {
            expression: Expression::Quantify(child, choices.start, choices.end),
            bounds,
            constant: false,
            total,
            free,
        })
    }
    pub fn check(&mut self, id: Id, byte: usize) -> Result<(), CompileError> {
        let safe = self.entries[id].bounds.is_none_or(|bounds| {
            bounds.greatest_min < CAP && bounds.finite_max.is_none_or(|max| max < CAP)
        });
        if !safe {
            let unassigned = vec![None; self.choices.len()];
            let free = dependencies(&self.entries, &unassigned, id);
            if !free.is_empty() {
                // A complete root cofactor can witness reachable overflow
                // without finding every dense alternative's exact extrema.
                // Assign only free choices: quantified body choices must still
                // undergo their full interval union, including unbounded arms.
                for last in [false, true] {
                    let mut evaluator = Evaluator::new(self, true);
                    for &choice in &free {
                        let arm = if last {
                            evaluator.arena.choices[choice] - 1
                        } else {
                            0
                        };
                        evaluator.set(choice, Some(arm));
                    }
                    if let Some(value) = evaluator.run(id) {
                        value.checked(byte)?;
                    }
                }
            }
            // Safe or absent witnesses prove nothing about other assignments.
            if let Some(value) = self.query(id, true) {
                value.checked(byte)?;
            }
        }
        Ok(())
    }
    pub fn is_fixed(&mut self, id: Id, byte: usize) -> Result<bool, CompileError> {
        match Evaluator::new(self, false).fixed(id) {
            Fixed::Variable => Ok(false),
            Fixed::Width(width) if width >= CAP => Err(CompileError::Unsupported {
                byte,
                feature: "pattern width exceeds addressable range",
            }),
            _ => Ok(true),
        }
    }
    fn query(&mut self, id: Id, precise: bool) -> Option<Summary> {
        Evaluator::new(self, precise).run(id)
    }
}

/// Scoped dependencies exclude quantified body choices, even if assigned outside.
fn dependencies(entries: &[Entry], assigned: &[Option<usize>], id: Id) -> BTreeSet<usize> {
    let mut result = BTreeSet::new();
    let mut work = vec![(id, 0usize)];
    let mut scopes: Vec<(Range<usize>, usize)> = vec![(0..0, 0)];
    let mut contexts = BTreeMap::new();
    let mut seen = BTreeSet::new();
    while let Some((id, scope)) = work.pop() {
        if !seen.insert((id, scope)) {
            continue;
        }
        let bound = |choice| {
            let mut current = scope;
            while current != 0 {
                if scopes[current].0.contains(&choice) {
                    return true;
                }
                current = scopes[current].1;
            }
            false
        };
        match &entries[id].expression {
            Expression::Value(_) => (),
            Expression::Add(a, b) => {
                work.push((*a, scope));
                work.push((*b, scope));
            }
            Expression::Choice(choice, arms) => {
                if bound(*choice) {
                    work.extend(arms.iter().map(|&id| (id, scope)));
                } else if let Some(arm) = assigned[*choice] {
                    work.push((arms[arm], scope));
                } else {
                    result.insert(*choice);
                    work.extend(arms.iter().map(|&id| (id, scope)));
                }
            }
            Expression::Union(arms) => work.extend(arms.iter().map(|&id| (id, scope))),
            Expression::Quantify(child, first, end) => {
                let key = (scope, *first, *end);
                let next = *contexts.entry(key).or_insert_with(|| {
                    scopes.push((*first..*end, scope));
                    scopes.len() - 1
                });
                work.push((*child, next));
            }
            Expression::Times(child, _)
            | Expression::Optional(child, _)
            | Expression::Positive(child)
            | Expression::Valid(child) => work.push((*child, scope)),
        }
    }
    result
}

#[derive(Clone)]
struct Query {
    id: Id,
    keep: Option<(usize, Arc<BTreeSet<Id>>)>,
}
impl Query {
    fn child(&self, id: Id) -> Self {
        Self {
            id,
            keep: self.keep.clone(),
        }
    }
}
enum SumPlan {
    Direct,
    Groups(Vec<Arc<BTreeSet<Id>>>),
    Choice(usize),
}
/// A variation witness is sufficient for admission. It is never stored as an
/// exact interval or used by the arithmetic-overflow interpreter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixed {
    Absent,
    Width(u128),
    Variable,
}
impl Fixed {
    fn of(value: Option<Summary>) -> Self {
        match value {
            None => Self::Absent,
            Some(value) if value.span.max == Some(value.span.min) => Self::Width(value.span.min),
            _ => Self::Variable,
        }
    }
    fn sum(self, other: Self) -> Self {
        match (self, other) {
            (Self::Absent, _) | (_, Self::Absent) => Self::Absent,
            (Self::Width(a), Self::Width(b)) => Self::Width(add(a, b)),
            _ => Self::Variable,
        }
    }
    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Absent, value) | (value, Self::Absent) => value,
            (Self::Width(a), Self::Width(b)) if a == b => Self::Width(a),
            _ => Self::Variable,
        }
    }
}
enum ProofWork {
    Eval(Query),
    Cache(Query),
    Add,
    Join {
        count: usize,
        union: bool,
    },
    Times(usize),
    Cofactor {
        query: Query,
        choice: usize,
        next: usize,
        previous: Option<usize>,
        value: Fixed,
    },
    Unbind(Vec<(usize, Option<usize>)>),
}

enum Work {
    Eval(Query),
    Cache(Query),
    Add,
    Join {
        count: usize,
        collapse: bool,
    },
    Times(usize),
    Optional(Option<usize>),
    Positive(Query, Id),
    Valid,
    Cofactor {
        query: Query,
        choice: usize,
        next: usize,
        previous: Option<usize>,
        value: Option<Summary>,
    },
    Unbind {
        previous: Vec<(usize, Option<usize>)>,
    },
}
struct Evaluator<'a> {
    arena: &'a mut Arena,
    precise: bool,
    assigned: Vec<Option<usize>>,
    active: BTreeSet<usize>,
    generation: usize,
    projection: usize,
    cache: Vec<Option<(usize, usize, Option<Summary>)>>,
    work: Vec<Work>,
    values: Vec<Option<Summary>>,
}
impl<'a> Evaluator<'a> {
    fn new(arena: &'a mut Arena, precise: bool) -> Self {
        Self {
            precise,
            assigned: vec![None; arena.choices.len()],
            active: BTreeSet::new(),
            generation: 0,
            projection: 0,
            cache: vec![None; arena.entries.len()],
            arena,
            work: Vec::new(),
            values: Vec::new(),
        }
    }
    fn global(&self, query: &Query) -> bool {
        query.keep.is_none()
            && self.arena.entries[query.id]
                .free
                .is_none_or(|(first, last)| self.active.range(first..=last).next().is_none())
    }
    fn set(&mut self, choice: usize, value: Option<usize>) {
        self.assigned[choice] = value;
        if value.is_some() {
            self.active.insert(choice);
        } else {
            self.active.remove(&choice);
        }
        self.generation += 1;
    }
    fn branch(&mut self, query: Query, choice: usize) {
        let previous = self.assigned[choice];
        self.set(choice, Some(0));
        self.work.push(Work::Cofactor {
            query: query.clone(),
            choice,
            next: 1,
            previous,
            value: None,
        });
        self.work.push(Work::Eval(query));
    }
    fn support(&self, id: Id) -> BTreeSet<usize> {
        dependencies(&self.arena.entries, &self.assigned, id)
    }
    /// Flatten the Add DAG into unique leaves, not occurrences. A projection
    /// evaluates the original DAG and therefore retains multiplicities without
    /// expanding shared capture expressions or overflowing an artificial count.
    fn plan_sum(&self, query: &Query, a: Id, b: Id) -> SumPlan {
        if self.arena.entries[a].free.is_none() || self.arena.entries[b].free.is_none() {
            return SumPlan::Direct;
        }
        let mut pending = vec![query.id];
        let mut seen = BTreeSet::new();
        let mut leaves = Vec::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            match self.arena.entries[id].expression {
                Expression::Add(a, b) => {
                    pending.push(a);
                    pending.push(b);
                }
                _ if query
                    .keep
                    .as_ref()
                    .is_none_or(|(_, keep)| keep.contains(&id)) =>
                {
                    leaves.push(id)
                }
                _ => (),
            }
        }
        let mut parent = (0..leaves.len()).collect::<Vec<_>>();
        let mut owners = BTreeMap::new();
        let mut split = None;
        let mut constant = None;
        for (position, &leaf) in leaves.iter().enumerate() {
            let support = self.support(leaf);
            if support.is_empty() {
                if let Some(previous) = constant {
                    let previous = component_root(&parent, previous);
                    parent[position] = previous;
                } else {
                    constant = Some(position);
                }
            }
            for choice in support {
                if let Some(&previous) = owners.get(&choice) {
                    let left = component_root(&parent, previous);
                    let right = component_root(&parent, position);
                    parent[right] = left;
                    split = Some(split.map_or(choice, |known: usize| known.max(choice)));
                } else {
                    owners.insert(choice, position);
                }
            }
        }
        let Some(choice) = split else {
            return SumPlan::Direct;
        };
        let mut groups: BTreeMap<usize, BTreeSet<Id>> = BTreeMap::new();
        for (position, leaf) in leaves.into_iter().enumerate() {
            groups
                .entry(component_root(&parent, position))
                .or_default()
                .insert(leaf);
        }
        if groups.len() == 1 {
            SumPlan::Choice(choice)
        } else {
            SumPlan::Groups(groups.into_values().map(Arc::new).collect())
        }
    }
    fn sum(&mut self, query: Query, a: Id, b: Id) {
        match self.plan_sum(&query, a, b) {
            SumPlan::Direct => self.direct_add(query, a, b),
            SumPlan::Choice(choice) => self.branch(query, choice),
            SumPlan::Groups(groups) => {
                self.work.push(Work::Join {
                    count: groups.len(),
                    collapse: false,
                });
                for keep in groups.into_iter().rev() {
                    self.projection += 1;
                    self.work.push(Work::Eval(Query {
                        id: query.id,
                        keep: Some((self.projection, keep)),
                    }));
                }
            }
        }
    }
    fn direct_add(&mut self, query: Query, a: Id, b: Id) {
        self.work.push(Work::Add);
        self.work.push(Work::Eval(query.child(b)));
        self.work.push(Work::Eval(query.child(a)));
    }
    fn run(self, id: Id) -> Option<Summary> {
        self.run_query(Query { id, keep: None })
    }
    fn run_query(mut self, query: Query) -> Option<Summary> {
        self.work.push(Work::Eval(query));
        while let Some(work) = self.work.pop() {
            match work {
                Work::Eval(query) => {
                    let entry = &self.arena.entries[query.id];
                    if query.keep.as_ref().is_some_and(|(_, keep)| {
                        !matches!(entry.expression, Expression::Add(..))
                            && !keep.contains(&query.id)
                    }) {
                        self.values.push(Some(Summary::one(Span {
                            min: 0,
                            max: Some(0),
                        })));
                        continue;
                    }
                    if entry.constant {
                        self.values.push(entry.bounds);
                        continue;
                    }
                    if self.global(&query) {
                        if let Some(value) = self.arena.global[query.id][usize::from(self.precise)]
                        {
                            self.values.push(value);
                            continue;
                        }
                    }
                    let projection = query.keep.as_ref().map_or(0, |(id, _)| *id);
                    if let Some((generation, known_projection, value)) = self.cache[query.id] {
                        if generation == self.generation && known_projection == projection {
                            self.values.push(value);
                            continue;
                        }
                    }
                    self.work.push(Work::Cache(query.clone()));
                    match &entry.expression {
                        Expression::Value(span) => self.values.push(Some(Summary::one(*span))),
                        Expression::Add(a, b) => self.sum(query, *a, *b),
                        Expression::Choice(choice, arms) => {
                            if let Some(arm) = self.assigned[*choice] {
                                self.work.push(Work::Eval(Query {
                                    id: arms[arm],
                                    keep: None,
                                }));
                            } else {
                                self.branch(query, *choice);
                            }
                        }
                        Expression::Union(arms) => {
                            if let Some(choice) = self
                                .support(query.id)
                                .first()
                                .copied()
                                .filter(|_| self.precise)
                            {
                                self.branch(query, choice);
                            } else {
                                self.work.push(Work::Join {
                                    count: arms.len(),
                                    collapse: true,
                                });
                                for &id in arms.iter().rev() {
                                    self.work.push(Work::Eval(Query { id, keep: None }));
                                }
                            }
                        }
                        Expression::Times(child, count) => {
                            self.work.push(Work::Times(*count));
                            self.work.push(Work::Eval(Query {
                                id: *child,
                                keep: None,
                            }));
                        }
                        Expression::Optional(child, maximum) => {
                            self.work.push(Work::Optional(*maximum));
                            self.work.push(Work::Eval(Query {
                                id: *child,
                                keep: None,
                            }));
                        }
                        Expression::Positive(child) => {
                            self.work.push(Work::Positive(query, *child));
                            self.work.push(Work::Eval(Query {
                                id: *child,
                                keep: None,
                            }));
                        }
                        Expression::Valid(child) => {
                            self.work.push(Work::Valid);
                            self.work.push(Work::Eval(Query {
                                id: *child,
                                keep: None,
                            }));
                        }
                        Expression::Quantify(child, first, end) => {
                            if let Some(choice) = self
                                .support(query.id)
                                .first()
                                .copied()
                                .filter(|_| self.precise)
                            {
                                self.branch(query, choice);
                            } else {
                                let child = *child;
                                let choices = *first..*end;
                                let mut previous = Vec::new();
                                for choice in choices {
                                    if let Some(arm) = self.assigned[choice] {
                                        previous.push((choice, Some(arm)));
                                        self.set(choice, None);
                                    }
                                }
                                self.work.push(Work::Unbind { previous });
                                self.work.push(Work::Eval(Query {
                                    id: child,
                                    keep: None,
                                }));
                            }
                        }
                    }
                }
                Work::Cache(query) => {
                    let value = *self.values.last().expect("expression value");
                    let projection = query.keep.as_ref().map_or(0, |(id, _)| *id);
                    self.cache[query.id] = Some((self.generation, projection, value));
                    if self.global(&query) {
                        self.arena.global[query.id][usize::from(self.precise)] = Some(value);
                    }
                }
                Work::Add => {
                    let b = self.values.pop().unwrap();
                    let a = self.values.pop().unwrap();
                    self.values.push(a.zip(b).map(|(a, b)| a.add(b)));
                }
                Work::Join { count, collapse } => {
                    let offset = self.values.len() - count;
                    let values = self.values.drain(offset..);
                    let value = if collapse {
                        values
                            .flatten()
                            .reduce(Summary::union)
                            .map(|value| Summary::one(value.span))
                    } else {
                        values.fold(
                            Some(Summary::one(Span {
                                min: 0,
                                max: Some(0),
                            })),
                            |a, b| a.zip(b).map(|(a, b)| a.add(b)),
                        )
                    };
                    self.values.push(value);
                }
                Work::Times(count) => {
                    let value = self.values.pop().unwrap();
                    self.values.push(value.map(|value| value.times(count)));
                }
                Work::Optional(maximum) => {
                    let value = self.values.pop().unwrap();
                    self.values.push(value.map(|value| value.optional(maximum)));
                }
                Work::Positive(query, child) => {
                    let value = self.values.pop().unwrap();
                    match value {
                        Some(value) if value.span.min > 0 => self.values.push(Some(value)),
                        Some(value) if value.span.max != Some(0) => {
                            if let Some(choice) = self.support(child).first().copied() {
                                self.branch(query, choice);
                            } else {
                                self.values.push(Some(Summary::one(Span {
                                    min: value.span.min.max(1),
                                    max: value.span.max,
                                })));
                            }
                        }
                        _ => self.values.push(None),
                    }
                }
                Work::Valid => {
                    let value = self.values.pop().unwrap();
                    self.values.push(value.map(|_| {
                        Summary::one(Span {
                            min: 0,
                            max: Some(0),
                        })
                    }));
                }
                Work::Cofactor {
                    query,
                    choice,
                    next,
                    previous,
                    value,
                } => {
                    let branch = self.values.pop().unwrap();
                    let value = match (value, branch) {
                        (Some(a), Some(b)) => Some(a.union(b)),
                        (a, b) => a.or(b),
                    };
                    if next == self.arena.choices[choice] {
                        self.set(choice, previous);
                        self.values.push(value);
                    } else {
                        self.set(choice, Some(next));
                        self.work.push(Work::Cofactor {
                            query: query.clone(),
                            choice,
                            next: next + 1,
                            previous,
                            value,
                        });
                        self.work.push(Work::Eval(query));
                    }
                }
                Work::Unbind { previous } => {
                    let value = self
                        .values
                        .pop()
                        .unwrap()
                        .map(|value| Summary::one(value.span));
                    for (choice, arm) in previous {
                        self.set(choice, arm);
                    }
                    self.values.push(value);
                }
            }
        }
        self.values.pop().expect("one query result")
    }
    fn proof_branch(&mut self, work: &mut Vec<ProofWork>, query: Query, choice: usize) {
        let previous = self.assigned[choice];
        self.set(choice, Some(0));
        work.push(ProofWork::Cofactor {
            query: query.clone(),
            choice,
            next: 1,
            previous,
            value: Fixed::Absent,
        });
        work.push(ProofWork::Eval(query));
    }
    fn current_range(&mut self, query: Query) -> Option<Summary> {
        let assigned = self.assigned.clone();
        let active = self.active.clone();
        let mut evaluator = Evaluator::new(self.arena, false);
        evaluator.assigned = assigned;
        evaluator.active = active;
        evaluator.run_query(query)
    }
    fn fixed(mut self, id: Id) -> Fixed {
        let mut work = vec![ProofWork::Eval(Query { id, keep: None })];
        let mut values: Vec<Fixed> = Vec::new();
        let mut cache = vec![None; self.arena.entries.len()];
        while let Some(next) = work.pop() {
            match next {
                ProofWork::Eval(query) => {
                    let entry = &self.arena.entries[query.id];
                    if query.keep.as_ref().is_some_and(|(_, keep)| {
                        !matches!(entry.expression, Expression::Add(..))
                            && !keep.contains(&query.id)
                    }) {
                        values.push(Fixed::Width(0));
                        continue;
                    }
                    if entry.constant {
                        values.push(Fixed::of(entry.bounds));
                        continue;
                    }
                    if self.global(&query) {
                        if let Some(value) = self.arena.fixed[query.id] {
                            values.push(value);
                            continue;
                        }
                        if let Some(value) = self.arena.global[query.id][0] {
                            values.push(Fixed::of(value));
                            continue;
                        }
                    }
                    let projection = query.keep.as_ref().map_or(0, |(id, _)| *id);
                    if let Some((generation, known_projection, value)) = cache[query.id] {
                        if generation == self.generation && known_projection == projection {
                            values.push(value);
                            continue;
                        }
                    }
                    work.push(ProofWork::Cache(query.clone()));
                    match &entry.expression {
                        Expression::Value(span) => {
                            values.push(Fixed::of(Some(Summary::one(*span))))
                        }
                        Expression::Add(a, b) => match self.plan_sum(&query, *a, *b) {
                            SumPlan::Direct => {
                                work.push(ProofWork::Add);
                                work.push(ProofWork::Eval(query.child(*b)));
                                work.push(ProofWork::Eval(query.child(*a)));
                            }
                            SumPlan::Choice(choice) => self.proof_branch(&mut work, query, choice),
                            SumPlan::Groups(groups) => {
                                work.push(ProofWork::Join {
                                    count: groups.len(),
                                    union: false,
                                });
                                for keep in groups.into_iter().rev() {
                                    self.projection += 1;
                                    work.push(ProofWork::Eval(Query {
                                        id: query.id,
                                        keep: Some((self.projection, keep)),
                                    }));
                                }
                            }
                        },
                        Expression::Choice(choice, arms) => {
                            if let Some(arm) = self.assigned[*choice] {
                                work.push(ProofWork::Eval(Query {
                                    id: arms[arm],
                                    keep: None,
                                }));
                            } else {
                                self.proof_branch(&mut work, query, *choice);
                            }
                        }
                        Expression::Union(arms) => {
                            work.push(ProofWork::Join {
                                count: arms.len(),
                                union: true,
                            });
                            for &id in arms.iter().rev() {
                                work.push(ProofWork::Eval(Query { id, keep: None }));
                            }
                        }
                        Expression::Times(child, count) => {
                            work.push(ProofWork::Times(*count));
                            work.push(ProofWork::Eval(Query {
                                id: *child,
                                keep: None,
                            }));
                        }
                        Expression::Quantify(child, first, end) => {
                            let child = *child;
                            let choices = *first..*end;
                            let mut previous = Vec::new();
                            for choice in choices {
                                if let Some(arm) = self.assigned[choice] {
                                    previous.push((choice, Some(arm)));
                                    self.set(choice, None);
                                }
                            }
                            work.push(ProofWork::Unbind(previous));
                            work.push(ProofWork::Eval(Query {
                                id: child,
                                keep: None,
                            }));
                        }
                        // These operators can discard variation (e.g. a zero
                        // optional count or an interval's positive restriction).
                        // Their exact interval query remains separate from proof.
                        Expression::Positive(_)
                        | Expression::Valid(_)
                        | Expression::Optional(..) => {
                            let value = self.current_range(query);
                            values.push(Fixed::of(value));
                        }
                    }
                }
                ProofWork::Cache(query) => {
                    let value = *values.last().expect("fixedness proof");
                    let projection = query.keep.as_ref().map_or(0, |(id, _)| *id);
                    cache[query.id] = Some((self.generation, projection, value));
                    if self.global(&query) {
                        self.arena.fixed[query.id] = Some(value);
                    }
                }
                ProofWork::Add => {
                    let b = values.pop().unwrap();
                    let a = values.pop().unwrap();
                    values.push(a.sum(b));
                }
                ProofWork::Join { count, union } => {
                    let offset = values.len() - count;
                    let value = values.drain(offset..).fold(
                        if union {
                            Fixed::Absent
                        } else {
                            Fixed::Width(0)
                        },
                        |a, b| if union { a.union(b) } else { a.sum(b) },
                    );
                    values.push(value);
                }
                ProofWork::Times(count) => {
                    let value = values.pop().unwrap();
                    values.push(match value {
                        Fixed::Width(value) => Fixed::Width(times(value, count)),
                        value => value,
                    });
                }
                ProofWork::Cofactor {
                    query,
                    choice,
                    next,
                    previous,
                    value,
                } => {
                    let value = value.union(values.pop().unwrap());
                    if value == Fixed::Variable || next == self.arena.choices[choice] {
                        self.set(choice, previous);
                        values.push(value);
                    } else {
                        self.set(choice, Some(next));
                        work.push(ProofWork::Cofactor {
                            query: query.clone(),
                            choice,
                            next: next + 1,
                            previous,
                            value,
                        });
                        work.push(ProofWork::Eval(query));
                    }
                }
                ProofWork::Unbind(previous) => {
                    for (choice, arm) in previous {
                        self.set(choice, arm);
                    }
                }
            }
        }
        values.pop().expect("one fixedness proof")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(arena: &mut Arena, width: usize) -> Id {
        arena.value(Extent {
            min: width,
            max: Some(width),
        })
    }

    #[test]
    fn correlated_extrema_do_not_invent_addressability_overflow() {
        let mut arena = Arena::new();
        let one = literal(&mut arena, 1);
        let remaining = literal(&mut arena, usize::MAX - 1);
        let choice = arena.new_choice(2);
        let first = arena.choice(choice, vec![one, remaining]);
        let second = arena.choice(choice, vec![remaining, one]);
        let sum = arena.add(first, second);
        arena.check(sum, 17).unwrap();
        assert!(arena.is_fixed(sum, 17).unwrap());
        let span = arena.query(sum, false).unwrap().span;
        assert_eq!(
            (span.min, span.max),
            (usize::MAX as u128, Some(usize::MAX as u128))
        );
    }

    #[test]
    fn virtual_components_do_not_overflow_before_an_unbounded_sum() {
        let mut arena = Arena::new();
        let wide = arena.value(Extent {
            min: 0,
            max: Some(usize::MAX),
        });
        let narrow = arena.value(Extent {
            min: 0,
            max: Some(1),
        });
        let unbounded = arena.value(Extent { min: 0, max: None });
        let choice = arena.new_choice(2);
        let first = arena.choice(choice, vec![wide, 0]);
        let second = arena.choice(choice, vec![narrow, 0]);
        let prefix = arena.add(first, unbounded);
        let sum = arena.add(prefix, second);
        let observed = arena.query(sum, true).unwrap().checked(17).unwrap();
        assert_eq!(observed, Extent { min: 0, max: None });
        assert!(!arena.is_fixed(sum, 17).unwrap());
    }

    #[test]
    fn unbounded_alternatives_do_not_hide_other_paths_overflow() {
        let mut arena = Arena::new();
        let unbounded = arena.value(Extent { min: 1, max: None });
        let maximum = literal(&mut arena, usize::MAX);
        let choice = arena.new_choice(2);
        let selected = arena.choice(choice, vec![unbounded, maximum]);
        let scaled = arena.times(selected, 2);
        assert_eq!(
            arena.check(scaled, 17),
            Err(CompileError::Unsupported {
                byte: 17,
                feature: "pattern width exceeds addressable range",
            })
        );
    }

    #[test]
    fn safe_first_and_last_cofactors_still_check_the_middle_arm() {
        let mut arena = Arena::new();
        let maximum = literal(&mut arena, usize::MAX);
        let one = literal(&mut arena, 1);
        let choice = arena.new_choice(3);
        let selected = arena.choice(choice, vec![0, maximum, 0]);
        let sum = arena.add(selected, one);
        assert_eq!(
            arena.check(sum, 17),
            Err(CompileError::Unsupported {
                byte: 17,
                feature: "pattern width exceeds addressable range",
            })
        );
    }

    #[test]
    fn absent_root_cofactors_do_not_report_child_overflow() {
        let mut arena = Arena::new();
        let maximum = literal(&mut arena, usize::MAX);
        let one = literal(&mut arena, 1);
        let bottom = arena.positive(0);
        let choice = arena.new_choice(2);
        let selected = arena.choice(choice, vec![maximum, 0]);
        let guard = arena.choice(choice, vec![bottom, 0]);
        let sum = arena.add(selected, one);
        let root = arena.add(sum, guard);
        arena.check(root, 17).unwrap();
        assert_eq!(arena.query(root, true).unwrap().span.min, 1);
    }

    #[test]
    fn fixedness_preserves_absent_factors_and_positive_intervals() {
        let mut arena = Arena::new();
        let varying = arena.value(Extent {
            min: 0,
            max: Some(1),
        });
        let positive = arena.positive(varying);
        assert!(arena.is_fixed(positive, 0).unwrap());
        let bottom = arena.positive(0);
        let absent = arena.add(varying, bottom);
        assert!(arena.is_fixed(absent, 0).unwrap());
        assert!(arena.query(absent, false).is_none());
    }
}
