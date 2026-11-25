//! Every match of a replacement scan, found with backward checkpoints (TEXT-11).
//!
//! A search restarted after each match reads past the match it keeps while a
//! preferred alternative is still alive: for `a+z|a` over a run of `a`, every
//! restart reads to the end of the run before it settles for one `a`, so the
//! scan is quadratic in the subject. Above a short subject, a backward pass
//! keeps its state at segment boundaries. A forward walk recomputes one
//! segment backward at a time, recording where each anchored leftmost-first
//! match ends, and keeps the earliest start at or after the scan position.
//! This is the same choice as a restarted search. Each backward column is
//! computed at most twice; keeping checkpoints and one segment takes
//! O(states + sqrt(subject bytes * states)) workspace, apart from the
//! returned match spans (#4954).
//!
//! The value of an NFA state at a position is the end of the first successful
//! path from it, taking epsilon edges in priority order and never entering a
//! state twice at one position: the same preference the crate's own engines
//! give. Consuming one byte reads the successor's value one position later,
//! already computed. A state outside an epsilon cycle takes the first present
//! value among its successors, computed once per position. Inside a cycle,
//! which states a walk has already entered depends on where it entered, so
//! each state the pass needs as an entry walks its cycle afresh, reading the
//! values of the states that leave it. The work per position is bounded by the
//! compiled program, never by the subject; only nested repetitions of patterns
//! that match the empty string make it quadratic in the program.

use regex_automata::nfa::thompson::{self, State, WhichCaptures, NFA};
use regex_automata::util::{primitives::StateID, syntax};

/// No match: the value of a state from which no path succeeds.
const NONE: usize = usize::MAX;

/// The capture-free program of one pattern, with its epsilon cycles.
#[derive(Clone, Debug)]
pub(super) struct Scanner {
    nfa: NFA,
    /// The epsilon strongly connected component of each state.
    component: Vec<u32>,
    /// The states that consume one byte.
    consuming: Vec<StateID>,
}

impl Scanner {
    /// Compile `pattern`, crate syntax the regex crate has already accepted
    /// with captures, so the smaller capture-free program fits its limits.
    pub(super) fn new(pattern: &str, nest_limit: u32, size_limit: usize) -> Self {
        let nfa = thompson::Compiler::new()
            .syntax(syntax::Config::new().nest_limit(nest_limit))
            .configure(
                thompson::Config::new()
                    .which_captures(WhichCaptures::None)
                    .nfa_size_limit(Some(size_limit)),
            )
            .build(pattern)
            .unwrap_or_else(|error| unreachable!("accepted pattern {pattern:?}: {error}"));
        let consuming = (0..nfa.states().len())
            .map(StateID::must)
            .filter(|&id| consumes(&nfa.states()[id]))
            .collect();
        Self {
            component: components(&nfa),
            consuming,
            nfa,
        }
    }

    /// The spans of a left-to-right scan from `first`, the start of the
    /// leftmost match, resuming where each match ends or one scalar later
    /// after an empty one.
    pub(super) fn search_all(&self, subject: &str, first: usize) -> Vec<(usize, usize)> {
        let haystack = subject.as_bytes();
        let states = self.nfa.states().len();
        let segment = segment_size(haystack.len() + 1 - first, states);
        let mut sweep = Sweep::new(self, subject, first);
        let mut checkpoints = Vec::new();
        // A segment needs the column immediately to its right. The first
        // segment is only computed when the forward walk reads it.
        for at in (first + segment..=haystack.len()).rev() {
            sweep.advance(at);
            if (at - first) % segment == 0 {
                checkpoints.push(sweep.later.clone());
            }
        }
        checkpoints.reverse();
        let mut spans = Vec::new();
        let mut at = first;
        while at <= haystack.len() {
            let block = (at - first) / segment;
            let start = first + block * segment;
            let stop = (start + segment).min(haystack.len() + 1);
            sweep.resume(checkpoints.get(block));
            // A match may end beyond this segment, so its length's width
            // is chosen from the remaining subject, not the segment size.
            let mut ends = Ends::new(stop - start, haystack.len() + 1 - start);
            for position in (start..stop).rev() {
                let end = sweep.advance(position);
                if end != NONE {
                    ends.set(position - start, end - position);
                }
            }
            while at < stop {
                let Some(length) = ends.get(at - start) else {
                    at += 1;
                    continue;
                };
                spans.push((at, at + length));
                if length > 0 {
                    at += length;
                } else {
                    match subject[at..].chars().next() {
                        Some(scalar) => at += scalar.len_utf8(),
                        None => return spans,
                    }
                }
            }
        }
        spans
    }
}

