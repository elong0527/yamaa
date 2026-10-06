//! Structural width admission with capture state and correlated alternative paths.
//! This does not execute subjects or use literal/assertion satisfiability to
//! exempt variable syntax. Intervals describe consumption, not captured text.
use super::*;

/// Use the existing scalar arithmetic when no capture-dependent pass is needed.
pub(super) fn validate(
    nodes: &[Node],
    root: usize,
    groups: usize,
    limits: CompileLimits,
) -> Result<(), CompileError> {
    let has_behind = nodes
        .iter()
        .any(|node| matches!(node.kind, Kind::Look { behind: true, .. }));
    if !has_behind {
        return Ok(());
    }
    if !nodes
        .iter()
        .any(|node| matches!(node.kind, Kind::Backreference(_)))
    {
        for node in nodes {
            if let Kind::Look {
                child,
                behind: true,
                ..
            } = node.kind
            {
                if nodes[child].width == Width::Variable {
                    return Err(variable(node.byte));
                }
            }
        }
        return Ok(());
    }
    let mut analysis = Analysis {
        nodes,
        limits,
        work: 0,
        cells: 0,
        behind: Vec::new(),
    };
    analysis.cells(nodes.len())?;
    analysis.behind.resize(nodes.len(), None);
    analysis.cells(groups + 2)?;
    let initial = Path {
        width: Extent::ZERO,
        captures: vec![Extent::ZERO; groups + 1],
    };
    analysis.visit(root, initial, false)?;
    // Retain deterministic arena/source ownership rather than returning whichever
    // reverse traversal happens to discover a variable assertion first.
    for (id, node) in nodes.iter().enumerate() {
        if matches!(node.kind, Kind::Look { behind: true, .. }) {
            let width = analysis.behind[id].expect("every assertion is structurally visited");
            if width.max != Some(width.min) {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Width arithmetic overflow and fixed-width ceilings remain policy errors.
    fn checked(
        min: Option<usize>,
        max: Option<Option<usize>>,
        limit: usize,
    ) -> Result<Self, CompileError> {
        let error = || CompileError::Limit {
            resource: Resource::Width,
            limit,
        };
        let value = Self {
            min: min.ok_or_else(error)?,
            max: max.ok_or_else(error)?,
        };
        if value.max == Some(value.min) && value.min > limit {
            return Err(error());
        }
        Ok(value)
    }
    /// Concatenate consumption intervals without subtracting unrelated bounds.
    fn add(self, other: Self, limit: usize) -> Result<Self, CompileError> {
        let max = match (self.max, other.max) {
            (Some(a), Some(b)) => a.checked_add(b).map(Some),
            _ => Some(None),
        };
        Self::checked(self.min.checked_add(other.min), max, limit)
    }
    /// Multiply required iterations symbolically, including exact zero counts.
    fn times(self, count: usize, limit: usize) -> Result<Self, CompileError> {
        if count == 0 {
            return Ok(Self::ZERO);
        }
        let max = match self.max {
            Some(value) => value.checked_mul(count).map(Some),
            None => Some(None),
        };
        Self::checked(self.min.checked_mul(count), max, limit)
    }
    /// Zero or more strictly consuming optional iterations, bounded or unbounded.
    fn optional(self, maximum: Option<usize>, limit: usize) -> Result<Self, CompileError> {
        if maximum == Some(0) {
            return Ok(Self::ZERO);
        }
        let max = match (self.max, maximum) {
            (Some(width), Some(count)) => width.checked_mul(count).map(Some),
            _ => Some(None),
        };
        Self::checked(Some(0), max, limit)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Path {
    width: Extent,
    // Unentered and empty captures both consume zero as references. The matcher
    // still retains their distinct participation/result values independently.
    captures: Vec<Extent>,
}
struct Analysis<'a> {
    nodes: &'a [Node],
    limits: CompileLimits,
    work: usize,
    cells: usize,
    behind: Vec<Option<Extent>>,
}
impl Analysis<'_> {
    /// Bound visits and comparisons cumulatively across the entire pattern.
    fn work(&mut self, n: usize) -> Result<(), CompileError> {
        self.work = self
            .work
            .checked_add(n)
            .filter(|&n| n <= self.limits.width_work)
            .ok_or(CompileError::Limit {
                resource: Resource::WidthWork,
                limit: self.limits.width_work,
            })?;
        Ok(())
    }
    /// Charge logical slots before allocation; allocator capacity is not counted.
    fn cells(&mut self, n: usize) -> Result<(), CompileError> {
        self.cells = self
            .cells
            .checked_add(n)
            .filter(|&n| n <= self.limits.width_cells)
            .ok_or(CompileError::Limit {
                resource: Resource::WidthCells,
                limit: self.limits.width_cells,
            })?;
        Ok(())
    }
    /// Preserve correlated capture vectors without allocating an uncharged clone.
    fn copy(&mut self, path: &Path) -> Result<Path, CompileError> {
        self.cells(path.captures.len() + 1)?;
        Ok(path.clone())
    }
    /// Equal width/capture states can share a continuation regardless of literals.
    fn push(&mut self, paths: &mut Vec<Path>, path: Path) -> Result<(), CompileError> {
        for known in paths.iter() {
            self.work(path.captures.len() + 1)?;
            if known == &path {
                return Ok(());
            }
        }
        self.cells(1)?;
        paths.push(path);
        Ok(())
    }
    /// Create one charged result slot while moving its owned capture state.
    fn one(&mut self, path: Path) -> Result<Vec<Path>, CompileError> {
        self.cells(1)?;
        Ok(vec![path])
    }
    /// Traverse flat sequences iteratively; only grammar-bounded nesting recurses.
    fn visit(
        &mut self,
        id: usize,
        mut path: Path,
        backwards: bool,
    ) -> Result<Vec<Path>, CompileError> {
        self.work(1)?;
        let nodes = self.nodes;
        let node = &nodes[id];
        match &node.kind {
            Kind::Literal(_) | Kind::Dot | Kind::Class { .. } => {
                path.width = path.width.add(Extent::ONE, self.limits.width)?;
                self.one(path)
            }
            Kind::Start | Kind::End | Kind::Boundary(_) => self.one(path),
            Kind::Backreference(Reference::Number(number)) => {
                path.width = path.width.add(path.captures[*number], self.limits.width)?;
                self.one(path)
            }
            Kind::Backreference(Reference::Name(_)) => unreachable!("references already resolved"),
            Kind::Sequence(children) => {
                let mut paths = self.one(path)?;
                for position in 0..children.len() {
                    let child = children[if backwards {
                        children.len() - 1 - position
                    } else {
                        position
                    }];
                    let mut next = Vec::new();
                    for path in paths {
                        for result in self.visit(child, path, backwards)? {
                            self.push(&mut next, result)?;
                        }
                    }
                    paths = next;
                }
                Ok(paths)
            }
            Kind::Alternative(children) => {
                let mut paths = Vec::new();
                for &child in children {
                    let branch = self.copy(&path)?;
                    for result in self.visit(child, branch, backwards)? {
                        self.push(&mut paths, result)?;
                    }
                }
                Ok(paths)
            }
            Kind::Group { child, number } => {
                let prefix = path.width;
                path.width = Extent::ZERO;
                let mut paths = self.visit(*child, path, backwards)?;
                for path in &mut paths {
                    if let Some(number) = number {
                        path.captures[*number] = path.width;
                    }
                    path.width = prefix.add(path.width, self.limits.width)?;
                }
                Ok(paths)
            }
            Kind::Look {
                child,
                behind,
                positive,
            } => {
                let prefix = path.width;
                let original = if !positive {
                    Some(self.copy(&path)?)
                } else {
                    None
                };
                path.width = Extent::ZERO;
                let mut paths = self.visit(*child, path, *behind)?;
                for path in &mut paths {
                    if *behind {
                        self.behind[id] = Some(
                            self.behind[id].map_or(path.width, |known| known.union(path.width)),
                        );
                    }
                    path.width = prefix;
                }
                if let Some(original) = original {
                    self.one(original)
                } else {
                    Ok(paths)
                }
            }
            Kind::Repeat {
                child, min, max, ..
            } => {
                let prefix = path.width;
                let mut body = self.copy(&path)?;
                body.width = Extent::ZERO;
                self.work(nodes[*child].captures.len())?;
                for capture in nodes[*child].captures.clone() {
                    body.captures[capture] = Extent::ZERO;
                }
                // Even a zero-count body must satisfy nested assertion grammar.
                let last_paths = self.visit(*child, body, backwards)?;
                if *max == Some(0) {
                    return self.one(path);
                }
                let any = last_paths
                    .iter()
                    .map(|path| path.width)
                    .reduce(Extent::union)
                    .expect("structural alternatives have at least one path");
                let mut paths = Vec::new();
                if *min == 0 {
                    self.push(&mut paths, path)?;
                }
                for mut last in last_paths {
                    let last_width = last.width;
                    if *min > 0 {
                        let mut required = self.copy(&last)?;
                        required.width = prefix
                            .add(any.times(min - 1, self.limits.width)?, self.limits.width)?
                            .add(last_width, self.limits.width)?;
                        self.push(&mut paths, required)?;
                    }
                    if max.is_none_or(|max| max > *min) {
                        if let Some(positive) = last_width.positive() {
                            let earlier_optional = any
                                .positive()
                                .expect("a positive last path exists")
                                .optional(max.map(|max| max - min - 1), self.limits.width)?;
                            last.width = prefix
                                .add(any.times(*min, self.limits.width)?, self.limits.width)?
                                .add(earlier_optional, self.limits.width)?
                                .add(positive, self.limits.width)?;
                            self.push(&mut paths, last)?;
                        }
                    }
                }
                Ok(paths)
            }
        }
    }
}
