//! Bounded, host-independent R022 pattern compilation and matching.
//! Shared by schema constraints, predicates and completed-string column checks.

use std::vec;
mod identifier_data;
mod identifiers;
mod parser;
mod widths;

/// Pinned ID_Start / ID_Continue data used for capture-group name admission.
pub const IDENTIFIER_UNICODE_VERSION: &str = "18.0.0";

/// Fixed recursion and per-match interpretation boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Depth,
    Work,
}

/// Invalid syntax owns REQ-0827. Unsupported features never masquerade as invalid syntax.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("regex invalid: byte={byte:?}, reason={reason:?}")]
    Invalid { byte: usize, reason: &'static str },
    #[error("regex limit: resource={resource:?}, limit={limit:?}")]
    Limit { resource: Resource, limit: usize },
    #[error("regex unsupported: byte={byte:?}, feature={feature:?}")]
    Unsupported { byte: usize, feature: &'static str },
}

/// A policy refusal cannot be confused with no match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("regex: resource={resource:?}, limit={limit:?}")]
pub struct MatchError {
    pub resource: Resource,
    pub limit: usize,
}

/// Group zero is the whole match; None is an unentered group, Some("") an empty capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match<'s> {
    pub groups: Vec<Option<&'s str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Reference {
    Number(usize),
    Name(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    kind: Kind,
    byte: usize,
    width: Width,
    captures: std::ops::Range<usize>,
}

/// Immutable postorder arena. Compilation never resolves fields or calls a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    nodes: Vec<Node>,
    root: usize,
    groups: usize,
}
impl Pattern {
    /// Parse the closed grammar before any subject is read.
    pub fn compile(source: &str) -> Result<Self, CompileError> {
        parser::compile(source)
    }
    /// Count numbered captures; the whole match is group zero in returned results.
    pub fn group_count(&self) -> usize {
        self.groups
    }
    /// Search leftmost-first with one fixed step guard for the complete match.
    pub fn search<'s>(&self, subject: &'s str) -> Result<Option<Match<'s>>, MatchError> {
        self.execute(subject, false)
    }
    /// Anchor the entire pattern, preserving alternatives that fail the final anchor.
    pub fn full_match<'s>(&self, subject: &'s str) -> Result<Option<Match<'s>>, MatchError> {
        self.execute(subject, true)
    }
    /// Every match of a left-to-right scan that resumes where the previous
    /// match ended, or one scalar later after an empty match (ECMA-262 global
    /// replacement with the Unicode flag). Spans are subject byte offsets. The
    /// whole scan is one match call under one fixed step guard.
    pub fn search_all(&self, subject: &str) -> Result<Vec<(usize, usize)>, MatchError> {
        let mut machine = self.machine();
        let (chars, offsets) = scalars(subject);
        let mut spans = Vec::new();
        let mut next = 0;
        while next <= chars.len() {
            let Some((start, found)) = self.first(&chars, next, false, &mut machine)? else {
                break;
            };
            spans.push((offsets[start], offsets[found.at]));
            next = if found.at == start {
                start + 1
            } else {
                found.at
            };
        }
        Ok(spans)
    }
    fn execute<'s>(&self, subject: &'s str, full: bool) -> Result<Option<Match<'s>>, MatchError> {
        let mut machine = self.machine();
        let (chars, offsets) = scalars(subject);
        if let Some((start, found)) = self.first(&chars, 0, full, &mut machine)? {
            let mut groups = machine
                .captures
                .materialize(found.captures, |a, b| &subject[offsets[a]..offsets[b]]);
            groups[0] = Some(&subject[offsets[start]..offsets[found.at]]);
            return Ok(Some(Match { groups }));
        }
        Ok(None)
    }
    fn machine(&self) -> Machine {
        Machine {
            steps: Steps(crate::limits::Limits::REGEX_STEPS),
            tasks: Vec::new(),
            captures: Captures {
                groups: self.groups + 1,
                nodes: Vec::new(),
            },
        }
    }
    /// The leftmost successful attempt starting at or after `from`; a full
    /// match tries the start of the subject only.
    fn first(
        &self,
        chars: &[char],
        from: usize,
        full: bool,
        machine: &mut Machine,
    ) -> Result<Option<(usize, State)>, MatchError> {
        for start in from..=if full { 0 } else { chars.len() } {
            machine.steps.work(1)?;
            machine.tasks.clear();
            machine.captures.nodes.clear();
            let mut state = State {
                at: start,
                tasks: None,
                captures: None,
            };
            machine.push(&mut state, Task::Node(self.root));
            if let Some(found) = self.run(chars, state, false, full, machine)? {
                return Ok(Some((start, found)));
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
        machine: &mut Machine,
    ) -> Result<Option<State>, MatchError> {
        let mut alternatives = vec![initial];
        while let Some(mut state) = alternatives.pop() {
            let mut failed = false;
            while let Some(task) = machine.pop(&mut state) {
                machine.steps.work(1)?;
                match task {
                    Task::FinishGroup(number, start) => {
                        state.captures = machine.captures.set(
                            state.captures,
                            number,
                            (start.min(state.at), start.max(state.at)),
                        );
                    }
                    Task::Sequence { id, next } => {
                        let Kind::Sequence(children) = &self.nodes[id].kind else {
                            unreachable!()
                        };
                        if next < children.len() {
                            let child = children[if backwards {
                                children.len() - next - 1
                            } else {
                                next
                            }];
                            if next + 1 < children.len() {
                                machine.push(&mut state, Task::Sequence { id, next: next + 1 });
                            }
                            machine.push(&mut state, Task::Node(child));
                        }
                    }
                    Task::Alternative { id, next } => {
                        let Kind::Alternative(children) = &self.nodes[id].kind else {
                            unreachable!()
                        };
                        if next + 1 < children.len() {
                            let mut branch = state;
                            machine.push(&mut branch, Task::Alternative { id, next: next + 1 });
                            alternatives.push(branch);
                        }
                        machine.push(&mut state, Task::Node(children[next]));
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
                        machine.push(
                            &mut state,
                            Task::Repeat {
                                id,
                                count: count + 1,
                            },
                        );
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
                        let mut iteration = state;
                        iteration.captures = machine
                            .captures
                            .clear(iteration.captures, self.nodes[child].captures.clone());
                        machine.push(
                            &mut iteration,
                            Task::RepeatEnd {
                                id,
                                count,
                                before: state.at,
                            },
                        );
                        machine.push(&mut iteration, Task::Node(child));
                        if count < min {
                            state = iteration;
                        } else if greedy {
                            alternatives.push(state);
                            state = iteration;
                        } else {
                            alternatives.push(iteration);
                        }
                    }
                    Task::Node(id) => match &self.nodes[id].kind {
                        Kind::Sequence(_) => {
                            machine.push(&mut state, Task::Sequence { id, next: 0 })
                        }
                        Kind::Alternative(_) => {
                            machine.push(&mut state, Task::Alternative { id, next: 0 })
                        }
                        Kind::Group { child, number } => {
                            if let Some(number) = number {
                                let start = state.at;
                                machine.push(&mut state, Task::FinishGroup(*number, start));
                            }
                            machine.push(&mut state, Task::Node(*child));
                        }
                        Kind::Repeat { .. } => {
                            machine.push(&mut state, Task::Repeat { id, count: 0 })
                        }
                        Kind::Look {
                            child,
                            behind,
                            positive,
                        } => {
                            let mut probe = State {
                                tasks: None,
                                ..state
                            };
                            machine.push(&mut probe, Task::Node(*child));
                            let result = self.run(input, probe, *behind, false, machine)?;
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
                            if let Some((a, b)) = machine.captures.get(state.captures, *number) {
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
                                machine.steps.work(length)?;
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
                                    machine.steps.work(items.len())?;
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

#[derive(Clone, Copy, Debug)]
enum Task {
    Node(usize),
    FinishGroup(usize, usize),
    Sequence {
        id: usize,
        next: usize,
    },
    Alternative {
        id: usize,
        next: usize,
    },
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
/// Exact Unicode-scalar and UTF-8 byte coordinates of one subject.
fn scalars(subject: &str) -> (Vec<char>, Vec<usize>) {
    let chars = subject.chars().collect();
    let offsets = subject
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(subject.len()))
        .collect();
    (chars, offsets)
}
#[derive(Clone, Copy, Debug)]
struct State {
    at: usize,
    tasks: Option<usize>,
    captures: Option<usize>,
}
/// Backtracking shares immutable task tails and capture roots. Forking a state
/// copies three coordinates, regardless of pending tasks or capture count.
struct Machine {
    steps: Steps,
    tasks: Vec<(Task, Option<usize>)>,
    captures: Captures,
}
impl Machine {
    fn push(&mut self, state: &mut State, task: Task) {
        self.tasks.push((task, state.tasks));
        state.tasks = Some(self.tasks.len() - 1);
    }
    fn pop(&self, state: &mut State) -> Option<Task> {
        let (task, previous) = self.tasks[state.tasks?];
        state.tasks = previous;
        Some(task)
    }
}

#[derive(Clone, Copy)]
enum CaptureNode {
    Branch(Option<usize>, Option<usize>),
    Span(usize, usize),
}
/// A sparse persistent tree needs no initial capture vector. Updates copy only
/// the search path; resetting a repeated group's capture interval drops covered
/// subtrees. Empty roots preserve the distinction between unset and empty spans.
struct Captures {
    groups: usize,
    nodes: Vec<CaptureNode>,
}
impl Captures {
    fn branches(&self, root: Option<usize>) -> (Option<usize>, Option<usize>) {
        match root.map(|root| self.nodes[root]) {
            Some(CaptureNode::Branch(left, right)) => (left, right),
            None => (None, None),
            Some(CaptureNode::Span(..)) => unreachable!("capture leaves have unit width"),
        }
    }
    fn push(&mut self, node: CaptureNode) -> Option<usize> {
        self.nodes.push(node);
        Some(self.nodes.len() - 1)
    }
    fn set(&mut self, root: Option<usize>, group: usize, span: (usize, usize)) -> Option<usize> {
        self.set_at(root, group, span, 0, self.groups)
    }
    fn set_at(
        &mut self,
        root: Option<usize>,
        group: usize,
        span: (usize, usize),
        start: usize,
        end: usize,
    ) -> Option<usize> {
        if end - start == 1 {
            return self.push(CaptureNode::Span(span.0, span.1));
        }
        let middle = start + (end - start) / 2;
        let (mut left, mut right) = self.branches(root);
        if group < middle {
            left = self.set_at(left, group, span, start, middle);
        } else {
            right = self.set_at(right, group, span, middle, end);
        }
        self.push(CaptureNode::Branch(left, right))
    }
    fn clear(&mut self, root: Option<usize>, range: std::ops::Range<usize>) -> Option<usize> {
        self.clear_at(root, &range, 0, self.groups)
    }
    fn clear_at(
        &mut self,
        root: Option<usize>,
        range: &std::ops::Range<usize>,
        start: usize,
        end: usize,
    ) -> Option<usize> {
        if range.start >= end || range.end <= start || root.is_none() {
            return root;
        }
        if range.start <= start && end <= range.end {
            return None;
        }
        let middle = start + (end - start) / 2;
        let (left, right) = self.branches(root);
        let next_left = self.clear_at(left, range, start, middle);
        let next_right = self.clear_at(right, range, middle, end);
        if (left, right) == (next_left, next_right) {
            root
        } else if next_left.is_none() && next_right.is_none() {
            None
        } else {
            self.push(CaptureNode::Branch(next_left, next_right))
        }
    }
    fn get(&self, mut root: Option<usize>, group: usize) -> Option<(usize, usize)> {
        let (mut start, mut end) = (0, self.groups);
        while let Some(id) = root {
            match self.nodes[id] {
                CaptureNode::Span(a, b) => return Some((a, b)),
                CaptureNode::Branch(left, right) => {
                    let middle = start + (end - start) / 2;
                    if group < middle {
                        root = left;
                        end = middle;
                    } else {
                        root = right;
                        start = middle;
                    }
                }
            }
        }
        None
    }
    fn materialize<T: Copy>(
        &self,
        root: Option<usize>,
        mut span: impl FnMut(usize, usize) -> T,
    ) -> Vec<Option<T>> {
        fn fill<T>(
            captures: &Captures,
            root: Option<usize>,
            output: &mut [Option<T>],
            span: &mut impl FnMut(usize, usize) -> T,
        ) {
            let Some(id) = root else { return };
            match captures.nodes[id] {
                CaptureNode::Span(a, b) => output[0] = Some(span(a, b)),
                CaptureNode::Branch(left, right) => {
                    let (a, b) = output.split_at_mut(output.len() / 2);
                    fill(captures, left, a, span);
                    fill(captures, right, b, span);
                }
            }
        }
        let mut output = vec![None; self.groups];
        fill(self, root, &mut output, &mut span);
        output
    }
}
/// The only matching counter spans starts, backtracking and nested lookarounds.
struct Steps(usize);
impl Steps {
    fn work(&mut self, amount: usize) -> Result<(), MatchError> {
        self.0 = self.0.checked_sub(amount).ok_or(MatchError {
            resource: Resource::Work,
            limit: crate::limits::Limits::REGEX_STEPS,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod matching_tests {
    use super::*;

    #[test]
    fn many_captures_share_backtracking_state_and_keep_repeat_participation() {
        let prefix = "(a)".repeat(3000);
        let pattern = Pattern::compile(&(prefix.clone() + r"(?:(a)|(b))+\3001\3002$")).unwrap();
        let subject = "a".repeat(3000) + "abb";
        let found = pattern.full_match(&subject).unwrap().unwrap();
        assert_eq!(found.groups.len(), 3003);
        assert_eq!(found.groups[0], Some(subject.as_str()));
        assert!(found.groups[1..=3000].iter().all(|span| *span == Some("a")));
        assert_eq!(&found.groups[3001..], &[None, Some("b")]);

        let refusing = Pattern::compile(&(prefix + r"(a+)+$")).unwrap();
        assert_eq!(
            refusing.full_match(&("a".repeat(3030) + "!")),
            Err(MatchError {
                resource: Resource::Work,
                limit: crate::limits::Limits::REGEX_STEPS,
            })
        );
        assert!(refusing.full_match(&"a".repeat(3001)).unwrap().is_some());
    }
}