/// Balance a segment's endpoint table against its state checkpoints. Each
/// checkpoint holds two words per state; ceil(sqrt(positions / words))
/// segments keep both terms sublinear in positions for a fixed program.
fn segment_size(positions: usize, states: usize) -> usize {
    let quotient = positions.div_ceil(states.saturating_mul(2).max(1));
    let (mut low, mut high) = (1, quotient);
    while low < high {
        let middle = low + (high - low) / 2;
        if middle >= quotient.div_ceil(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    positions.div_ceil(low)
}

/// The match length at each position of one segment, plus one, zero for
/// none. A length may reach the end of the whole remaining subject.
enum Ends {
    Narrow(Vec<u32>),
    Wide(Vec<usize>),
}

impl Ends {
    fn new(positions: usize, remaining: usize) -> Self {
        if u32::try_from(remaining).is_ok() {
            Self::Narrow(vec![0; positions])
        } else {
            Self::Wide(vec![0; positions])
        }
    }
    fn set(&mut self, index: usize, length: usize) {
        match self {
            Self::Narrow(ends) => {
                ends[index] = u32::try_from(length + 1)
                    .expect("a length below the remaining subject positions")
            }
            Self::Wide(ends) => ends[index] = length + 1,
        }
    }
    fn get(&self, index: usize) -> Option<usize> {
        let stored = match self {
            Self::Narrow(ends) => ends[index] as usize,
            Self::Wide(ends) => ends[index],
        };
        stored.checked_sub(1)
    }
}

/// The values of states at one position, valid where the stamp names it.
#[derive(Clone)]
struct Column {
    stamp: Vec<usize>,
    value: Vec<usize>,
}

impl Column {
    fn new(states: usize) -> Self {
        Self {
            stamp: vec![0; states],
            value: vec![NONE; states],
        }
    }
    fn get(&self, id: StateID, at: usize) -> Option<usize> {
        (self.stamp[id] == at + 1).then(|| self.value[id])
    }
    fn set(&mut self, id: StateID, at: usize, value: usize) {
        self.stamp[id] = at + 1;
        self.value[id] = value;
    }
}

/// A reusable backward sweep. A checkpoint is its `later` column; the walk
/// and byte-class successors depend on the program, not the segment.
struct Sweep<'a> {
    scanner: &'a Scanner,
    subject: &'a str,
    first: usize,
    current: Column,
    later: Column,
    walk: Walk,
    successors: Vec<Option<Vec<StateID>>>,
}

impl<'a> Sweep<'a> {
    fn new(scanner: &'a Scanner, subject: &'a str, first: usize) -> Self {
        let states = scanner.nfa.states().len();
        Self {
            scanner,
            subject,
            first,
            current: Column::new(states),
            later: Column::new(states),
            walk: Walk {
                entered: vec![0; states],
                token: 0,
                frames: Vec::new(),
                items: Vec::new(),
            },
            successors: vec![None; scanner.nfa.byte_classes().alphabet_len()],
        }
    }

    fn resume(&mut self, later: Option<&Column>) {
        self.current.stamp.fill(0);
        if let Some(later) = later {
            self.later.stamp.clone_from(&later.stamp);
            self.later.value.clone_from(&later.value);
        } else {
            self.later.stamp.fill(0);
        }
    }

    /// Compute one column and return the anchored match end at a scalar
    /// boundary, or NONE. Also prepare every state the byte to its left
    /// can consume, including when a checkpoint splits a UTF-8 scalar.
    fn advance(&mut self, at: usize) -> usize {
        let haystack = self.subject.as_bytes();
        let mut position = Position {
            scanner: self.scanner,
            haystack,
            at,
            current: &mut self.current,
            later: &self.later,
            walk: &mut self.walk,
        };
        if at > self.first {
            let byte = haystack[at - 1];
            let class = self.scanner.nfa.byte_classes().get(byte);
            let read = self.successors[usize::from(class)].get_or_insert_with(|| {
                self.scanner
                    .consuming
                    .iter()
                    .filter_map(|&id| step(&self.scanner.nfa.states()[id], byte))
                    .collect()
            });
            for &next in read.iter() {
                position.value(next);
            }
        }
        let end = if self.subject.is_char_boundary(at) {
            position.value(self.scanner.nfa.start_anchored())
        } else {
            NONE
        };
        std::mem::swap(&mut self.current, &mut self.later);
        end
    }
}

/// Reusable buffers of the walks of one scan.
struct Walk {
    /// The token of the frame that last entered each state.
    entered: Vec<u64>,
    token: u64,
    frames: Vec<Frame>,
    items: Vec<Item>,
}

/// The walk of one component from its entry state.
struct Frame {
    entry: StateID,
    component: u32,
    token: u64,
    /// The index of the entry's item.
    base: usize,
}

/// A state on the walk and the next of its successors to try.
struct Item {
    state: StateID,
    edge: usize,
    frame: usize,
}

/// One position of the backward pass.
struct Position<'a> {
    scanner: &'a Scanner,
    haystack: &'a [u8],
    at: usize,
    current: &'a mut Column,
    later: &'a Column,
    walk: &'a mut Walk,
}

