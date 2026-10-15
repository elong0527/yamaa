//! Structural width admission with capture state and correlated alternative paths.
//! This does not execute subjects or use literal/assertion satisfiability to
//! exempt variable syntax. Intervals describe consumption, not captured text.
use super::*;
use num_bigint::BigUint;
use std::collections::{BTreeMap, BTreeSet};

/// Use the existing scalar arithmetic when no capture-dependent pass is needed.
pub(super) fn validate(nodes: &[Node], root: usize) -> Result<(), CompileError> {
    let has_behind = nodes
        .iter()
        .any(|node| matches!(node.kind, Kind::Look { behind: true, .. }));
    if !has_behind {
        return Ok(());
    }
    let has_reference = nodes
        .iter()
        .any(|node| matches!(node.kind, Kind::Backreference(_)));
    let mut dependent = false;
    for node in nodes {
        if let Kind::Look {
            child,
            behind: true,
            ..
        } = node.kind
        {
            match nodes[child].width {
                Width::Fixed(_) => {}
                Width::Variable if !has_reference => return Err(variable(node.byte)),
                _ => dependent = true,
            }
        }
    }
    // Nested assertions have their own arena entries. An unrelated reference
    // cannot add analysis cost when every lookbehind is already statically fixed.
    if !dependent {
        return Ok(());
    }
    let mut analysis = Analysis::new(nodes, root);

    let initial = Path {
        width: analysis.expressions.zero,
        captures: 0,
        domain: analysis.expressions.zero,
    };
    analysis.visit(root, initial, false)?;
    analysis.expressions.check_arithmetic()?;
    // Retain deterministic arena/source ownership rather than returning whichever
    // reverse traversal happens to discover a variable assertion first.
    for (id, node) in nodes.iter().enumerate() {
        if matches!(node.kind, Kind::Look { behind: true, .. }) {
            let expression = analysis.behind[id].expect("every assertion is structurally visited");
            if !analysis.expressions.is_fixed(expression, node.byte)? {
                return Err(variable(node.byte));
            }
        }
    }
    Ok(())
}

/// Distinguish a grammar rejection from analysis/resource refusal.
fn variable(byte: usize) -> CompileError {
    CompileError::Invalid {
        byte,
        reason: "variable-length lookbehind",
    }
}

/// Unknown upper bounds represent varying repetition, never an arithmetic wrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Extent {
    min: usize,
    max: Option<usize>,
}
impl Extent {
    const ZERO: Self = Self {
        min: 0,
        max: Some(0),
    };
    const ONE: Self = Self {
        min: 1,
        max: Some(1),
    };

    /// Union intervals without forgetting correlations stored in separate paths.
    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.zip(other.max).map(|(a, b)| a.max(b)),
        }
    }
    /// Optional repeat iterations must consume, unlike required iterations.
    fn positive(self) -> Option<Self> {
        (self.max != Some(0)).then_some(Self {
            min: self.min.max(1),
            max: self.max,
        })
    }
    /// Preserve width arithmetic without wrapping addressable positions.
    fn checked(
        min: Option<usize>,
        max: Option<Option<usize>>,
        byte: usize,
    ) -> Result<Self, CompileError> {
        let error = || CompileError::Unsupported {
            byte,
            feature: "pattern width exceeds addressable range",
        };
        let value = Self {
            min: min.ok_or_else(error)?,
            max: max.ok_or_else(error)?,
        };
        Ok(value)
    }
    /// Zero or more strictly consuming optional iterations, bounded or unbounded.
    fn optional(self, maximum: Option<usize>, byte: usize) -> Result<Self, CompileError> {
        if maximum == Some(0) {
            return Ok(Self::ZERO);
        }
        let max = match (self.max, maximum) {
            (Some(width), Some(count)) => width.checked_mul(count).map(Some),
            _ => Some(None),
        };
        Self::checked(Some(0), max, byte)
    }
}

