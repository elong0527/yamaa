//! Persistent indexed capture widths, adapted from the incoming shared-width
//! analyzer. Each root denotes one immutable capture environment; branch joins
//! share roots and preserve a choice only on indexed paths whose values differ.
//! Domains belong to Path, so a lookup never imports unrelated prefix guards.
use super::symbolic;
use std::collections::BTreeMap;

#[derive(Clone)]
enum Environment {
    Empty,
    Leaf(usize),
    Branch(usize, usize),
    Choice { choice: usize, branches: Vec<usize> },
}
pub(super) struct Captures {
    nodes: Vec<Environment>,
    groups: usize,
    splits: BTreeMap<usize, (usize, usize)>,
    lookup: BTreeMap<(usize, usize), usize>,
}
impl Captures {
    pub fn new(groups: usize) -> Self {
        Self {
            nodes: vec![Environment::Empty],
            groups,
            splits: BTreeMap::new(),
            lookup: BTreeMap::new(),
        }
    }
    pub fn choice(&mut self, choice: usize, branches: Vec<usize>) -> usize {
        self.push(Environment::Choice { choice, branches })
    }
    fn push(&mut self, environment: Environment) -> usize {
        match &environment {
            Environment::Leaf(width) if *width == 0 => return 0,
            Environment::Branch(0, 0) => return 0,
            Environment::Choice { branches, .. }
                if branches.iter().all(|branch| *branch == branches[0]) =>
            {
                return branches[0];
            }
            _ => {}
        }
        let id = self.nodes.len();
        self.nodes.push(environment);
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
            let children = match self.nodes[id].clone() {
                Environment::Empty => Some((0, 0)),
                Environment::Branch(left, right) => Some((left, right)),
                Environment::Leaf(_) => unreachable!("only non-leaf capture intervals split"),
                Environment::Choice { choice, branches } => {
                    if ready || branches.iter().all(|child| self.splits.contains_key(child)) {
                        let left = branches
                            .iter()
                            .map(|&child| self.splits[&child].0)
                            .collect();
                        let right = branches
                            .iter()
                            .map(|&child| self.splits[&child].1)
                            .collect();
                        Some((
                            self.push(Environment::Choice {
                                choice,
                                branches: left,
                            }),
                            self.push(Environment::Choice {
                                choice,
                                branches: right,
                            }),
                        ))
                    } else {
                        pending.push((id, true));
                        pending.extend(branches.iter().map(|&child| (child, false)));
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
    pub fn set(&mut self, mut root: usize, number: usize, width: usize) -> usize {
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
        root = self.push(Environment::Leaf(width));
        while let Some((right, sibling)) = path.pop() {
            root = if right {
                self.push(Environment::Branch(sibling, root))
            } else {
                self.push(Environment::Branch(root, sibling))
            };
        }
        root
    }
    pub fn clear(&mut self, root: usize, range: std::ops::Range<usize>) -> usize {
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
                self.push(Environment::Branch(children.0, children.1))
            };
            cleared.insert(key, next);
        }
        cleared[&(root, 0, self.groups)]
    }
    /// Indexed lookup and lazy choice conditioning use explicit work frames.
    pub fn get(
        &mut self,
        environment: usize,
        number: usize,
        expressions: &mut symbolic::Arena,
    ) -> usize {
        let mut pending = vec![(environment, 0, self.groups, false)];
        while let Some((id, start, end, ready)) = pending.pop() {
            if self.lookup.contains_key(&(id, number)) {
                continue;
            }
            let result = match self.nodes[id].clone() {
                Environment::Empty => Some(0),
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
                            .all(|parent| self.lookup.contains_key(&(*parent, number)))
                    {
                        let values = branches
                            .iter()
                            .map(|&parent| self.lookup[&(parent, number)])
                            .collect();
                        Some(expressions.choice(choice, values))
                    } else {
                        pending.push((id, start, end, true));
                        pending.extend(branches.iter().map(|&parent| (parent, start, end, false)));
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
}