impl Position<'_> {
    /// The end of the first successful path from `root` at this position.
    fn value(&mut self, root: StateID) -> usize {
        if let Some(value) = self.current.get(root, self.at) {
            return value;
        }
        if let Some(value) = self.leaf(root) {
            self.current.set(root, self.at, value);
            return value;
        }
        self.open(root);
        loop {
            let top = self.walk.items.len() - 1;
            let Item { state, edge, frame } = self.walk.items[top];
            let Some(next) = self.edge(state, edge) else {
                if let Some(value) = self.close(NONE) {
                    return value;
                }
                continue;
            };
            self.walk.items[top].edge += 1;
            let found = if let Some(value) = self.leaf(next) {
                value
            } else if self.scanner.component[next] == self.walk.frames[frame].component {
                let token = self.walk.frames[frame].token;
                if self.walk.entered[next] != token {
                    self.walk.entered[next] = token;
                    self.walk.items.push(Item {
                        state: next,
                        edge: 0,
                        frame,
                    });
                }
                NONE
            } else if let Some(value) = self.current.get(next, self.at) {
                value
            } else {
                self.open(next);
                NONE
            };
            // Every earlier path of every state on the walk has failed, so
            // the first success is the value of each state that led to it.
            if found != NONE {
                while self.walk.items.len() > 1 {
                    self.close(found);
                }
                return self.close(found).expect("the root closes last");
            }
        }
    }

    /// Start walking the component of `entry` from it.
    fn open(&mut self, entry: StateID) {
        self.walk.token += 1;
        let token = self.walk.token;
        self.walk.entered[entry] = token;
        self.walk.frames.push(Frame {
            entry,
            component: self.scanner.component[entry],
            token,
            base: self.walk.items.len(),
        });
        self.walk.items.push(Item {
            state: entry,
            edge: 0,
            frame: self.walk.frames.len() - 1,
        });
    }

    /// Finish the top state with `value`, recording it when the state entered
    /// its component; the root's value once the walk is empty.
    fn close(&mut self, value: usize) -> Option<usize> {
        self.walk.items.pop();
        let frame = self.walk.frames.last().expect("an open frame");
        if frame.base == self.walk.items.len() {
            self.current.set(frame.entry, self.at, value);
            self.walk.frames.pop();
        }
        self.walk.items.is_empty().then_some(value)
    }

    /// The `index`th epsilon successor of `state` open at this position.
    fn edge(&self, state: StateID, index: usize) -> Option<StateID> {
        let state = &self.scanner.nfa.states()[state];
        if let State::Look { look, .. } = state {
            let open = self
                .scanner
                .nfa
                .look_matcher()
                .matches(*look, self.haystack, self.at);
            if !open {
                return None;
            }
        }
        epsilon(state, index)
    }

    /// The value of a state with no epsilon successor; None for any other.
    fn leaf(&self, id: StateID) -> Option<usize> {
        let state = &self.scanner.nfa.states()[id];
        match state {
            State::Match { .. } => Some(self.at),
            State::Fail => Some(NONE),
            _ if consumes(state) => Some(
                self.haystack
                    .get(self.at)
                    .and_then(|&byte| step(state, byte))
                    .map_or(NONE, |next| {
                        self.later
                            .get(next, self.at + 1)
                            .expect("the successor of a consumed byte is computed")
                    }),
            ),
            _ => None,
        }
    }
}