/// Immutable roots share all preceding choices instead of enumerating their product.
#[derive(Clone, Copy)]
struct Path {
    width: usize,
    captures: usize,
    // Zero on a structural path; absent on an excluded empty optional iteration.
    domain: usize,
}
#[derive(Clone)]
enum Environment {
    Empty,
    Leaf(usize),
    Branch(usize, usize),
    Choice {
        choice: usize,
        branches: Vec<(usize, usize)>,
    },
}
struct Analysis<'a> {
    nodes: &'a [Node],
    behind: Vec<Option<usize>>,
    referenced: BTreeSet<usize>,
    expressions: Expressions,
    environments: Vec<Environment>,
    groups: usize,
    splits: BTreeMap<usize, (usize, usize)>,
    lookup: BTreeMap<(usize, usize), usize>,
}
impl<'a> Analysis<'a> {
    fn new(nodes: &'a [Node], root: usize) -> Self {
        Self {
            nodes,
            behind: vec![None; nodes.len()],
            referenced: nodes
                .iter()
                .filter_map(|node| match node.kind {
                    Kind::Backreference(Reference::Number(number)) => Some(number),
                    _ => None,
                })
                .collect(),
            expressions: Expressions::new(),
            environments: vec![Environment::Empty],
            groups: nodes[root].captures.end,
            splits: BTreeMap::new(),
            lookup: BTreeMap::new(),
        }
    }
    fn environment(&mut self, environment: Environment) -> usize {
        match &environment {
            Environment::Leaf(width) if *width == self.expressions.zero => return 0,
            Environment::Branch(0, 0) => return 0,
            Environment::Choice { branches, .. }
                if branches.iter().all(|branch| *branch == branches[0]) =>
            {
                return branches[0].0
            }
            _ => {}
        }
        let id = self.environments.len();
        self.environments.push(environment);
        id
    }
    /// Persistent indexing avoids a history scan for every referenced capture.
    /// A choice splits lazily only along the indexed paths actually touched.
    fn split(&mut self, root: usize) -> (usize, usize) {
        let mut pending = vec![(root, false)];
        while let Some((id, ready)) = pending.pop() {
            if self.splits.contains_key(&id) {
                continue;
            }
            let children = match self.environments[id].clone() {
                Environment::Empty => Some((0, 0)),
                Environment::Branch(left, right) => Some((left, right)),
                Environment::Leaf(_) => unreachable!("only non-leaf capture intervals split"),
                Environment::Choice { choice, branches } => {
                    if ready
                        || branches
                            .iter()
                            .all(|(child, _)| self.splits.contains_key(child))
                    {
                        let left = branches
                            .iter()
                            .map(|&(child, domain)| (self.splits[&child].0, domain))
                            .collect();
                        let right = branches
                            .iter()
                            .map(|&(child, domain)| (self.splits[&child].1, domain))
                            .collect();
                        Some((
                            self.environment(Environment::Choice {
                                choice,
                                branches: left,
                            }),
                            self.environment(Environment::Choice {
                                choice,
                                branches: right,
                            }),
                        ))
                    } else {
                        pending.push((id, true));
                        pending.extend(branches.iter().map(|&(child, _)| (child, false)));
                        None
                    }
                }
            };
            if let Some(children) = children {
                self.splits.insert(id, children);
            }
        }
        self.splits[&root]
    }
    fn set_capture(&mut self, mut root: usize, number: usize, width: usize) -> usize {
        let (mut start, mut end) = (0, self.groups);
        let mut path = Vec::new();
        while end - start > 1 {
            let (left, right) = self.split(root);
            let middle = start + (end - start) / 2;
            if number < middle {
                path.push((false, right));
                root = left;
                end = middle;
            } else {
                path.push((true, left));
                root = right;
                start = middle;
            }
        }
        root = self.environment(Environment::Leaf(width));
        while let Some((right, sibling)) = path.pop() {
            root = if right {
                self.environment(Environment::Branch(sibling, root))
            } else {
                self.environment(Environment::Branch(root, sibling))
            };
        }
        root
    }
    fn clear_captures(&mut self, root: usize, range: std::ops::Range<usize>) -> usize {
        let mut pending = vec![(root, 0, self.groups, false)];
        let mut cleared = BTreeMap::new();
        while let Some((id, start, end, ready)) = pending.pop() {
            let key = (id, start, end);
            if cleared.contains_key(&key) {
                continue;
            }
            if id == 0 || range.start >= end || range.end <= start {
                cleared.insert(key, id);
                continue;
            }
            if range.start <= start && end <= range.end {
                cleared.insert(key, 0);
                continue;
            }
            let (left, right) = self.split(id);
            let middle = start + (end - start) / 2;
            if !ready {
                pending.push((id, start, end, true));
                pending.push((left, start, middle, false));
                pending.push((right, middle, end, false));
                continue;
            }
            let children = (
                cleared[&(left, start, middle)],
                cleared[&(right, middle, end)],
            );
            let next = if children == (left, right) {
                id
            } else {
                self.environment(Environment::Branch(children.0, children.1))
            };
            cleared.insert(key, next);
        }
        cleared[&(root, 0, self.groups)]
    }
    /// Indexed lookup and lazy choice conditioning use explicit work frames.
    fn capture(&mut self, environment: usize, number: usize) -> usize {
        let mut pending = vec![(environment, 0, self.groups, false)];
        while let Some((id, start, end, ready)) = pending.pop() {
            if self.lookup.contains_key(&(id, number)) {
                continue;
            }
            let result = match self.environments[id].clone() {
                Environment::Empty => Some(self.expressions.zero),
                Environment::Leaf(width) => Some(width),
                Environment::Branch(left, right) => {
                    let middle = start + (end - start) / 2;
                    let (child, child_start, child_end) = if number < middle {
                        (left, start, middle)
                    } else {
                        (right, middle, end)
                    };
                    if let Some(&known) = self.lookup.get(&(child, number)) {
                        Some(known)
                    } else {
                        pending.push((id, start, end, true));
                        pending.push((child, child_start, child_end, false));
                        None
                    }
                }
                Environment::Choice { choice, branches } => {
                    if ready
                        || branches
                            .iter()
                            .all(|(parent, _)| self.lookup.contains_key(&(*parent, number)))
                    {
                        let values = branches
                            .iter()
                            .map(|&(parent, domain)| {
                                self.expressions
                                    .add(domain, self.lookup[&(parent, number)], 0)
                            })
                            .collect();
                        Some(self.expressions.choice(choice, values))
                    } else {
                        pending.push((id, start, end, true));
                        pending.extend(
                            branches
                                .iter()
                                .map(|&(parent, _)| (parent, start, end, false)),
                        );
                        None
                    }
                }
            };
            if let Some(result) = result {
                self.lookup.insert((id, number), result);
            }
        }
        self.lookup[&(environment, number)]
    }
    fn merge(&mut self, paths: Vec<Path>) -> Path {
        if paths.len() == 1 {
            return paths[0];
        }
        let choice = self.expressions.arities.len();
        self.expressions.arities.push(paths.len());
        let width = self
            .expressions
            .choice(choice, paths.iter().map(|p| p.width).collect());
        let domain = self
            .expressions
            .choice(choice, paths.iter().map(|p| p.domain).collect());
        if paths
            .iter()
            .all(|path| path.captures == paths[0].captures && path.domain == paths[0].domain)
        {
            // Equal capture environments have no observable branch correlation
            // to retain. Match the structural interval union before a later
            // enclosing capture records this consumed width.
            let width = self.expressions.intern(Expression::Project(width, choice));
            return Path {
                width,
                captures: paths[0].captures,
                domain: paths[0].domain,
            };
        }
        let captures = self.environment(Environment::Choice {
            choice,
            branches: paths.iter().map(|p| (p.captures, p.domain)).collect(),
        });
        Path {
            width,
            captures,
            domain,
        }
    }
    /// Grammar nesting alone recurses. Flat sequences and all query graphs are iterative.
    fn visit(&mut self, id: usize, mut path: Path, backwards: bool) -> Result<Path, CompileError> {
        let node = &self.nodes[id];
        match &node.kind {
            Kind::Literal(_) | Kind::Dot | Kind::Class { .. } => {
                path.width = self
                    .expressions
                    .add(path.width, self.expressions.one, node.byte);
            }
            Kind::Start | Kind::End | Kind::Boundary(_) => {}
            Kind::Backreference(Reference::Number(number)) => {
                let width = self.capture(path.captures, *number);
                path.width = self.expressions.add(path.width, width, node.byte);
            }
            Kind::Backreference(Reference::Name(_)) => unreachable!("references already resolved"),
            Kind::Sequence(children) => {
                for position in 0..children.len() {
                    let child = children[if backwards {
                        children.len() - position - 1
                    } else {
                        position
                    }];
                    path = self.visit(child, path, backwards)?;
                }
            }
            Kind::Alternative(children) => {
                let mut branches = Vec::with_capacity(children.len());
                for &child in children {
                    branches.push(self.visit(child, path, backwards)?);
                }
                path = self.merge(branches);
            }
            Kind::Group { child, number } => {
                let prefix = path.width;
                path.width = path.domain;
                path = self.visit(*child, path, backwards)?;
                if let Some(number) = number.filter(|n| self.referenced.contains(n)) {
                    path.captures = self.set_capture(path.captures, number, path.width);
                }
                path.width = self.expressions.add(prefix, path.width, node.byte);
            }
            Kind::Look {
                child,
                behind,
                positive,
            } => {
                let original = path;
                path.width = path.domain;
                path = self.visit(*child, path, *behind)?;
                if *behind {
                    self.behind[id] = Some(path.width);
                }
                if *positive {
                    path.width = self.expressions.add(original.width, path.domain, node.byte);
                } else {
                    path = original;
                }
            }
            Kind::Repeat {
                child, min, max, ..
            } => {
                let original = path;
                let captures = self.nodes[*child].captures.clone();
                path.captures = self.clear_captures(path.captures, captures);
                path.width = path.domain;
                let first_choice = self.expressions.arities.len();
                let last = self.visit(*child, path, backwards)?;
                if *max == Some(0) {
                    return Ok(original);
                }
                // Earlier iterations choose independently of the last iteration,
                // but retain every choice made before entering this repeat.
                let any = self
                    .expressions
                    .intern(Expression::Project(last.width, first_choice));
                let mut branches = Vec::new();
                if *min == 0 {
                    branches.push(original);
                }
                if *min > 0 {
                    let earlier = self.expressions.times(any, min - 1, node.byte);
                    let width = self.expressions.add(earlier, last.width, node.byte);
                    branches.push(Path {
                        width: self.expressions.add(original.width, width, node.byte),
                        ..last
                    });
                }
                if max.is_none_or(|max| max > *min) {
                    let positive = self.expressions.positive(last.width);
                    let earlier_positive = self.expressions.positive(any);
                    let optional = self.expressions.intern(Expression::Optional(
                        earlier_positive,
                        max.map(|max| max - min - 1),
                        node.byte,
                    ));
                    let required = self.expressions.times(any, *min, node.byte);
                    let width = self.expressions.add(required, optional, node.byte);
                    let width = self.expressions.add(width, positive, node.byte);
                    branches.push(Path {
                        width: self.expressions.add(original.width, width, node.byte),
                        domain: self.expressions.present(positive),
                        ..last
                    });
                }
                path = self.merge(branches);
            }
        }
        Ok(path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Expression {
    Value(Option<Extent>),
    Add(usize, usize, usize),
    Times(usize, usize, usize),
    Choice(usize, Vec<usize>),
    Positive(usize),
    Optional(usize, Option<usize>, usize),
    Present(usize),
    // Marginalize body-local choices, conditionally on outside choices.
    Project(usize, usize),
}
impl Expression {
    fn children(&self) -> Vec<usize> {
        match self {
            Self::Value(_) => Vec::new(),
            Self::Add(a, b, _) => vec![*a, *b],
            Self::Times(a, ..)
            | Self::Positive(a)
            | Self::Optional(a, ..)
            | Self::Present(a)
            | Self::Project(a, _) => vec![*a],
            Self::Choice(_, branches) => branches.clone(),
        }
    }
}
/// Exact, ordered expression identities; no digest or generated content identity.
struct Expressions {
    nodes: Vec<Expression>,
    identities: BTreeMap<Expression, usize>,
    arities: Vec<usize>,
    restrictions: BTreeMap<(usize, usize, usize), usize>,
    supports: BTreeMap<usize, BTreeSet<usize>>,
    ranges: BTreeMap<Query, Result<Option<Outcomes>, CompileError>>,
    arithmetic: Vec<ArithmeticBounds>,
    checks: Vec<(usize, usize)>,
    fixed: BTreeMap<Query, Result<FixedProof, CompileError>>,
    zero: usize,
    one: usize,
    absent: usize,
}
impl Expressions {
    fn new() -> Self {
        let mut expressions = Self {
            nodes: Vec::new(),
            identities: BTreeMap::new(),
            arities: Vec::new(),
            restrictions: BTreeMap::new(),
            supports: BTreeMap::new(),
            ranges: BTreeMap::new(),
            arithmetic: Vec::new(),
            checks: Vec::new(),
            fixed: BTreeMap::new(),
            zero: 0,
            one: 0,
            absent: 0,
        };
        expressions.zero = expressions.intern(Expression::Value(Some(Extent::ZERO)));
        expressions.one = expressions.intern(Expression::Value(Some(Extent::ONE)));
        expressions.absent = expressions.intern(Expression::Value(None));
        expressions
    }
    fn intern(&mut self, expression: Expression) -> usize {
        if let Some(&id) = self.identities.get(&expression) {
            return id;
        }
        let bounds = ArithmeticBounds::of(&expression, &self.arithmetic);
        let id = self.nodes.len();
        if bounds.may_overflow() {
            if let Expression::Add(_, _, byte)
            | Expression::Times(_, _, byte)
            | Expression::Optional(_, _, byte) = &expression
            {
                self.checks.push((id, *byte));
            }
        }
        self.arithmetic.push(bounds);
        self.nodes.push(expression.clone());
        self.identities.insert(expression, id);
        id
    }
    fn check_arithmetic(&mut self) -> Result<(), CompileError> {
        // Ordinary finite prefixes are proved safe by constant-space bounds;
        // only potentially overflowing operations need a symbolic query.
        // When different contexts overflow at different operations, report the
        // first constructed operation, independently of concrete path expansion.
        let checks = self.checks.clone();
        for (expression, byte) in checks {
            let query = self.query(expression, byte);
            // A single reachable overflow proves refusal. Check a completely
            // conditioned query first so a huge independent summand need not
            // wait for exact optimization of the other connected summands.
            // A non-overflowing witness proves nothing about other choices;
            // they still require the complete arithmetic query below.
            let witness = self.arithmetic_witness(query);
            self.evaluate(witness)?;
            self.extent(expression, byte)?;
        }
        Ok(())
    }
    fn arithmetic_witness(&mut self, mut query: Query) -> Query {
        let choices: BTreeSet<_> = query
            .terms
            .iter()
            .flat_map(|(id, _)| self.support(*id))
            .collect();
        for choice in choices.into_iter().rev() {
            let mut preferred = None;
            for branch in 0..self.arities[choice] {
                let candidate = self.conditioned(&query, choice, branch);
                if candidate.absent {
                    continue;
                }
                // These observations choose a promising assignment only.
                // They never establish overflow or safety by themselves.
                let mut minimum = candidate.constant.min.clone();
                let mut finite = candidate.constant.max.clone();
                for (id, weight) in &candidate.terms {
                    let bounds = self.arithmetic[*id];
                    minimum += BigUint::from(bounds.min) * weight;
                    finite = finite
                        .zip(bounds.finite)
                        .map(|(sum, max)| sum + BigUint::from(max) * weight);
                }
                let priority = minimum.max(finite.unwrap_or_default());
                if preferred
                    .as_ref()
                    .is_none_or(|(known, _)| priority > *known)
                {
                    preferred = Some((priority, candidate));
                }
            }
            let Some((_, candidate)) = preferred else {
                return self.query(self.absent, query.byte);
            };
            query = candidate;
        }
        query
    }
    fn add(&mut self, a: usize, b: usize, byte: usize) -> usize {
        if a == self.absent || b == self.absent {
            return self.absent;
        }
        if a == self.zero {
            return b;
        }
        if b == self.zero {
            return a;
        }
        self.intern(Expression::Add(a.min(b), a.max(b), byte))
    }
    fn times(&mut self, expression: usize, count: usize, byte: usize) -> usize {
        if count == 1 || expression == self.zero || expression == self.absent {
            return expression;
        }
        if count == 0 {
            return self.present(expression);
        }
        self.intern(Expression::Times(expression, count, byte))
    }
    fn present(&mut self, expression: usize) -> usize {
        match self.nodes[expression] {
            Expression::Value(None) => self.absent,
            Expression::Value(Some(_)) => self.zero,
            Expression::Present(_) => expression,
            _ => self.intern(Expression::Present(expression)),
        }
    }
    fn positive(&mut self, expression: usize) -> usize {
        if let Expression::Value(value) = self.nodes[expression] {
            self.intern(Expression::Value(value.and_then(Extent::positive)))
        } else {
            self.intern(Expression::Positive(expression))
        }
    }
    fn choice(&mut self, choice: usize, branches: Vec<usize>) -> usize {
        if branches.iter().all(|branch| *branch == branches[0]) {
            return branches[0];
        }
        self.intern(Expression::Choice(choice, branches))
    }
    /// Collect supports only for queried atoms, never for every growing Add prefix.
    fn support(&mut self, id: usize) -> BTreeSet<usize> {
        if let Some(known) = self.supports.get(&id) {
            return known.clone();
        }
        let mut support = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut pending = vec![(id, usize::MAX)];
        while let Some((id, cutoff)) = pending.pop() {
            if !seen.insert((id, cutoff)) {
                continue;
            }
            match &self.nodes[id] {
                Expression::Choice(choice, branches) => {
                    if *choice < cutoff {
                        support.insert(*choice);
                    }
                    pending.extend(branches.iter().map(|&id| (id, cutoff)));
                }
                Expression::Project(child, first) => pending.push((*child, cutoff.min(*first))),
                expression => {
                    pending.extend(expression.children().into_iter().map(|id| (id, cutoff)))
                }
            }
        }
        self.supports.insert(id, support.clone());
        support
    }
    /// Condition a shared choice everywhere, except beneath a projection that
    /// deliberately gives earlier iterations independent copies of that choice.
    fn restrict(&mut self, root: usize, choice: usize, branch: usize) -> usize {
        let mut pending = vec![(root, false)];
        while let Some((id, ready)) = pending.pop() {
            if self.restrictions.contains_key(&(id, choice, branch)) {
                continue;
            }
            let expression = self.nodes[id].clone();
            let immediate = match &expression {
                Expression::Value(_) => Some(id),
                Expression::Choice(current, branches) if *current == choice => {
                    Some(branches[branch])
                }
                Expression::Project(_, first) if choice >= *first => Some(id),
                _ => None,
            };
            if let Some(result) = immediate {
                self.restrictions.insert((id, choice, branch), result);
                continue;
            }
            if !ready {
                pending.push((id, true));
                pending.extend(expression.children().into_iter().map(|id| (id, false)));
                continue;
            }
            let mapped = |child: usize| self.restrictions[&(child, choice, branch)];
            let result = match expression {
                Expression::Add(a, b, byte) => {
                    let (a, b) = (mapped(a), mapped(b));
                    self.add(a, b, byte)
                }
                Expression::Times(a, n, byte) => {
                    let a = mapped(a);
                    self.times(a, n, byte)
                }
                Expression::Choice(current, branches) => {
                    let branches = branches.into_iter().map(mapped).collect();
                    self.choice(current, branches)
                }
                Expression::Positive(a) => {
                    let a = mapped(a);
                    self.positive(a)
                }
                Expression::Optional(a, n, byte) => {
                    let a = mapped(a);
                    self.intern(Expression::Optional(a, n, byte))
                }
                Expression::Present(a) => {
                    let a = mapped(a);
                    self.present(a)
                }
                Expression::Project(a, first) => {
                    let a = mapped(a);
                    self.intern(Expression::Project(a, first))
                }
                Expression::Value(_) => unreachable!(),
            };
            self.restrictions.insert((id, choice, branch), result);
        }
        self.restrictions[&(root, choice, branch)]
    }
    fn query(&self, expression: usize, byte: usize) -> Query {
        self.normalize(Query {
            absent: false,
            constant: WideExtent::zero(),
            terms: vec![(expression, BigUint::from(1u8))],
            byte,
        })
    }
    /// Flatten sums on demand, combining shared terms without expanding their
    /// expression DAG or multiplying the number of capture environments.
    fn normalize(&self, mut query: Query) -> Query {
        let mut pending = BTreeMap::<usize, BigUint>::new();
        for (id, weight) in std::mem::take(&mut query.terms) {
            *pending.entry(id).or_default() += weight;
        }
        let mut terms = BTreeMap::<usize, BigUint>::new();
        // Children precede parents in the arena. Aggregate all incoming weights
        // before expanding a shared node, including doubling backreferences.
        while let Some((id, weight)) = pending.pop_last() {
            match &self.nodes[id] {
                Expression::Value(None) => {
                    query.absent = true;
                    break;
                }
                Expression::Value(Some(value)) => query.constant.add(value, &weight),
                Expression::Add(a, b, _) => {
                    *pending.entry(*a).or_default() += &weight;
                    *pending.entry(*b).or_default() += weight;
                }
                Expression::Times(child, count, _) => {
                    *pending.entry(*child).or_default() += weight * BigUint::from(*count)
                }
                _ => {
                    *terms.entry(id).or_default() += weight;
                }
            }
        }
        query.terms = if query.absent {
            Vec::new()
        } else {
            terms.into_iter().collect()
        };
        if query.absent {
            query.constant = WideExtent::zero();
        }
        query
    }
    fn conditioned(&mut self, query: &Query, choice: usize, branch: usize) -> Query {
        let terms = query
            .terms
            .iter()
            .map(|(id, weight)| (self.restrict(*id, choice, branch), weight.clone()))
            .collect();
        self.normalize(Query {
            terms,
            ..query.clone()
        })
    }
    fn plan(&mut self, query: &Query) -> Plan {
        if query.absent || query.terms.is_empty() {
            return Plan::Constant;
        }
        if query.constant != WideExtent::zero() {
            return Plan::Sum(vec![Query {
                constant: WideExtent::zero(),
                ..query.clone()
            }]);
        }
        let supports: Vec<_> = query
            .terms
            .iter()
            .map(|(id, _)| self.support(*id))
            .collect();
        // Connected support components share choices; separate components are
        // independent and their extrema can be summed without a Cartesian product.
        let mut parents: Vec<_> = (0..supports.len()).collect();
        fn representative(parents: &mut [usize], mut current: usize) -> usize {
            while parents[current] != current {
                parents[current] = parents[parents[current]];
                current = parents[current];
            }
            current
        }
        let mut owners = BTreeMap::new();
        for (position, support) in supports.iter().enumerate() {
            for &choice in support {
                if let Some(&owner) = owners.get(&choice) {
                    let a = representative(&mut parents, position);
                    let b = representative(&mut parents, owner);
                    parents[a] = b;
                } else {
                    owners.insert(choice, position);
                }
            }
        }
        let mut components = BTreeMap::<usize, Vec<usize>>::new();
        for position in 0..supports.len() {
            let root = representative(&mut parents, position);
            components.entry(root).or_default().push(position);
        }
        let groups: Vec<_> = components.into_values().collect();
        if groups.len() > 1 {
            return Plan::Sum(
                groups
                    .into_iter()
                    .map(|group| Query {
                        absent: false,
                        constant: WideExtent::zero(),
                        terms: group
                            .into_iter()
                            .map(|position| query.terms[position].clone())
                            .collect(),
                        byte: query.byte,
                    })
                    .collect(),
            );
        }
        if let Some(&choice) = owners.last_key_value().map(|(choice, _)| choice) {
            let children = (0..self.arities[choice])
                .map(|branch| self.conditioned(query, choice, branch))
                .collect();
            // Independent term extrema are a sound outer bound. Once one or
            // more correlated branches attain both bounds, no remaining branch
            // can enlarge the exact result.
            let bounds = if query.terms.len() > 1 {
                query
                    .terms
                    .iter()
                    .map(|term| Query {
                        absent: false,
                        constant: WideExtent::zero(),
                        terms: vec![term.clone()],
                        byte: query.byte,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            return Plan::Branches { children, bounds };
        }
        Plan::Closed(
            query
                .terms
                .iter()
                .map(|(id, _)| {
                    let child = self.nodes[*id].children();
                    child
                        .into_iter()
                        .map(|id| self.query(id, query.byte))
                        .collect()
                })
                .collect(),
        )
    }
    /// Explicit evaluation frames avoid recursive lookup/evaluation/destruction
    /// even for long, flat chains of shared environments and expressions.
    fn extent(&mut self, expression: usize, byte: usize) -> Result<Option<Extent>, CompileError> {
        let root = self.query(expression, byte);
        self.evaluate(root)
            .map(|value| value.map(|value| value.extent))
    }
    fn evaluate(&mut self, root: Query) -> Result<Option<Outcomes>, CompileError> {
        let mut pending = vec![(root.clone(), None)];
        while let Some((query, plan)) = pending.pop() {
            if self.ranges.contains_key(&query) {
                continue;
            }
            let Some(plan) = plan else {
                let plan = self.plan(&query);
                let dependencies = plan.dependencies();
                pending.push((query, Some(plan)));
                pending.extend(dependencies.into_iter().map(|query| (query, None)));
                continue;
            };
            if let Plan::Branches { children, bounds } = &plan {
                let bound = if bounds.is_empty() {
                    None
                } else {
                    Some(weighted_sum(
                        WideExtent::zero(),
                        bounds
                            .iter()
                            .map(|child| (self.ranges[child].clone(), BigUint::from(1u8))),
                        query.byte,
                    ))
                };
                if matches!(bound, Some(Ok(None))) {
                    self.ranges.insert(query, Ok(None));
                    continue;
                }
                let mut reached = None;
                let mut next = None;
                let mut error = None;
                for child in children {
                    match self.ranges.get(child) {
                        Some(Ok(Some(value))) => {
                            reached =
                                Some(reached.map_or(*value, |known: Outcomes| known.union(*value)))
                        }
                        Some(Ok(None)) => {}
                        Some(Err(found)) => {
                            error = Some(found.clone());
                            break;
                        }
                        None => {
                            next = Some(child.clone());
                            break;
                        }
                    }
                }
                if let Some(error) = error {
                    self.ranges.insert(query, Err(error));
                    continue;
                }
                if let Some(Ok(Some(bound))) = bound {
                    if reached == Some(bound) {
                        self.ranges.insert(query, Ok(reached));
                        continue;
                    }
                }
                if let Some(next) = next {
                    pending.push((query, Some(plan)));
                    pending.push((next, None));
                    continue;
                }
            }
            let result = self.finish(&query, plan);
            self.ranges.insert(query, result);
        }
        self.ranges[&root].clone()
    }
    /// Admission needs a fixedness proof, not exact extrema for a variable
    /// assertion. Keep witnessed variation separate from exact range caches.
    fn is_fixed(&mut self, expression: usize, byte: usize) -> Result<bool, CompileError> {
        let root = self.query(expression, byte);
        let mut pending = vec![(root.clone(), None)];
        while let Some((query, plan)) = pending.pop() {
            if self.fixed.contains_key(&query) {
                continue;
            }
            if let Some(exact) = self.ranges.get(&query) {
                self.fixed
                    .insert(query, exact.clone().map(FixedProof::from_outcomes));
                continue;
            }
            let Some(plan) = plan else {
                let plan = self.plan(&query);
                let children = match &plan {
                    Plan::Sum(children) => children.clone(),
                    Plan::Branches { children, .. } => {
                        children.first().cloned().into_iter().collect()
                    }
                    Plan::Constant | Plan::Closed(_) => {
                        let proof = self.evaluate(query.clone()).map(FixedProof::from_outcomes);
                        self.fixed.insert(query, proof);
                        continue;
                    }
                };
                pending.push((query, Some(plan)));
                pending.extend(children.into_iter().map(|child| (child, None)));
                continue;
            };
            let proof = match &plan {
                Plan::Sum(children) => {
                    let values = children
                        .iter()
                        .map(|child| self.fixed[child].clone())
                        .collect::<Vec<_>>();
                    if values
                        .iter()
                        .any(|value| matches!(value, Ok(FixedProof::Absent)))
                    {
                        Ok(FixedProof::Absent)
                    } else if let Some(error) = values.iter().find_map(|value| value.as_ref().err())
                    {
                        Err(error.clone())
                    } else if values
                        .iter()
                        .any(|value| matches!(value, Ok(FixedProof::Variable)))
                    {
                        Ok(FixedProof::Variable)
                    } else {
                        weighted_sum(
                            query.constant.clone(),
                            values.into_iter().map(|value| {
                                let Ok(FixedProof::Fixed(value)) = value else {
                                    unreachable!()
                                };
                                (
                                    Ok(Some(Outcomes::from_extent(Extent {
                                        min: value,
                                        max: Some(value),
                                    }))),
                                    BigUint::from(1u8),
                                )
                            }),
                            query.byte,
                        )
                        .map(FixedProof::from_outcomes)
                    }
                }
                Plan::Branches { children, .. } => {
                    let mut reached = None;
                    let mut next = None;
                    let mut proof = Ok(FixedProof::Absent);
                    for child in children {
                        match self.fixed.get(child) {
                            Some(Ok(FixedProof::Absent)) => {}
                            Some(Ok(FixedProof::Variable)) => {
                                proof = Ok(FixedProof::Variable);
                                break;
                            }
                            Some(Ok(FixedProof::Fixed(value))) => {
                                if reached.is_some_and(|old| old != *value) {
                                    proof = Ok(FixedProof::Variable);
                                    break;
                                }
                                reached = Some(*value);
                                proof = Ok(FixedProof::Fixed(*value));
                            }
                            Some(Err(error)) => {
                                proof = Err(error.clone());
                                break;
                            }
                            None => {
                                next = Some(child.clone());
                                break;
                            }
                        }
                    }
                    if let Some(next) = next {
                        pending.push((query, Some(plan)));
                        pending.push((next, None));
                        continue;
                    }
                    proof
                }
                Plan::Constant | Plan::Closed(_) => unreachable!(),
            };
            self.fixed.insert(query, proof);
        }
        self.fixed[&root]
            .clone()
            .map(|proof| !matches!(proof, FixedProof::Variable))
    }
    fn finish(&self, query: &Query, plan: Plan) -> Result<Option<Outcomes>, CompileError> {
        if query.absent {
            return Ok(None);
        }
        match plan {
            Plan::Constant => query
                .constant
                .extent(query.byte)
                .map(|value| Some(Outcomes::from_extent(value))),
            Plan::Sum(children) => {
                let values = children
                    .iter()
                    .map(|child| self.ranges[child].clone())
                    .collect::<Vec<_>>();
                weighted_sum(
                    query.constant.clone(),
                    values.into_iter().map(|value| (value, BigUint::from(1u8))),
                    query.byte,
                )
            }
            Plan::Branches { children, .. } => {
                let mut result = None;
                for child in children {
                    if let Some(value) = self.ranges[&child].clone()? {
                        result = Some(result.map_or(value, |known: Outcomes| known.union(value)));
                    }
                }
                Ok(result)
            }
            Plan::Closed(children) => {
                let mut values = Vec::new();
                for ((id, weight), children) in query.terms.iter().zip(children) {
                    let child = self.ranges[&children[0]].clone();
                    let value = match self.nodes[*id] {
                        Expression::Positive(_) => child.map(|value| {
                            value.and_then(|value| {
                                value.extent.positive().map(Outcomes::from_extent)
                            })
                        }),
                        Expression::Optional(_, count, byte) => child.and_then(|value| {
                            value
                                .map(|value| {
                                    value
                                        .extent
                                        .optional(count, byte)
                                        .map(Outcomes::from_extent)
                                })
                                .transpose()
                        }),
                        Expression::Present(_) => {
                            child.map(|value| value.map(|_| Outcomes::from_extent(Extent::ZERO)))
                        }
                        Expression::Project(_, _) => child
                            .map(|value| value.map(|value| Outcomes::from_extent(value.extent))),
                        _ => unreachable!("closed query atoms have no visible choice"),
                    };
                    values.push((value, weight.clone()));
                }
                weighted_sum(query.constant.clone(), values, query.byte)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct WideExtent {
    min: BigUint,
    max: Option<BigUint>,
}
impl WideExtent {
    fn zero() -> Self {
        Self {
            min: BigUint::from(0u8),
            max: Some(BigUint::from(0u8)),
        }
    }
    fn add(&mut self, value: &Extent, weight: &BigUint) {
        self.min += BigUint::from(value.min) * weight;
        self.max = self
            .max
            .as_ref()
            .zip(value.max)
            .map(|(a, b)| a + BigUint::from(b) * weight);
    }
    fn extent(&self, byte: usize) -> Result<Extent, CompileError> {
        fn native(value: &BigUint) -> Option<usize> {
            (value.bits() <= u64::from(usize::BITS))
                .then(|| value.to_u64_digits().first().copied().unwrap_or(0) as usize)
        }
        Extent::checked(
            native(&self.min),
            self.max
                .as_ref()
                .map(native)
                .map_or(Some(None), |value| value.map(Some)),
            byte,
        )
    }
}
/// Preserve per-path finite maxima and lower bounds across unbounded unions:
/// an unbounded sibling must not hide another path's addressability overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Outcomes {
    extent: Extent,
    largest_min: usize,
    largest_finite: Option<usize>,
}
impl Outcomes {
    fn from_extent(extent: Extent) -> Self {
        Self {
            extent,
            largest_min: extent.min,
            largest_finite: extent.max,
        }
    }
    fn union(self, other: Self) -> Self {
        Self {
            extent: self.extent.union(other.extent),
            largest_min: self.largest_min.max(other.largest_min),
            largest_finite: match (self.largest_finite, other.largest_finite) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
        }
    }
}
fn weighted_sum(
    constant: WideExtent,
    values: impl IntoIterator<Item = (Result<Option<Outcomes>, CompileError>, BigUint)>,
    byte: usize,
) -> Result<Option<Outcomes>, CompileError> {
    let values: Vec<_> = values.into_iter().collect();
    if values.iter().any(|(value, _)| matches!(value, Ok(None))) {
        return Ok(None);
    }
    let mut largest_min = constant.min.clone();
    let mut largest_finite = constant.max.clone();
    let mut sum = constant;
    for (value, weight) in values {
        let value = value?.expect("absent domains checked above");
        sum.add(&value.extent, &weight);
        largest_min += BigUint::from(value.largest_min) * &weight;
        largest_finite = largest_finite
            .as_ref()
            .zip(value.largest_finite)
            .map(|(a, b)| a + BigUint::from(b) * &weight);
    }
    let extent = sum.extent(byte)?;
    let observed = WideExtent {
        min: largest_min,
        max: largest_finite,
    }
    .extent(byte)?;
    Ok(Some(Outcomes {
        extent,
        largest_min: observed.min,
        largest_finite: observed.max,
    }))
}
/// Saturating upper observations use constant space per DAG node. They are
/// proofs of safety only; correlated possible overflow is resolved exactly.
#[derive(Clone, Copy)]
struct ArithmeticBounds {
    min: u128,
    finite: Option<u128>,
}
impl ArithmeticBounds {
    fn cap(value: u128) -> u128 {
        value.min(usize::MAX as u128 + 1)
    }
    fn zero() -> Self {
        Self {
            min: 0,
            finite: Some(0),
        }
    }
    fn may_overflow(self) -> bool {
        self.min > usize::MAX as u128 || self.finite.is_some_and(|max| max > usize::MAX as u128)
    }
    fn of(expression: &Expression, previous: &[Self]) -> Self {
        match expression {
            Expression::Value(Some(value)) => Self {
                min: value.min as u128,
                finite: value.max.map(|max| max as u128),
            },
            Expression::Value(None) | Expression::Present(_) => Self::zero(),
            Expression::Add(a, b, _) => Self {
                min: Self::cap(previous[*a].min.saturating_add(previous[*b].min)),
                finite: previous[*a]
                    .finite
                    .zip(previous[*b].finite)
                    .map(|(a, b)| Self::cap(a.saturating_add(b))),
            },
            Expression::Times(a, count, _) => Self {
                min: Self::cap(previous[*a].min.saturating_mul(*count as u128)),
                finite: previous[*a]
                    .finite
                    .map(|max| Self::cap(max.saturating_mul(*count as u128))),
            },
            Expression::Choice(_, branches) => Self {
                min: branches
                    .iter()
                    .map(|id| previous[*id].min)
                    .max()
                    .unwrap_or(0),
                finite: branches.iter().filter_map(|id| previous[*id].finite).max(),
            },
            Expression::Positive(a) => Self {
                min: previous[*a].min.max(1),
                finite: previous[*a].finite,
            },
            Expression::Optional(_, Some(0), _) => Self::zero(),
            Expression::Optional(a, Some(count), _) => Self {
                min: 0,
                finite: previous[*a]
                    .finite
                    .map(|max| Self::cap(max.saturating_mul(*count as u128))),
            },
            Expression::Optional(_, None, _) => Self {
                min: 0,
                finite: None,
            },
            Expression::Project(a, _) => previous[*a],
        }
    }
}
#[derive(Clone, Copy)]
enum FixedProof {
    Absent,
    Fixed(usize),
    Variable,
}
impl FixedProof {
    fn from_outcomes(value: Option<Outcomes>) -> Self {
        match value {
            None => Self::Absent,
            Some(value) if value.extent.max == Some(value.extent.min) => {
                Self::Fixed(value.extent.min)
            }
            Some(_) => Self::Variable,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Query {
    absent: bool,
    constant: WideExtent,
    terms: Vec<(usize, BigUint)>,
    byte: usize,
}
enum Plan {
    Constant,
    Sum(Vec<Query>),
    Branches {
        children: Vec<Query>,
        bounds: Vec<Query>,
    },
    Closed(Vec<Vec<Query>>),
}
impl Plan {
    fn dependencies(&self) -> Vec<Query> {
        match self {
            Self::Constant => Vec::new(),
            Self::Sum(queries) => queries.clone(),
            Self::Branches { children, bounds } => {
                if bounds.is_empty() {
                    children.first().cloned().into_iter().collect()
                } else {
                    bounds.clone()
                }
            }
            Self::Closed(queries) => queries.iter().flatten().cloned().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_dependent_width_keeps_fixed_and_variable_lookbehind_distinct() {
        Pattern::compile(r"(a)(?<=\1)").unwrap();
        assert!(matches!(
            Pattern::compile(r"(a+)(?<=\1)"),
            Err(CompileError::Invalid { .. })
        ));
        Pattern::compile(r"(a)\1(?<=a)").unwrap();
    }

    #[test]
    fn unrelated_capture_alternatives_do_not_multiply_width_states() {
        let choices = "(a|bb)".repeat(30);
        assert!(matches!(
            Pattern::compile(&(choices.clone() + r"(?<=\1)")),
            Err(CompileError::Invalid {
                reason: "variable-length lookbehind",
                ..
            })
        ));
        Pattern::compile(&("(a)".to_owned() + &choices + r"(?<=\1)")).unwrap();

        // Four captures share one alternative choice and always total three.
        // Later, unreferenced choices must not break that correlation or grow
        // the retained capture-state product.
        let correlated = r"(?:(a)(bc)|(ab)(c))".repeat(30) + r"(?<=\1\2\3\4)";
        Pattern::compile(&correlated).unwrap();
    }

    fn references(last: usize) -> String {
        (1..=last).map(|number| format!("\\{number}")).collect()
    }

    #[test]
    fn every_independent_ambiguous_capture_can_be_referenced_without_a_path_product() {
        let source = "(a|bb)".repeat(30) + "(?<=" + &references(30) + ")";
        assert!(matches!(
            Pattern::compile(&source),
            Err(CompileError::Invalid {
                reason: "variable-length lookbehind",
                ..
            })
        ));
        // A capturing wrapper must not hide the same independent choices.
        let source = "(".to_owned() + &"(a|bb)".repeat(30) + ")(?<=" + &references(31) + ")";
        assert!(matches!(
            Pattern::compile(&source),
            Err(CompileError::Invalid {
                reason: "variable-length lookbehind",
                ..
            })
        ));
    }

    #[test]
    fn every_correlated_capture_keeps_its_shared_choice_in_a_fixed_width_sum() {
        let choices = r"(?:(a)(bc)|(ab)(c))".repeat(30);
        let source = choices.clone() + "(?<=" + &references(120) + ")";
        Pattern::compile(&source).unwrap();
        // Group one captures all ninety scalars; the other references total
        // ninety as well, including inside a nested capturing wrapper.
        let source = "(".to_owned() + &choices + ")(?<=" + &references(121) + ")";
        Pattern::compile(&source).unwrap();
    }

    #[test]
    fn repeat_projection_preserves_outside_choices_and_last_capture_participation() {
        for source in [
            r"(?:(a)()|()(b))(\1){2}(?<=\5\4)",
            r"(?:(a)()|()(b))(\1){1,3}(?<=\5\4)",
            r"(?:(a)|(b)){1000000}(?<=\1\2)",
        ] {
            Pattern::compile(source).unwrap();
        }
        for source in [
            r"(?:(a)()|()(b))((?:\1){2})(?<=\5\4)",
            r"(?:(a)|(b)){0,1000000}(?<=\1\2)",
        ] {
            assert!(
                matches!(
                    Pattern::compile(source),
                    Err(CompileError::Invalid {
                        reason: "variable-length lookbehind",
                        ..
                    })
                ),
                "{source}"
            );
        }
    }

    fn doubling_capture(initial: &str, count: usize) -> String {
        let mut source = format!("({initial})");
        for number in 1..count {
            source.push_str(&format!("(\\{number}\\{number})"));
        }
        source + &format!("(?<=\\{count})")
    }

    #[test]
    fn shared_width_sums_are_normalized_once_per_expression() {
        // The final width is 2^29, while the authored source has thirty groups.
        Pattern::compile(&doubling_capture("a", 30)).unwrap();
        assert!(matches!(
            Pattern::compile(&doubling_capture("a", usize::BITS as usize + 1)),
            Err(CompileError::Unsupported {
                feature: "pattern width exceeds addressable range",
                ..
            })
        ));
        // An unbounded maximum remains unbounded, rather than becoming an
        // addressability overflow through a large symbolic coefficient.
        assert!(matches!(
            Pattern::compile(&doubling_capture("a*", 30)),
            Err(CompileError::Invalid {
                reason: "variable-length lookbehind",
                ..
            })
        ));
    }

    fn inspect_final_assertion(source: &str) -> (Extent, usize, usize, usize) {
        // Parse the same final assertion as lookahead to obtain its arena
        // without running width admission before inspecting its structural cost.
        let mut pattern = Pattern::compile(&source.replacen("(?<=", "(?=", 1)).unwrap();
        let id = pattern
            .nodes
            .iter()
            .position(|node| matches!(node.kind, Kind::Look { .. }))
            .unwrap();
        let Kind::Look { behind, .. } = &mut pattern.nodes[id].kind else {
            unreachable!()
        };
        *behind = true;
        let mut analysis = Analysis::new(&pattern.nodes, pattern.root);
        let initial = Path {
            width: analysis.expressions.zero,
            captures: 0,
            domain: analysis.expressions.zero,
        };
        analysis.visit(pattern.root, initial, false).unwrap();
        let extent = analysis
            .expressions
            .extent(analysis.behind[id].unwrap(), pattern.nodes[id].byte)
            .unwrap()
            .unwrap();
        (
            extent,
            analysis.lookup.len(),
            analysis.environments.len(),
            analysis.expressions.ranges.len(),
        )
    }

    #[test]
    fn indexed_capture_queries_do_not_retain_triangular_history_caches() {
        let count = 3000;
        let source = "(a)".repeat(count) + "(?<=" + &references(count) + ")";
        Pattern::compile(&source).unwrap();
        let (extent, lookups, environments, queries) = inspect_final_assertion(&source);
        assert_eq!(
            extent,
            Extent {
                min: count,
                max: Some(count)
            }
        );
        let height = (usize::BITS - count.leading_zeros()) as usize;
        assert!(lookups <= count * (height + 1), "{lookups} cached lookups");
        eprintln!("{count} fixed captures: {lookups} lookups, {environments} environments, {queries} queries");
    }

    #[test]
    fn connected_reference_choices_attain_proved_extrema_without_a_branch_product() {
        for count in [8, 12, 30] {
            let mut source = "(a|bb)".repeat(count);
            source.push_str("(?<=");
            for number in 1..count {
                source.push_str(&format!("(?:\\{number}|\\{})", number + 1));
            }
            source.push(')');
            let (extent, lookups, environments, queries) = inspect_final_assertion(&source);
            assert_eq!(
                extent,
                Extent {
                    min: count - 1,
                    max: Some(2 * (count - 1))
                }
            );
            assert!(
                queries < count * count * 50,
                "{count} choices retained {queries} queries"
            );
            eprintln!("{count} connected captures: {lookups} lookups, {environments} environments, {queries} queries");
        }
    }

    #[test]
    fn overflow_observation_preserves_discarded_bodies_and_source_bytes() {
        let maximum = usize::MAX;
        for (source, byte) in [
            (format!(r"(aa)\1{{{maximum}}}(?<=\1)"), 4),
            (format!(r"(aa)(?:\1{{{maximum}}}){{0}}(?<=\1)"), 7),
            (format!(r"(a*)(aa)\2{{{maximum}}}(?<=\2)"), 8),
        ] {
            assert_eq!(
                Pattern::compile(&source),
                Err(CompileError::Unsupported {
                    byte,
                    feature: "pattern width exceeds addressable range"
                }),
                "{source}"
            );
        }
        // Previous-iteration widths are marginalized before multiplication;
        // a projected unbounded range remains unbounded instead of restoring
        // a finite sibling that the structural union intentionally summarized.
        assert!(matches!(
            Pattern::compile(&format!(r"(a*|aa)\1{{{maximum}}}(?<=\1)")),
            Err(CompileError::Invalid {
                reason: "variable-length lookbehind",
                ..
            })
        ));
    }

    #[test]
    fn capability_overflow_ownership_uses_deterministic_operation_order() {
        for prefix in ["", "λ", "λ*"] {
            let body = format!(
                r"(?:(a)(bb)|(aa)(b))(?:\3{{{maximum}}}\2{{{maximum}}})",
                maximum = usize::MAX,
            );
            for consumed in [body.clone(), format!("(?:{body}){{0}}")] {
                let source = format!(r"{prefix}{consumed}(?<=\1)");
                assert_eq!(
                    Pattern::compile(&source),
                    Err(CompileError::Unsupported {
                        byte: source.find(r"\3").unwrap(),
                        feature: "pattern width exceeds addressable range",
                    }),
                    "{source}",
                );
            }
        }
    }

    #[test]
    fn indexed_middle_capture_reset_keeps_non_power_of_two_siblings() {
        let pattern = Pattern::compile(r"^(a)(?:(a)|(b)){2}(d)(?<=\1\2\3\4)e$").unwrap();
        assert_eq!(
            pattern.full_match("aabde").unwrap().unwrap().groups,
            [Some("aabde"), Some("a"), None, Some("b"), Some("d")]
        );
    }

    #[test]
    fn earlier_iteration_projection_keeps_its_unbounded_interval() {
        let maximum = usize::MAX;
        Pattern::compile(&format!(r"(?:(a*)|a{{{maximum}}}){{2}}()(?<=\2)")).unwrap();
    }

    fn dense_reference_choices(count: usize) -> String {
        let mut source = r"(?:(a)()|()(a))".repeat(count);
        source.push_str("(?<=");
        for left in 0..count {
            for right in left + 1..count {
                let (a, b, c, d) = (4 * left + 1, 4 * left + 4, 4 * right + 1, 4 * right + 4);
                source.push_str(&format!(r"(?:\{a}\{d}|\{b}\{c})"));
            }
        }
        source.push(')');
        source
    }

    fn inspect_arithmetic(source: &str) -> (Result<(), CompileError>, usize) {
        let mut pattern = Pattern::compile(&source.replacen("(?<=", "(?=", 1)).unwrap();
        let assertion = pattern
            .nodes
            .iter()
            .find(|node| matches!(node.kind, Kind::Look { .. }))
            .unwrap()
            .byte;
        for node in &mut pattern.nodes {
            // Restore exact source coordinates after removing '<' for parsing.
            if node.byte > assertion {
                node.byte += 1;
            }
            if let Kind::Look { behind, .. } = &mut node.kind {
                *behind = true;
            }
        }
        let mut analysis = Analysis::new(&pattern.nodes, pattern.root);
        let initial = Path {
            width: analysis.expressions.zero,
            captures: 0,
            domain: analysis.expressions.zero,
        };
        analysis.visit(pattern.root, initial, false).unwrap();
        let result = analysis.expressions.check_arithmetic();
        (result, analysis.expressions.ranges.len())
    }

    #[test]
    fn reachable_overflow_does_not_require_exact_dense_choice_extrema() {
        for count in [4, 8, 16] {
            let dense = dense_reference_choices(count);
            let mut sources = vec![(
                dense.clone() + &format!(r"\1{{{}}}", usize::MAX),
                dense.len(),
            )];
            // Lookbehind visits these sums backwards: the dense sum has been
            // built before its huge independent or correlated summand is added.
            // The two last captures require opposite branches for overflow.
            for term in [
                "a".to_owned(),
                format!(r"\{}", 4 * count - 3),
                format!(r"\{}", 4 * count),
            ] {
                sources.push((
                    dense.replacen("(?<=", &format!(r"(?<={term}{{{}}}", usize::MAX), 1),
                    dense.find("(?<=").unwrap() + 4,
                ));
            }
            for (source, byte) in sources {
                let expected = CompileError::Unsupported {
                    byte,
                    feature: "pattern width exceeds addressable range",
                };
                let (result, queries) = inspect_arithmetic(&source);
                assert_eq!(result, Err(expected.clone()), "{source}");
                assert_eq!(Pattern::compile(&source), Err(expected), "{source}");
                assert!(
                    queries < count * count * 8,
                    "{count} choices retained {queries} exact arithmetic queries"
                );
                eprintln!("{count} dense arithmetic choices: {queries} exact queries");
            }
        }
    }

    #[test]
    fn non_overflowing_witness_still_requires_complete_arithmetic_and_fixedness_proofs() {
        // Each branch has exactly one participating capture, so the relaxed
        // two-MAX sum is unreachable and the actual width is exactly MAX.
        Pattern::compile(&format!(
            r"(?:(a)()|()(a))(?<=\1{{{0}}}\4{{{0}}})",
            usize::MAX
        ))
        .unwrap();
        // The promising MAX branch is safe, but another branch has width two.
        assert!(matches!(
            Pattern::compile(&format!(r"(?:(a)()|()(aa))(?<=\1{{{}}}\4)", usize::MAX)),
            Err(CompileError::Invalid {
                reason: "variable-length lookbehind",
                ..
            })
        ));
        // The first outer alternative has incompatible per-term maxima, so
        // its promising assignment is safe. The other outer alternative has
        // two participating captures and really overflows at the final sum.
        let source = format!(
            r"(?:(?:(a)()|()(a))|(a)(a))(?<=\6{{{0}}}\5{{{0}}}\1{{{0}}}\4{{{0}}})",
            usize::MAX
        );
        assert_eq!(
            Pattern::compile(&source),
            Err(CompileError::Unsupported {
                byte: source.find(r"\6").unwrap(),
                feature: "pattern width exceeds addressable range",
            })
        );
    }

    #[test]
    fn witnessed_variation_avoids_exact_dense_choice_optimization() {
        // Independent assignment enumeration gives E +/- floor(N^2/4).
        // These extrema are strictly inside the per-term relaxed bound for N>=3.
        for count in [3, 4] {
            let edges = count * (count - 1) / 2;
            let cut = count * count / 4;
            let (extent, _, _, queries) = inspect_final_assertion(&dense_reference_choices(count));
            assert_eq!(
                extent,
                Extent {
                    min: edges - cut,
                    max: Some(edges + cut)
                }
            );
            eprintln!("{count} dense captures: {queries} exact queries");
        }
        for count in [4, 8, 16] {
            let source = dense_reference_choices(count);
            assert!(matches!(
                Pattern::compile(&source),
                Err(CompileError::Invalid {
                    reason: "variable-length lookbehind",
                    ..
                })
            ));
            let mut pattern = Pattern::compile(&source.replacen("(?<=", "(?=", 1)).unwrap();
            let id = pattern
                .nodes
                .iter()
                .position(|node| matches!(node.kind, Kind::Look { .. }))
                .unwrap();
            let Kind::Look { behind, .. } = &mut pattern.nodes[id].kind else {
                unreachable!()
            };
            *behind = true;
            let mut analysis = Analysis::new(&pattern.nodes, pattern.root);
            let initial = Path {
                width: analysis.expressions.zero,
                captures: 0,
                domain: analysis.expressions.zero,
            };
            analysis.visit(pattern.root, initial, false).unwrap();
            analysis.expressions.check_arithmetic().unwrap();
            assert!(!analysis
                .expressions
                .is_fixed(analysis.behind[id].unwrap(), pattern.nodes[id].byte)
                .unwrap());
            let queries = analysis.expressions.fixed.len();
            assert!(
                queries < count * count * 20,
                "{count} captures retained {queries} fixedness queries"
            );
            eprintln!("{count} dense captures: {queries} fixedness queries");
        }
    }
}
