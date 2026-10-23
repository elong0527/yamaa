//! The handle table: every result, plans holder and table the engine hands
//! out lives in a slot here, behind a handle that names the slot and the
//! slot's generation. Releasing a handle empties its slot and moves the
//! generation on, so the released handle, and any copy of it, no longer
//! finds a value; a slot whose generation would wrap is retired.
//!
//! A lookup clones the value's `Arc` and lets the table go, so an entry never
//! holds the table while it runs: a callback may reenter the engine, and a
//! release drops its value outside the table's lock. The census counts a
//! value until its last owner drops it, which an entry still running may be.
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yamaa_boundary::abi::Handle;
use yamaa_boundary::{Census, HandleKind, Live};

/// The live values of the table, by kind.
pub(crate) static CENSUS: Census = Census::new();

/// What a handle names.
pub(crate) enum Value {
    Check(Mutex<yamaa::Check>),
    Run(Mutex<yamaa::Run>),
    Streamed(Arc<yamaa::Streamed>),
    ProjectRun(Mutex<yamaa::ProjectRun>),
    Submission(Mutex<yamaa::Submission>),
    SelfTest(yamaa::SelfTest),
    Plans(yamaa::Plans),
    Table(yamaa::Table),
    StreamTable(Arc<yamaa::Streamed>),
}

impl Value {
    pub(crate) fn kind(&self) -> HandleKind {
        match self {
            Value::Check(_) => HandleKind::Check,
            Value::Run(_) | Value::Streamed(_) => HandleKind::Run,
            Value::ProjectRun(_) => HandleKind::ProjectRun,
            Value::Submission(_) => HandleKind::Submission,
            Value::SelfTest(_) => HandleKind::SelfTest,
            Value::Plans(_) => HandleKind::Plans,
            Value::Table(_) | Value::StreamTable(_) => HandleKind::Table,
        }
    }
}

/// A value in the table, counted in the census while it lives.
pub(crate) struct Held {
    pub(crate) value: Value,
    _live: Live,
}

struct Slot {
    generation: u32,
    held: Option<Arc<Held>>,
}

struct Slots {
    slots: Vec<Slot>,
    free: Vec<usize>,
}

static SLOTS: Mutex<Slots> = Mutex::new(Slots {
    slots: Vec::new(),
    free: Vec::new(),
});

fn slots() -> MutexGuard<'static, Slots> {
    // No code runs under the lock that could panic halfway through a change.
    SLOTS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The handle of `slot` at `generation`: the slot plus one in the low 32
/// bits, so that 0 is no handle, and the generation in the high ones.
fn encode(slot: usize, generation: u32) -> Handle {
    (u64::from(generation) << 32) | (slot as u64 + 1)
}

fn decode(handle: Handle) -> Option<(usize, u32)> {
    let slot = usize::try_from((handle & u64::from(u32::MAX)).checked_sub(1)?).ok()?;
    Some((slot, (handle >> 32) as u32))
}

/// Hold `value` and return its handle.
pub(crate) fn insert(value: Value) -> Result<Handle, String> {
    let held = Arc::new(Held {
        _live: CENSUS.hold(value.kind()),
        value,
    });
    let mut table = slots();
    let slot = match table.free.pop() {
        Some(slot) => slot,
        None if table.slots.len() < u32::MAX as usize => {
            table.slots.push(Slot {
                generation: 0,
                held: None,
            });
            table.slots.len() - 1
        }
        None => return Err("the engine holds as many handles as it can".to_owned()),
    };
    let entry = &mut table.slots[slot];
    entry.held = Some(held);
    Ok(encode(slot, entry.generation))
}

/// The value `handle` names, while it is live.
pub(crate) fn get(handle: Handle) -> Option<Arc<Held>> {
    let (slot, generation) = decode(handle)?;
    let table = slots();
    let entry = table.slots.get(slot)?;
    if entry.generation == generation {
        entry.held.clone()
    } else {
        None
    }
}

/// Empty the slot of a live `handle` and return its value, which the caller
/// drops once the table is let go; `None` for a handle that is not live.
pub(crate) fn release(handle: Handle) -> Option<Arc<Held>> {
    let (slot, generation) = decode(handle)?;
    let mut table = slots();
    let entry = table.slots.get_mut(slot)?;
    if entry.generation != generation {
        return None;
    }
    let held = entry.held.take()?;
    entry.generation += 1;
    if entry.generation < u32::MAX {
        table.free.push(slot);
    }
    Some(held)
}
