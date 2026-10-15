//! Structural width admission with capture state and correlated alternative paths.
//! This does not execute subjects or use literal/assertion satisfiability to
//! exempt variable syntax. Intervals describe consumption, not captured text.
use super::*;
use std::collections::BTreeSet;
mod captures;
mod symbolic;

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
    let mut analysis = Analysis {
        nodes,
        behind: Vec::new(),
        referenced: nodes
            .iter()
            .filter_map(|node| match node.kind {
                Kind::Backreference(Reference::Number(number)) => Some(number),
                _ => None,
            })
            .collect(),
        arena: symbolic::Arena::new(),
        captures: captures::Captures::new(nodes[root].captures.end),
    };

    analysis.behind.resize(nodes.len(), None);

    let initial = Path {
        width: 0,
        captures: 0,
        guards: BTreeSet::new(),
    };
    analysis.visit(root, initial, false)?;
    // Retain deterministic arena/source ownership rather than returning whichever
    // reverse traversal happens to discover a variable assertion first.
    for (id, node) in nodes.iter().enumerate() {
        if matches!(node.kind, Kind::Look { behind: true, .. }) {
            let expression = analysis.behind[id].expect("every assertion is structurally visited");
            if !analysis.arena.is_fixed(expression, node.byte)? {
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
}

#[derive(Clone)]
struct Path {
    width: symbolic::Id,
    // Unentered and empty captures both consume zero as references. The matcher
    // retains their distinct participation/result values independently.
    captures: usize,
    // Independent domain factors stay outside consumption and later captures.
    guards: BTreeSet<symbolic::Id>,
}
struct Analysis<'a> {
    nodes: &'a [Node],
    behind: Vec<Option<symbolic::Id>>,
    referenced: BTreeSet<usize>,
    arena: symbolic::Arena,
    captures: captures::Captures,
}
impl Analysis<'_> {
    /// A syntax join creates one choice shared by its width and capture slots.
    /// If all captures agree, union consumption immediately as the interval
    /// analysis did, without retaining an unobservable branch identity.
    fn join(&mut self, paths: Vec<Path>) -> Path {
        let first = &paths[0];
        if paths
            .iter()
            .all(|path| path.captures == first.captures && path.guards == first.guards)
        {
            return Path {
                width: self
                    .arena
                    .union(paths.iter().map(|path| path.width).collect()),
                captures: first.captures,
                guards: first.guards.clone(),
            };
        }
        let choice = self.arena.new_choice(paths.len());
        let width = self
            .arena
            .choice(choice, paths.iter().map(|path| path.width).collect());
        let mut guards = first.guards.clone();
        guards.retain(|guard| paths.iter().all(|path| path.guards.contains(guard)));
        let mut domains = Vec::with_capacity(paths.len());
        for path in &paths {
            let mut domain = 0;
            for &guard in path.guards.difference(&guards) {
                domain = self.arena.add(domain, guard);
            }
            domains.push(domain);
        }
        let domain = self.arena.choice(choice, domains);
        if domain != 0 {
            guards.insert(domain);
        }
        let captures = self
            .captures
            .choice(choice, paths.iter().map(|path| path.captures).collect());
        Path {
            width,
            captures,
            guards,
        }
    }

    fn guarded(&mut self, path: &Path) -> symbolic::Id {
        let mut width = path.width;
        for &guard in &path.guards {
            width = self.arena.add(width, guard);
        }
        width
    }
    /// Flat sequences are iterative. Recursion follows only grammar nesting;
    /// capture-dependent expression chains live in the separate indexed arena.
    fn visit(&mut self, id: usize, mut path: Path, backwards: bool) -> Result<Path, CompileError> {
        let nodes = self.nodes;
        let node = &nodes[id];
        let result = match &node.kind {
            Kind::Literal(_) | Kind::Dot | Kind::Class { .. } => {
                let one = self.arena.value(Extent::ONE);
                path.width = self.arena.add(path.width, one);
                path
            }
            Kind::Start | Kind::End | Kind::Boundary(_) => path,
            Kind::Backreference(Reference::Number(number)) => {
                let width = self.captures.get(path.captures, *number, &mut self.arena);
                path.width = self.arena.add(path.width, width);
                path
            }
            Kind::Backreference(Reference::Name(_)) => unreachable!("references already resolved"),
            Kind::Sequence(children) => {
                for position in 0..children.len() {
                    let child = children[if backwards {
                        children.len() - 1 - position
                    } else {
                        position
                    }];
                    path = self.visit(child, path, backwards)?;
                }
                path
            }
            Kind::Alternative(children) => {
                let mut paths = Vec::with_capacity(children.len());
                for &child in children {
                    paths.push(self.visit(child, path.clone(), backwards)?);
                }
                self.join(paths)
            }
            Kind::Group { child, number } => {
                let prefix = path.width;
                path.width = 0;
                let mut path = self.visit(*child, path, backwards)?;
                if let Some(number) = number.filter(|number| self.referenced.contains(number)) {
                    path.captures = self.captures.set(path.captures, number, path.width);
                }
                path.width = self.arena.add(prefix, path.width);
                path
            }
            Kind::Look {
                child,
                behind,
                positive,
            } => {
                let original = path.clone();
                path.width = 0;
                let mut path = self.visit(*child, path, *behind)?;
                if *behind {
                    self.behind[id] = Some(self.guarded(&path));
                }
                if *positive {
                    path.width = original.width;
                    path
                } else {
                    original
                }
            }
            Kind::Repeat {
                child, min, max, ..
            } => {
                let prefix = path.width;
                let mut body = path.clone();
                body.width = 0;
                body.captures = self
                    .captures
                    .clear(body.captures, nodes[*child].captures.clone());
                let first_choice = self.arena.choices();
                // Zero-count bodies still undergo nested assertion admission.
                let last = self.visit(*child, body, backwards)?;
                let end_choice = self.arena.choices();
                if *max == Some(0) {
                    return Ok(path);
                }
                let mut body_width = last.width;
                for &guard in last.guards.difference(&path.guards) {
                    body_width = self.arena.add(body_width, guard);
                }
                let any = self.arena.quantify(body_width, first_choice..end_choice);
                let mut paths = Vec::new();
                if *min == 0 {
                    paths.push(path);
                }
                if *min > 0 {
                    let mut required = last.clone();
                    let earlier = self.arena.times(any, min - 1);
                    let width = self.arena.add(prefix, earlier);
                    required.width = self.arena.add(width, last.width);
                    paths.push(required);
                }
                if max.is_none_or(|max| max > *min) {
                    let mut optional = last.clone();
                    let positive_last = self.arena.positive(last.width);
                    let positive_any = self.arena.positive(any);
                    let earlier_optional = self
                        .arena
                        .optional(positive_any, max.map(|max| max - min - 1));
                    let earlier_required = self.arena.times(any, *min);
                    let width = self.arena.add(prefix, earlier_required);
                    let width = self.arena.add(width, earlier_optional);
                    let width = self.arena.add(width, positive_last);
                    let guard = self.arena.valid(positive_last);
                    optional.width = width;
                    if guard != 0 {
                        optional.guards.insert(guard);
                    }
                    paths.push(optional);
                }
                self.join(paths)
            }
        };
        // Conservative bounds handle ordinary prefixes without exact queries.
        // Only a possible arithmetic overflow needs correlation-aware checking.
        let checked = self.guarded(&result);
        self.arena.check(checked, node.byte)?;
        Ok(result)
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
}
