//! Bounded, host-independent R022 pattern compilation and matching.
//! Shared by schema constraints, predicates and completed-string column checks.

use std::vec;
mod identifier_data;
mod identifiers;
mod parser;
mod widths;

/// Pinned ID_Start / ID_Continue data used for capture-group name admission.
pub const IDENTIFIER_UNICODE_VERSION: &str = "18.0.0";

/// Fixed recursion and regex compilation/matching work boundaries.
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
    fn execute<'s>(&self, subject: &'s str, full: bool) -> Result<Option<Match<'s>>, MatchError> {
        let mut steps = Steps(crate::limits::Limits::REGEX_STEPS);
        // Retain exact Unicode-scalar and UTF-8 byte coordinates.

        let chars: Vec<_> = subject.chars().collect();
        let offsets: Vec<_> = subject
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(subject.len()))
            .collect();
        for start in 0..=if full { 0 } else { chars.len() } {
            steps.work(1)?;
            steps.work(self.groups)?;
            steps.work(1)?;
            let state = State {
                at: start,
                tasks: vec![Task::Node(self.root)],
                captures: vec![None; self.groups + 1],
            };
            if let Some(mut found) = self.run(&chars, state, false, full, &mut steps)? {
                found.captures[0] = Some((start, found.at));

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
        steps: &mut Steps,
    ) -> Result<Option<State>, MatchError> {
        let mut alternatives = vec![initial];
        while let Some(mut state) = alternatives.pop() {
            let mut failed = false;
            while let Some(task) = state.tasks.pop() {
                steps.work(1)?;
                match task {
                    Task::FinishGroup(number, start) => {
                        state.captures[number] = Some((
                            std::cmp::min(start, state.at),
                            std::cmp::max(start, state.at),
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
                        state.tasks.push(Task::Repeat {
                            id,
                            count: count + 1,
                        });
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
                        let mut iteration = state.copy(steps)?;
                        steps.work(self.nodes[child].captures.len())?;
                        for capture in self.nodes[child].captures.clone() {
                            iteration.captures[capture] = None;
                        }
                        iteration.tasks.push(Task::RepeatEnd {
                            id,
                            count,
                            before: state.at,
                        });
                        iteration.tasks.push(Task::Node(child));
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
                        Kind::Sequence(children) => {
                            steps.work(children.len())?;
                            if backwards {
                                for &child in children {
                                    state.tasks.push(Task::Node(child));
                                }
                            } else {
                                for &child in children.iter().rev() {
                                    state.tasks.push(Task::Node(child));
                                }
                            }
                        }
                        Kind::Alternative(children) => {
                            for &child in children[1..].iter().rev() {
                                let mut branch = state.copy(steps)?;
                                branch.tasks.push(Task::Node(child));

                                alternatives.push(branch);
                            }
                            state.tasks.push(Task::Node(children[0]));
                        }
                        Kind::Group { child, number } => {
                            if let Some(number) = number {
                                state.tasks.push(Task::FinishGroup(*number, state.at));
                            }
                            state.tasks.push(Task::Node(*child));
                        }
                        Kind::Repeat { .. } => state.tasks.push(Task::Repeat { id, count: 0 }),
                        Kind::Look {
                            child,
                            behind,
                            positive,
                        } => {
                            let mut probe = state.copy(steps)?;
                            probe.tasks.clear();
                            probe.tasks.push(Task::Node(*child));
                            let result = self.run(input, probe, *behind, false, steps)?;
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
                                steps.work(length)?;
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
                                    steps.work(items.len())?;
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
#[derive(Debug)]
struct State {
    at: usize,
    tasks: Vec<Task>,
    captures: Vec<Option<(usize, usize)>>,
}
impl State {
    /// Reserve the copy work before allocating either backtracking vector.
    fn copy(&self, steps: &mut Steps) -> Result<Self, MatchError> {
        steps.work(self.tasks.len())?;
        steps.work(self.captures.len())?;
        Ok(Self {
            at: self.at,
            tasks: self.tasks.clone(),
            captures: self.captures.clone(),
        })
    }
}
/// One fixed work scope per compilation or match, including copied state.
struct Steps(usize);
impl Steps {
    fn work(&mut self, amount: usize) -> Result<(), MatchError> {
        self.0 = self.0.checked_sub(amount).ok_or(MatchError {
            resource: Resource::Work,
            limit: crate::limits::Limits::REGEX_STEPS,
        })?;
        Ok(())
    }

    fn compile_work(&mut self, amount: usize) -> Result<(), CompileError> {
        self.work(amount).map_err(|error| CompileError::Limit {
            resource: error.resource,
            limit: error.limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_capture_storage_is_refused_before_allocating_it() {
        // Model the requested storage directly; no million-element vector is built.
        let pattern = Pattern {
            nodes: Vec::new(),
            root: 0,
            groups: crate::limits::Limits::REGEX_STEPS,
        };
        assert_eq!(
            pattern.full_match(""),
            Err(MatchError {
                resource: Resource::Work,
                limit: crate::limits::Limits::REGEX_STEPS,
            })
        );
    }

    #[test]
    fn repeated_capture_copies_consume_the_match_work_scope() {
        // All three continuations match the empty string structurally. Their
        // repeats, alternatives and assertions copy the full capture state.
        for continuation in [
            "(){1000}".to_owned(),
            "(?:|)".repeat(1000),
            "(?=)".repeat(1000),
        ] {
            let pattern = Pattern::compile(&("()".repeat(1000) + &continuation)).unwrap();
            assert_eq!(
                pattern.full_match(""),
                Err(MatchError {
                    resource: Resource::Work,
                    limit: crate::limits::Limits::REGEX_STEPS,
                })
            );
        }
        let small = Pattern::compile("(){1000}").unwrap();
        assert_eq!(
            small.full_match("").unwrap().unwrap().groups,
            [Some(""), Some("")]
        );
    }
}
