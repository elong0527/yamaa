//! Structural width admission with capture state and correlated alternative paths.
//! This does not execute subjects or use literal/assertion satisfiability to
//! exempt variable syntax. Intervals describe consumption, not captured text.
use super::*;

/// Use the existing scalar arithmetic when no capture-dependent pass is needed.
pub(super) fn validate(nodes: &[Node], root: usize, groups: usize) -> Result<(), CompileError> {
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
    let mut analysis = Analysis {
        nodes,
        behind: Vec::new(),
    };

    analysis.behind.resize(nodes.len(), None);

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
    /// Concatenate consumption intervals without subtracting unrelated bounds.
    fn add(self, other: Self, byte: usize) -> Result<Self, CompileError> {
        let max = match (self.max, other.max) {
            (Some(a), Some(b)) => a.checked_add(b).map(Some),
            _ => Some(None),
        };
        Self::checked(self.min.checked_add(other.min), max, byte)
    }
    /// Multiply required iterations symbolically, including exact zero counts.
    fn times(self, count: usize, byte: usize) -> Result<Self, CompileError> {
        if count == 0 {
            return Ok(Self::ZERO);
        }
        let max = match self.max {
            Some(value) => value.checked_mul(count).map(Some),
            None => Some(None),
        };
        Self::checked(self.min.checked_mul(count), max, byte)
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Path {
    width: Extent,
    // Unentered and empty captures both consume zero as references. The matcher
    // still retains their distinct participation/result values independently.
    captures: Vec<Extent>,
}
struct Analysis<'a> {
    nodes: &'a [Node],
    behind: Vec<Option<Extent>>,
}
impl Analysis<'_> {
    /// Equal width/capture states can share a continuation regardless of literals.
    fn push(&mut self, paths: &mut Vec<Path>, path: Path) {
        for known in paths.iter() {
            if known == &path {
                return;
            }
        }

        paths.push(path);
    }
    /// Traverse flat sequences iteratively; only grammar-bounded nesting recurses.
    fn visit(
        &mut self,
        id: usize,
        mut path: Path,
        backwards: bool,
    ) -> Result<Vec<Path>, CompileError> {
        let nodes = self.nodes;
        let node = &nodes[id];
        match &node.kind {
            Kind::Literal(_) | Kind::Dot | Kind::Class { .. } => {
                path.width = path.width.add(Extent::ONE, node.byte)?;
                Ok(vec![path])
            }
            Kind::Start | Kind::End | Kind::Boundary(_) => Ok(vec![path]),
            Kind::Backreference(Reference::Number(number)) => {
                path.width = path.width.add(path.captures[*number], node.byte)?;
                Ok(vec![path])
            }
            Kind::Backreference(Reference::Name(_)) => unreachable!("references already resolved"),
            Kind::Sequence(children) => {
                let mut paths = Ok(vec![path])?;
                for position in 0..children.len() {
                    let child = children[if backwards {
                        children.len() - 1 - position
                    } else {
                        position
                    }];
                    let mut next = Vec::new();
                    for path in paths {
                        for result in self.visit(child, path, backwards)? {
                            self.push(&mut next, result);
                        }
                    }
                    paths = next;
                }
                Ok(paths)
            }
            Kind::Alternative(children) => {
                let mut paths = Vec::new();
                for &child in children {
                    let branch = path.clone();
                    for result in self.visit(child, branch, backwards)? {
                        self.push(&mut paths, result);
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
                    path.width = prefix.add(path.width, node.byte)?;
                }
                Ok(paths)
            }
            Kind::Look {
                child,
                behind,
                positive,
            } => {
                let prefix = path.width;
                let original = if !positive { Some(path.clone()) } else { None };
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
                    Ok(vec![original])
                } else {
                    Ok(paths)
                }
            }
            Kind::Repeat {
                child, min, max, ..
            } => {
                let prefix = path.width;
                let mut body = path.clone();
                body.width = Extent::ZERO;

                for capture in nodes[*child].captures.clone() {
                    body.captures[capture] = Extent::ZERO;
                }
                // Even a zero-count body must satisfy nested assertion grammar.
                let last_paths = self.visit(*child, body, backwards)?;
                if *max == Some(0) {
                    return Ok(vec![path]);
                }
                let any = last_paths
                    .iter()
                    .map(|path| path.width)
                    .reduce(Extent::union)
                    .expect("structural alternatives have at least one path");
                let mut paths = Vec::new();
                if *min == 0 {
                    self.push(&mut paths, path);
                }
                for mut last in last_paths {
                    let last_width = last.width;
                    if *min > 0 {
                        let mut required = last.clone();
                        required.width = prefix
                            .add(any.times(min - 1, node.byte)?, node.byte)?
                            .add(last_width, node.byte)?;
                        self.push(&mut paths, required);
                    }
                    if max.is_none_or(|max| max > *min) {
                        if let Some(positive) = last_width.positive() {
                            let earlier_optional = any
                                .positive()
                                .expect("a positive last path exists")
                                .optional(max.map(|max| max - min - 1), node.byte)?;
                            last.width = prefix
                                .add(any.times(*min, node.byte)?, node.byte)?
                                .add(earlier_optional, node.byte)?
                                .add(positive, node.byte)?;
                            self.push(&mut paths, last);
                        }
                    }
                }
                Ok(paths)
            }
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
}
