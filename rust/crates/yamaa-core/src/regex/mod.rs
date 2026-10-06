//! Bounded, host-independent R022 pattern compilation and matching.
//! No native binding or dataset capability is enabled by this internal core.

use alloc::{string::String, vec, vec::Vec};
mod identifier_data;
mod identifiers;
mod parser;
mod widths;

// Request-owned accounting for capture-dependent compilation across literals.
pub(crate) use widths::Budget as CompileBudget;

/// Pinned ID_Start / ID_Continue data used for capture-group name admission.
pub const IDENTIFIER_UNICODE_VERSION: &str = "18.0.0";

/// The repository's portable pattern contract, not a host library dialect.
pub const CONTRACT_VERSION: &str = "2.0.0";

/// Caller-selected compilation budgets; recursive pattern nesting is capped at 64.
#[derive(Clone, Copy, Debug)]
pub struct CompileLimits {
    pub bytes: usize,
    pub nodes: usize,
    pub groups: usize,
    pub depth: usize,
    pub repetition: usize,
    /// Maximum statically fixed consumed width; no repetition is expanded.
    pub width: usize,
    /// Cumulative visits/comparisons for capture-dependent lookbehind admission.
    pub width_work: usize,
    /// Cumulative logical capture/path slots allocated during width analysis.
    pub width_cells: usize,
}
impl Default for CompileLimits {
    /// Conservative defaults are compiler/matcher policies, not language limits.
    fn default() -> Self {
        Self {
            bytes: 65_536,
            nodes: 4096,
            groups: 256,
            depth: 64,
            repetition: 1_000_000,
            width: 1_048_576,
            width_work: 1_000_000,
            width_cells: 1_000_000,
        }
    }
}

/// Resource policy is distinct from language rejection and execution failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    PatternBytes,
    Nodes,
    Groups,
    Depth,
    Repetition,
    Width,
    WidthWork,
    WidthCells,
    SubjectBytes,
    Work,
    StateCells,
}

/// Invalid syntax owns REQ-0827. Unsupported features never masquerade as invalid syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    Invalid { byte: usize, reason: &'static str },
    Limit { resource: Resource, limit: usize },
    Unsupported { byte: usize, feature: &'static str },
}

/// Per-call budgets span every candidate position, backtrack and lookaround.
/// These are independent ceilings, not a guarantee that every admitted input
/// fits the remaining work/storage budgets. Callers may raise them separately.
#[derive(Clone, Copy, Debug)]
pub struct MatchLimits {
    /// Input byte ceiling checked before scalar indexing or matching.
    pub subject_bytes: usize,
    pub work: usize,
    /// Cumulative logical task/capture/scalar cells created or copied.
    /// Allocator capacity and process-wide memory are not measured by this count.
    pub state_cells: usize,
}
impl Default for MatchLimits {
    /// Conservative defaults are compiler/matcher policies, not language limits.
    fn default() -> Self {
        Self {
            subject_bytes: 1_048_576,
            work: 1_000_000,
            state_cells: 1_000_000,
        }
    }
}

/// A policy refusal cannot be confused with no match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchError {
    pub resource: Resource,
    pub limit: usize,
}

/// Group zero is the whole match; None is an unentered group, Some("") an empty capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match<'s> {
    pub groups: Vec<Option<&'s str>>,
}

#[derive(Clone, Debug)]
enum SetItem {
    Range(u32, u32),
    Digit(bool),
    Word(bool),
    Space(bool),
}
impl SetItem {
    /// Evaluate one scalar against the closed class vocabulary.
    fn accepts(&self, c: char) -> bool {
        match *self {
            Self::Range(a, b) => a <= c as u32 && c as u32 <= b,
            Self::Digit(positive) => c.is_ascii_digit() == positive,
            Self::Word(positive) => word(c) == positive,
            Self::Space(positive) => whitespace(c) == positive,
        }
    }
}
/// Word membership is ASCII-only under REQ-0822.
fn word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
/// ECMA whitespace plus line terminators, without U+0085.
fn whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

