//! Every match of a replacement scan, found in one backward pass (TEXT-11).
//!
//! A search restarted after each match reads past the match it keeps while a
//! preferred alternative is still alive: for `a+z|a` over a run of `a`, every
//! restart reads to the end of the run before it settles for one `a`, so the
//! scan is quadratic in the subject. Above a short subject, one pass from the
//! end of the subject to its start records, at every scalar boundary, where
//! the anchored leftmost-first match starting there ends. A forward walk then
//! keeps the earliest recorded start at or after the scan position, exactly
//! as a restarted search would, since a search from a position keeps the
//! anchored match at the earliest start that has one.
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
        let mut ends = Ends::new(haystack.len() + 1 - first);
        let mut walk = Walk {
            entered: vec![0; states],
            token: 0,
            frames: Vec::new(),
            items: Vec::new(),
        };
        let mut current = Column::new(states);
        let mut later = Column::new(states);
        // The successors of the consuming states that read each byte class,
        // listed when the class is first read.
        let classes = self.nfa.byte_classes();
        let mut successors: Vec<Option<Vec<StateID>>> = vec![None; classes.alphabet_len()];
        for at in (first..=haystack.len()).rev() {
            let mut position = Position {
                scanner: self,
                haystack,
                at,
                current: &mut current,
                later: &later,
                walk: &mut walk,
            };
            if at > first {
                let byte = haystack[at - 1];
                let read = successors[usize::from(classes.get(byte))].get_or_insert_with(|| {
                    self.consuming
                        .iter()
                        .filter_map(|&id| step(&self.nfa.states()[id], byte))
                        .collect()
                });
                for &next in read.iter() {
                    position.value(next);
                }
            }
            if subject.is_char_boundary(at) {
                let end = position.value(self.nfa.start_anchored());
                if end != NONE {
                    ends.set(at - first, end - at);
                }
            }
            std::mem::swap(&mut current, &mut later);
        }
        let mut spans = Vec::new();
        let mut at = first;
        while at <= haystack.len() {
            let Some(length) = ends.get(at - first) else {
                at += 1;
                continue;
            };
            spans.push((at, at + length));
            if length > 0 {
                at += length;
            } else {
                match subject[at..].chars().next() {
                    Some(scalar) => at += scalar.len_utf8(),
                    None => break,
                }
            }
        }
        spans
    }
}

/// The length of the match starting at each position from the first, plus
/// one, zero for none: four bytes per subject byte below 4 GiB.
enum Ends {
    Narrow(Vec<u32>),
    Wide(Vec<usize>),
}

impl Ends {
    fn new(positions: usize) -> Self {
        if u32::try_from(positions).is_ok() {
            Self::Narrow(vec![0; positions])
        } else {
            Self::Wide(vec![0; positions])
        }
    }
    fn set(&mut self, index: usize, length: usize) {
        match self {
            Self::Narrow(ends) => {
                ends[index] = u32::try_from(length + 1).expect("a length below the positions")
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