fn consumes(state: &State) -> bool {
    matches!(
        state,
        State::ByteRange { .. } | State::Sparse(_) | State::Dense(_)
    )
}

/// The successor of a consuming state on `byte`.
fn step(state: &State, byte: u8) -> Option<StateID> {
    match state {
        State::ByteRange { trans } => trans.matches_byte(byte).then_some(trans.next),
        State::Sparse(sparse) => sparse.matches_byte(byte),
        State::Dense(dense) => dense.matches_byte(byte),
        _ => None,
    }
}

/// The `index`th epsilon successor of `state` in priority order, whatever
/// its assertion reads.
fn epsilon(state: &State, index: usize) -> Option<StateID> {
    match state {
        State::Union { alternates } => alternates.get(index).copied(),
        State::BinaryUnion { alt1, alt2 } => [*alt1, *alt2].get(index).copied(),
        State::Look { next, .. } | State::Capture { next, .. } => (index == 0).then_some(*next),
        _ => None,
    }
}

/// The epsilon strongly connected components, by an iterative Tarjan walk.
fn components(nfa: &NFA) -> Vec<u32> {
    const UNSEEN: u32 = u32::MAX;
    let states = nfa.states();
    let mut index = vec![UNSEEN; states.len()];
    let mut low = vec![0; states.len()];
    let mut stacked = vec![false; states.len()];
    let mut component = vec![UNSEEN; states.len()];
    let mut stack = Vec::new();
    let mut calls: Vec<(usize, usize)> = Vec::new();
    let (mut counter, mut count) = (0, 0);
    for root in 0..states.len() {
        if index[root] != UNSEEN {
            continue;
        }
        index[root] = counter;
        low[root] = counter;
        counter += 1;
        stack.push(root);
        stacked[root] = true;
        calls.push((root, 0));
        while let Some(&mut (state, ref mut edge)) = calls.last_mut() {
            if let Some(next) = epsilon(&states[state], *edge) {
                *edge += 1;
                let next = next.as_usize();
                if index[next] == UNSEEN {
                    index[next] = counter;
                    low[next] = counter;
                    counter += 1;
                    stack.push(next);
                    stacked[next] = true;
                    calls.push((next, 0));
                } else if stacked[next] {
                    low[state] = low[state].min(index[next]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[state]);
            }
            if low[state] == index[state] {
                loop {
                    let member = stack.pop().expect("the component's root is stacked");
                    stacked[member] = false;
                    component[member] = count;
                    if member == state {
                        break;
                    }
                }
                count += 1;
            }
        }
    }
    component
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_segment_keeps_match_lengths_past_its_boundary() {
        // Seam: an endpoint table holds lengths into later segments, which
        // a fixture cannot exercise above 4 GiB without that allocation.
        // Such a length is impossible on a 32-bit target.
        for length in [Some(127), (u32::MAX as usize).checked_add(1)]
            .into_iter()
            .flatten()
        {
            let mut ends = Ends::new(2, length + 1);
            ends.set(0, length);
            assert_eq!(
                (ends.get(0), ends.get(1)),
                (Some(length), None),
                "one match of {length} bytes beyond the segment"
            );
        }
    }
}