#[derive(Clone, Debug)]
enum Reference {
    Number(usize),
    Name(String),
}
#[derive(Clone, Debug)]
enum Kind {
    Literal(u32),
    Dot,
    Class {
        items: Vec<SetItem>,
        negative: bool,
    },
    Start,
    End,
    Boundary(bool),
    Sequence(Vec<usize>),
    Alternative(Vec<usize>),
    Group {
        child: usize,
        number: Option<usize>,
    },
    Repeat {
        child: usize,
        min: usize,
        max: Option<usize>,
        greedy: bool,
    },
    Look {
        child: usize,
        behind: bool,
        positive: bool,
    },
    Backreference(Reference),
}
/// Keep proven variability separate from the unresolved width of a backreference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    Fixed(usize),
    /// Known terms may prove nonempty consumption independently of the reference.
    Backref {
        nonempty: bool,
    },
    Variable,
}

#[derive(Clone, Debug)]
struct Node {
    kind: Kind,
    byte: usize,
    width: Width,
    captures: core::ops::Range<usize>,
}

/// Immutable postorder arena. Compilation never resolves fields or calls a host.
#[derive(Clone, Debug)]
pub struct Pattern {
    nodes: Vec<Node>,
    root: usize,
    groups: usize,
}
impl Pattern {
    /// Parse the closed grammar before any subject is read.
    pub fn compile(source: &str, limits: CompileLimits) -> Result<Self, CompileError> {
        Self::compile_with_budget(source, limits, &mut CompileBudget::new(limits))
    }
    /// Share capture-path compilation work/storage across literals in one request.
    /// Per-pattern ceilings still apply; failed work retains its consumed prefix.
    pub(crate) fn compile_with_budget(
        source: &str,
        limits: CompileLimits,
        shared: &mut CompileBudget,
    ) -> Result<Self, CompileError> {
        parser::compile(source, limits, shared)
    }
    /// Count numbered captures; the whole match is group zero in returned results.
    pub fn group_count(&self) -> usize {
        self.groups
    }
    /// Search leftmost-first with a fresh cumulative resource budget.
    pub fn search<'s>(
        &self,
        subject: &'s str,
        limits: MatchLimits,
    ) -> Result<Option<Match<'s>>, MatchError> {
        self.execute(subject, limits, false)
    }
    /// Anchor the entire pattern, preserving alternatives that fail the final anchor.
    pub fn full_match<'s>(
        &self,
        subject: &'s str,
        limits: MatchLimits,
    ) -> Result<Option<Match<'s>>, MatchError> {
        self.execute(subject, limits, true)
    }
    /// Allocate bounded scalar coordinates and share budgets across candidate starts.
    fn execute<'s>(
        &self,
        subject: &'s str,
        limits: MatchLimits,
        full: bool,
    ) -> Result<Option<Match<'s>>, MatchError> {
        if subject.len() > limits.subject_bytes {
            return Err(MatchError {
                resource: Resource::SubjectBytes,
                limit: limits.subject_bytes,
            });
        }
        let mut budget = Budget {
            limits,
            work: 0,
            cells: 0,
        };
        // Count before allocating scalar storage or its byte-boundary map.
        let size = subject.chars().count();
        budget.cells(
            size.checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .unwrap_or(usize::MAX),
        )?;
        let chars: Vec<_> = subject.chars().collect();
        let offsets: Vec<_> = subject
            .char_indices()
            .map(|(i, _)| i)
            .chain(core::iter::once(subject.len()))
            .collect();
        for start in 0..=if full { 0 } else { chars.len() } {
            budget.work(1)?;
            budget.cells(self.groups + 2)?;
            let state = State {
                at: start,
                tasks: vec![Task::Node(self.root)],
                captures: vec![None; self.groups + 1],
            };
            if let Some(mut found) = self.run(&chars, state, false, full, &mut budget)? {
                found.captures[0] = Some((start, found.at));
                budget.cells(found.captures.len())?;
                let groups = found
                    .captures
                    .iter()
                    .map(|span| span.map(|(a, b)| &subject[offsets[a]..offsets[b]]))
                    .collect();
                return Ok(Some(Match { groups }));
            }
        }
        Ok(None)
    }
    /// Walk explicit backtracking states; only bounded nested assertions recurse.
    fn run(
        &self,
        input: &[char],
        initial: State,
        backwards: bool,
        require_end: bool,
        budget: &mut Budget,
    ) -> Result<Option<State>, MatchError> {
        budget.cells(1)?;
        let mut alternatives = vec![initial];
        while let Some(mut state) = alternatives.pop() {
            let mut failed = false;
            while let Some(task) = state.tasks.pop() {
                budget.work(1)?;
                match task {
                    Task::FinishGroup(number, start) => {
                        state.captures[number] = Some((
                            core::cmp::min(start, state.at),
                            core::cmp::max(start, state.at),
                        ));
                    }
                    Task::RepeatEnd { id, count, before } => {
                        let Kind::Repeat { min, .. } = self.nodes[id].kind else {
                            unreachable!()
                        };
                        // Empty optional iterations fail, restoring the alternative's captures.
                        if state.at == before && count >= min {
                            failed = true;
                            break;
                        }
                        state.push(
                            Task::Repeat {
                                id,
                                count: count + 1,
                            },
                            budget,
                        )?;
                    }
                    Task::Repeat { id, count } => {
                        let Kind::Repeat {
                            child,
                            min,
                            max,
                            greedy,
                        } = self.nodes[id].kind
                        else {
                            unreachable!()
                        };
                        if max.is_some_and(|max| count >= max) {
                            continue;
                        }
                        let mut iteration = state.copy(budget)?;
                        for capture in self.nodes[child].captures.clone() {
                            iteration.captures[capture] = None;
                        }
                        iteration.push(
                            Task::RepeatEnd {
                                id,
                                count,
                                before: state.at,
                            },
                            budget,
                        )?;
                        iteration.push(Task::Node(child), budget)?;
                        if count < min {
                            state = iteration;
                        } else if greedy {
                            budget.cells(1)?;
                            alternatives.push(state);
                            state = iteration;
                        } else {
                            budget.cells(1)?;
                            alternatives.push(iteration);
                        }
                    }
                    Task::Node(id) => match &self.nodes[id].kind {
                        Kind::Sequence(children) => {
                            if backwards {
                                for &child in children {
                                    state.push(Task::Node(child), budget)?;
                                }
                            } else {
                                for &child in children.iter().rev() {
                                    state.push(Task::Node(child), budget)?;
                                }
                            }
                        }
                        Kind::Alternative(children) => {
                            for &child in children[1..].iter().rev() {
                                let mut branch = state.copy(budget)?;
                                branch.push(Task::Node(child), budget)?;
                                budget.cells(1)?;
                                alternatives.push(branch);
                            }
                            state.push(Task::Node(children[0]), budget)?;
                        }
                        Kind::Group { child, number } => {
                            if let Some(number) = number {
                                state.push(Task::FinishGroup(*number, state.at), budget)?;
                            }
                            state.push(Task::Node(*child), budget)?;
                        }
                        Kind::Repeat { .. } => state.push(Task::Repeat { id, count: 0 }, budget)?,
                        Kind::Look {
                            child,
                            behind,
                            positive,
                        } => {
                            let mut probe = state.copy(budget)?;
                            probe.tasks.clear();
                            probe.push(Task::Node(*child), budget)?;
                            let result = self.run(input, probe, *behind, false, budget)?;
                            if result.is_some() != *positive {
                                failed = true;
                                break;
                            }
                            if let Some(found) = result {
                                state.captures = found.captures;
                            }
                        }
                        Kind::Start => {
                            if state.at != 0 {
                                failed = true;
                                break;
                            }
                        }
                        Kind::End => {
                            if state.at != input.len() {
                                failed = true;
                                break;
                            }
                        }
                        Kind::Boundary(positive) => {
                            let left = state.at.checked_sub(1).is_some_and(|i| word(input[i]));
                            let right = input.get(state.at).is_some_and(|c| word(*c));
                            if (left != right) != *positive {
                                failed = true;
                                break;
                            }
                        }
                        Kind::Backreference(Reference::Number(number)) => {
                            if let Some((a, b)) = state.captures[*number] {
                                let length = b - a;
                                let target = if backwards {
                                    state.at.checked_sub(length)
                                } else {
                                    state
                                        .at
                                        .checked_add(length)
                                        .filter(|end| *end <= input.len())
                                        .map(|_| state.at)
                                };
                                let Some(target) = target else {
                                    failed = true;
                                    break;
                                };
                                budget.work(length)?;
                                if input[a..b] != input[target..target + length] {
                                    failed = true;
                                    break;
                                }
                                state.at = if backwards { target } else { target + length };
                            }
                        }
                        Kind::Backreference(Reference::Name(_)) => {
                            unreachable!("names resolved during compilation")
                        }
                        kind => {
                            let index = if backwards {
                                state.at.checked_sub(1)
                            } else {
                                (state.at < input.len()).then_some(state.at)
                            };
                            let Some(index) = index else {
                                failed = true;
                                break;
                            };
                            let c = input[index];
                            let accepts = match kind {
                                Kind::Literal(value) => c as u32 == *value,
                                Kind::Dot => !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'),
                                Kind::Class { items, negative } => {
                                    budget.work(items.len())?;
                                    items.iter().any(|item| item.accepts(c)) != *negative
                                }
                                _ => unreachable!(),
                            };
                            if !accepts {
                                failed = true;
                                break;
                            }
                            state.at = if backwards { index } else { index + 1 };
                        }
                    },
                }
            }
            if !failed && (!require_end || state.at == input.len()) {
                return Ok(Some(state));
            }
        }
        Ok(None)
    }
}

#[derive(Clone, Debug)]
enum Task {
    Node(usize),
    FinishGroup(usize, usize),
    Repeat {
        id: usize,
        count: usize,
    },
    RepeatEnd {
        id: usize,
        count: usize,
        before: usize,
    },
}
#[derive(Clone, Debug)]
struct State {
    at: usize,
    tasks: Vec<Task>,
    captures: Vec<Option<(usize, usize)>>,
}
impl State {
    /// Charge task/capture copies before allocating a backtracking snapshot.
    fn copy(&self, budget: &mut Budget) -> Result<Self, MatchError> {
        budget.cells(self.tasks.len().saturating_add(self.captures.len()))?;
        Ok(self.clone())
    }
    /// Charge every logical continuation slot before extending the task stack.
    fn push(&mut self, task: Task, budget: &mut Budget) -> Result<(), MatchError> {
        budget.cells(1)?;
        self.tasks.push(task);
        Ok(())
    }
}
struct Budget {
    limits: MatchLimits,
    work: usize,
    cells: usize,
}
impl Budget {
    /// Charge cumulative interpretation and scalar-comparison work.
    fn work(&mut self, n: usize) -> Result<(), MatchError> {
        self.work = self
            .work
            .checked_add(n)
            .filter(|v| *v <= self.limits.work)
            .ok_or(MatchError {
                resource: Resource::Work,
                limit: self.limits.work,
            })?;
        Ok(())
    }
    /// Charge cumulative logical storage with overflow-safe arithmetic.
    fn cells(&mut self, n: usize) -> Result<(), MatchError> {
        self.cells = self
            .cells
            .checked_add(n)
            .filter(|v| *v <= self.limits.state_cells)
            .ok_or(MatchError {
                resource: Resource::StateCells,
                limit: self.limits.state_cells,
            })?;
        Ok(())
    }
}
